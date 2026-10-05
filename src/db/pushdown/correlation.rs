//! Correlation on the server.
//!
//! Pearson is one statement: `CORR` (or the six sums) for every column pair.
//! Spearman is the Pearson correlation of average ranks, which must be taken
//! over the rows where *both* columns are present. Ranking each column once
//! over its own non-NULL rows is the same thing exactly when the two columns
//! are NULL in the same rows, which a count query tells us; those pairs share
//! one statement, and only the rest get a statement of their own.
//!
//! The caller picks the columns (`numeric_columns` over the loaded rows, so a
//! NUMERIC that reaches the tab as text still takes part). A column whose
//! server values do not cast to a number makes the server raise, which surfaces.
//!
//! Derived-table aliases are `octa_*` so they never collide with a source
//! column (ClickHouse resolves aliases ahead of columns).

use std::sync::atomic::AtomicBool;

use crate::data::ColumnInfo;
use crate::data::correlation::{CorrMatrix, CorrMethod};
use crate::db::{DbConnector, DbEngine};

use super::dialect::{as_number, avg_rank, pearson_read, pearson_select, subquery_alias};
use super::{MAX_SELECT_ITEMS, ServerSource, cell_i64, check_cancel, width_chunks};

/// Answer `pairs` with one statement over `x(i)` per column, from `target`.
fn pair_statement(
    c: &mut dyn DbConnector,
    e: DbEngine,
    target: &str,
    pairs: &[(usize, usize)],
    x: &dyn Fn(usize) -> String,
    matrix: &mut [Vec<Option<f64>>],
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    if pairs.is_empty() {
        return Ok(());
    }
    let sel: Vec<(String, usize)> = pairs
        .iter()
        .map(|&(i, j)| pearson_select(e, &x(i), &x(j)))
        .collect();
    // A wide table has n*(n+1)/2 pairs: split them under the select cap.
    let widths: Vec<usize> = sel.iter().map(|(_, w)| *w).collect();
    for range in width_chunks(&widths) {
        let body = sel[range.clone()]
            .iter()
            .map(|(s, _)| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        check_cancel(cancel)?;
        let t = c.query(&format!("SELECT {body} FROM {target}"))?;
        let mut at = 0;
        for (&(i, j), (_, w)) in pairs[range.clone()].iter().zip(&sel[range]) {
            let r = pearson_read(&t, at, *w);
            matrix[i][j] = r;
            matrix[j][i] = r;
            at += w;
        }
    }
    Ok(())
}

pub fn run(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    columns: &[ColumnInfo],
    method: CorrMethod,
    cancel: &AtomicBool,
) -> anyhow::Result<CorrMatrix> {
    let e = src.engine();
    let from = src.from_sql();
    let quoted: Vec<String> = columns.iter().map(|c| e.quote_ident(&c.name)).collect();
    // Each column as a double; a Boolean as 1.0/0.0 (see `as_number`).
    let num: Vec<String> = columns
        .iter()
        .zip(&quoted)
        .map(|(c, q)| as_number(e, q, &c.data_type))
        .collect();
    let n = columns.len();
    let mut matrix = vec![vec![None; n]; n];
    let pairs: Vec<(usize, usize)> = (0..n).flat_map(|i| (i..n).map(move |j| (i, j))).collect();
    let names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
    if n == 0 {
        return Ok(CorrMatrix {
            columns: names,
            matrix,
        });
    }

    match method {
        CorrMethod::Pearson => {
            let x = |i: usize| num[i].clone();
            pair_statement(c, e, &from, &pairs, &x, &mut matrix, cancel)?;
        }
        CorrMethod::Spearman => {
            // Non-NULL count per column, then per pair.
            let mut counts: Vec<String> = quoted.iter().map(|q| format!("COUNT({q})")).collect();
            counts.extend(pairs.iter().map(|&(i, j)| {
                format!(
                    "SUM(CASE WHEN {} IS NOT NULL AND {} IS NOT NULL THEN 1 ELSE 0 END)",
                    quoted[i], quoted[j]
                )
            }));
            let mut got: Vec<i64> = Vec::with_capacity(counts.len());
            for chunk in counts.chunks(MAX_SELECT_ITEMS) {
                check_cancel(cancel)?;
                let t = c.query(&format!("SELECT {} FROM {from}", chunk.join(", ")))?;
                got.extend((0..chunk.len()).map(|k| t.get(0, k).and_then(cell_i64).unwrap_or(0)));
            }
            let k = |col: usize| got[col];
            let (same, apart): (Vec<_>, Vec<_>) = pairs
                .iter()
                .enumerate()
                .partition(|(p, (i, j))| k(n + p) == k(*i) && k(n + p) == k(*j));
            let same: Vec<(usize, usize)> = same.into_iter().map(|(_, p)| *p).collect();
            let apart: Vec<(usize, usize)> = apart.into_iter().map(|(_, p)| *p).collect();

            if !same.is_empty() {
                let ranks: Vec<String> = num
                    .iter()
                    .enumerate()
                    .map(|(i, x)| format!("{} AS octa_r{i}", avg_rank(e, x)))
                    .collect();
                let target = format!(
                    "(SELECT {} FROM {from}){}",
                    ranks.join(", "),
                    subquery_alias(e, "octa_s")
                );
                let r = |i: usize| format!("octa_r{i}");
                pair_statement(c, e, &target, &same, &r, &mut matrix, cancel)?;
            }

            // ponytail: one statement per pair whose NULLs fall in different
            // rows; a wide table with scattered NULLs is many round trips.
            // UNION ALL them into one statement if that ever hurts.
            for &(i, j) in &apart {
                let (qi, qj) = (&quoted[i], &quoted[j]);
                let target = format!(
                    "(SELECT {} AS octa_rx, {} AS octa_ry FROM {from} \
                     WHERE {qi} IS NOT NULL AND {qj} IS NOT NULL){}",
                    avg_rank(e, &num[i]),
                    avg_rank(e, &num[j]),
                    subquery_alias(e, "octa_s")
                );
                let xy = |k: usize| if k == i { "octa_rx" } else { "octa_ry" }.to_string();
                pair_statement(c, e, &target, &[(i, j)], &xy, &mut matrix, cancel)?;
            }
        }
    }
    Ok(CorrMatrix {
        columns: names,
        matrix,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::correlation::{correlation_matrix, numeric_columns};
    use crate::data::{CellValue, DataTable};
    use crate::db::pushdown::test_support::{DuckConn, source};
    use crate::db::{DbEngine, DbWriteMode, DbWriteReport};

    /// Ties, and NULLs in *different* rows of each column: the case where
    /// ranking each column on its own would be wrong, so the pair query runs.
    fn table() -> DataTable {
        let mut t = DataTable::empty();
        t.columns = ["a", "b", "c", "d"]
            .iter()
            .map(|n| ColumnInfo {
                name: (*n).into(),
                data_type: "Float64".into(),
            })
            .collect();
        t.rows = (0..30)
            .map(|i| {
                let f = i as f64;
                vec![
                    if i % 8 == 3 {
                        CellValue::Null
                    } else {
                        CellValue::Float((f * 1.3) % 7.0)
                    },
                    if i % 5 == 1 {
                        CellValue::Null
                    } else {
                        CellValue::Float(f * 0.5 + (i % 3) as f64)
                    },
                    CellValue::Float(((i * i) % 11) as f64),
                    // NULL in exactly the rows `a` is, numeric elsewhere.
                    if i % 8 == 3 {
                        CellValue::Null
                    } else {
                        CellValue::Float(((i * 7) % 5) as f64 - 1.5)
                    },
                ]
            })
            .collect();
        t
    }

    fn assert_close(a: &CorrMatrix, b: &CorrMatrix) {
        assert_eq!(a.columns, b.columns);
        for i in 0..a.columns.len() {
            for j in 0..a.columns.len() {
                match (a.matrix[i][j], b.matrix[i][j]) {
                    (Some(x), Some(y)) => assert!((x - y).abs() < 1e-9, "[{i}][{j}] {x} vs {y}"),
                    (x, y) => assert_eq!(x, y, "[{i}][{j}]"),
                }
            }
        }
    }

    #[test]
    fn pearson_and_spearman_match_the_in_memory_matrix() {
        let t = table();
        for method in [CorrMethod::Pearson, CorrMethod::Spearman] {
            let local = correlation_matrix(&t, method);
            let mut c = DuckConn::new(t.clone());
            let cols = picked(&t);
            let server = run(&mut c, &source(), &cols, method, &AtomicBool::new(false)).unwrap();
            assert_close(&local, &server);
        }
    }

    fn picked(t: &DataTable) -> Vec<ColumnInfo> {
        numeric_columns(t)
            .into_iter()
            .map(|i| t.columns[i].clone())
            .collect()
    }

    /// A Boolean column takes part as 1/0, as in memory. DuckDB would cast
    /// it either way; the Postgres spelling it gets here is the `CASE`
    /// Postgres needs, so this pins that the `CASE` gives the same numbers.
    #[test]
    fn a_boolean_column_correlates_as_one_and_zero() {
        let mut t = DataTable::empty();
        t.columns = ["flag", "n"]
            .iter()
            .zip(["Boolean", "Float64"])
            .map(|(n, d)| ColumnInfo {
                name: (*n).into(),
                data_type: d.into(),
            })
            .collect();
        t.rows = (0..20)
            .map(|i| {
                vec![
                    if i % 6 == 5 {
                        CellValue::Null
                    } else {
                        CellValue::Bool(i % 3 != 0)
                    },
                    CellValue::Float((i * 7 % 11) as f64),
                ]
            })
            .collect();
        assert_eq!(numeric_columns(&t), vec![0, 1]);
        for method in [CorrMethod::Pearson, CorrMethod::Spearman] {
            let local = correlation_matrix(&t, method);
            let mut c = DuckConn::new(t.clone());
            let server = run(
                &mut c,
                &source(),
                &picked(&t),
                method,
                &AtomicBool::new(false),
            )
            .unwrap();
            assert!(
                c.log
                    .iter()
                    .any(|q| q.contains("CASE WHEN \"flag\" THEN 1.0")),
                "{:?}",
                c.log
            );
            assert!(server.matrix[0][1].is_some());
            assert_close(&local, &server);
        }
    }

    #[test]
    fn spearman_ranks_numeric_text_by_value() {
        let mut t = DataTable::empty();
        t.columns = ["s", "n"]
            .iter()
            .zip(["Utf8", "Float64"])
            .map(|(n, d)| ColumnInfo {
                name: (*n).into(),
                data_type: d.into(),
            })
            .collect();
        t.rows = (0..12)
            .map(|i| {
                vec![
                    CellValue::String((i + 5).to_string()),
                    // Monotone in i: numeric ranks give 1.0, text order
                    // ("10" < "5") gives about -0.47.
                    CellValue::Float(i as f64),
                ]
            })
            .collect();
        let local = correlation_matrix(&t, CorrMethod::Spearman);
        let mut c = DuckConn::new(t.clone());
        let server = run(
            &mut c,
            &source(),
            &picked(&t),
            CorrMethod::Spearman,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_close(&local, &server);
    }

    #[test]
    fn numeric_text_is_picked_and_words_are_not() {
        let mut t = DataTable::empty();
        t.columns = ["money", "words"]
            .iter()
            .map(|n| ColumnInfo {
                name: (*n).into(),
                data_type: "Utf8".into(),
            })
            .collect();
        t.rows = ["1.50", "2.25", "3.00", "4.75"]
            .iter()
            .zip(["red", "green", "blue", "pink"])
            .map(|(m, w)| vec![CellValue::String((*m).into()), CellValue::String(w.into())])
            .collect();
        assert_eq!(numeric_columns(&t), vec![0]);
    }

    /// Records every statement and answers with an empty table.
    struct Recorder(DbEngine, Vec<String>);

    impl DbConnector for Recorder {
        fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn query(&mut self, sql: &str) -> anyhow::Result<DataTable> {
            self.1.push(sql.to_string());
            Ok(DataTable::empty())
        }
        fn engine(&self) -> DbEngine {
            self.0
        }
        fn execute(&mut self, _: &str) -> anyhow::Result<u64> {
            Ok(0)
        }
        fn write_table(
            &mut self,
            _: Option<&str>,
            _: &str,
            _: &str,
            _: DbWriteMode,
            _: &DataTable,
        ) -> anyhow::Result<DbWriteReport> {
            anyhow::bail!("recorder")
        }
    }

    #[test]
    fn wide_pearson_is_chunked_under_the_select_cap() {
        let cols: Vec<ColumnInfo> = (0..50)
            .map(|i| ColumnInfo {
                name: format!("c{i}"),
                data_type: "Float64".into(),
            })
            .collect();
        let mut c = Recorder(DbEngine::Oracle, vec![]);
        let mut src = source();
        src.conn.engine = DbEngine::Oracle;
        run(
            &mut c,
            &src,
            &cols,
            CorrMethod::Pearson,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(c.1.len() > 1, "{}", c.1.len());
        for q in &c.1 {
            assert!(q.matches("CORR(").count() <= MAX_SELECT_ITEMS);
        }
    }

    #[test]
    fn mysql_pearson_uses_sums_not_corr() {
        let t = table();
        let mut c = Recorder(DbEngine::MySql, vec![]);
        let mut src = source();
        src.conn.engine = DbEngine::MySql;
        run(
            &mut c,
            &src,
            &t.columns,
            CorrMethod::Pearson,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(c.1[0].contains("SUM(CASE WHEN"), "{:?}", c.1);
        assert!(!c.1[0].contains("CORR("), "{:?}", c.1);
    }
}
