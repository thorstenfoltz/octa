//! Value frequency on the server: totals in one scan, then either the top
//! values (`GROUP BY` the text form) or, for a binned numeric column, its
//! range and a `GROUP BY FLOOR(...)` over equal-width bins. Labels are the
//! server's text form of each value, which can differ from Octa's for dates
//! and decimals.

use std::sync::atomic::AtomicBool;

use crate::data::value_frequency::{
    BinningMode, MAX_CUSTOM_BUCKETS, ValueFrequency, ValueFrequencyRow, bin_index, bin_rows,
    sturges_bin_count,
};
use crate::data::{ColumnInfo, is_numeric_data_type};
use crate::db::DbConnector;

use super::dialect::{as_float, exact, float_lit, limit_clause, present, subquery_alias, text};
use super::{ServerSource, cell_f64, cell_i64, check_cancel};

pub fn run(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    col: &ColumnInfo,
    top_n: Option<usize>,
    binning: BinningMode,
    cancel: &AtomicBool,
) -> anyhow::Result<ValueFrequency> {
    let e = src.engine();
    let from = src.from_sql();
    let q = e.quote_ident(&col.name);
    let v = text(e, &q);
    // Counted by the exact text, as in memory and as a value filter compares:
    // MySQL's and SQL Server's default collations fold case and accents.
    let ex = exact(e, &v);
    let has = present(e, &q);

    check_cancel(cancel)?;
    let totals = c.query(&format!(
        "SELECT COUNT(*), SUM(CASE WHEN {has} THEN 1 ELSE 0 END), \
         COUNT(DISTINCT CASE WHEN {has} THEN {ex} END) FROM {from}"
    ))?;
    let at = |k: usize| totals.get(0, k).and_then(cell_i64).unwrap_or(0).max(0) as usize;
    let (total, non_null, distinct) = (at(0), at(1), at(2));
    let mut out = ValueFrequency {
        column_name: col.name.clone(),
        rows: Vec::new(),
        nulls: total.saturating_sub(non_null),
        total_non_null: non_null,
        unique_count: distinct,
        binned: false,
    };

    if is_numeric_data_type(&col.data_type) && binning.bins_numerics() {
        let f = as_float(e, &q);
        check_cancel(cancel)?;
        let range = c.query(&format!(
            "SELECT COUNT({q}), MIN({f}), MAX({f}) FROM {from}"
        ))?;
        let n = range.get(0, 0).and_then(cell_i64).unwrap_or(0).max(0) as usize;
        if let (true, Some(lo), Some(hi)) = (
            n > 0,
            range.get(0, 1).and_then(cell_f64),
            range.get(0, 2).and_then(cell_f64),
        ) {
            let bins = match binning {
                BinningMode::Custom(k) => k.clamp(1, MAX_CUSTOM_BUCKETS),
                _ => sturges_bin_count(n),
            };
            let counts = if (hi - lo).abs() < f64::EPSILON {
                vec![n]
            } else {
                let width = (hi - lo) / bins as f64;
                let b = format!("FLOOR(({f} - {}) / {})", float_lit(lo), float_lit(width));
                check_cancel(cancel)?;
                let t = c.query(&format!(
                    "SELECT octa_b, COUNT(*) FROM (SELECT {b} AS octa_b FROM {from} WHERE {q} IS NOT NULL){} GROUP BY octa_b",
                    subquery_alias(e, "s")
                ))?;
                let mut counts = vec![0usize; bins];
                for r in 0..t.row_count() {
                    let (Some(idx), Some(k)) = (
                        t.get(r, 0).and_then(cell_f64),
                        t.get(r, 1).and_then(cell_i64),
                    ) else {
                        continue;
                    };
                    // FLOOR puts `max` one past the last bin; fold it back.
                    let i = bin_index(lo + idx * width + width / 2.0, lo, width, bins);
                    counts[i] += k.max(0) as usize;
                }
                counts
            };
            out.rows = bin_rows(lo, hi, &counts);
            out.unique_count = out.rows.len();
            out.binned = true;
            return Ok(out);
        }
    }

    let limit = top_n.unwrap_or_else(crate::formats::initial_load_rows);
    check_cancel(cancel)?;
    let t = c.query(&format!(
        "SELECT MIN({v}) AS octa_v, COUNT(*) AS octa_n FROM {from} WHERE {has} GROUP BY {ex} ORDER BY octa_n DESC, octa_v{}",
        limit_clause(e, limit)
    ))?;
    out.rows = (0..t.row_count())
        .filter_map(|r| {
            Some(ValueFrequencyRow {
                label: t.get(r, 0)?.to_string(),
                count: t.get(r, 1).and_then(cell_i64)?.max(0) as usize,
            })
        })
        .collect();
    // The server picks which ties survive the LIMIT; what came back is
    // re-sorted the way Octa orders ties, by label.
    out.rows
        .sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::value_frequency::compute_value_frequency;
    use crate::data::{CellValue, DataTable};
    use crate::db::pushdown::test_support::{DuckConn, source};
    use crate::db::{DbEngine, DbWriteMode, DbWriteReport};

    fn table() -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "n".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "s".into(),
                data_type: "Utf8".into(),
            },
        ];
        t.rows = (0..50)
            .map(|i| {
                vec![
                    if i % 11 == 0 {
                        CellValue::Null
                    } else {
                        CellValue::Int((i * 7) % 23)
                    },
                    if i % 6 == 0 {
                        CellValue::String(String::new())
                    } else {
                        CellValue::String(format!("k{}", i % 4))
                    },
                ]
            })
            .collect();
        t
    }

    fn same(a: &ValueFrequency, b: &ValueFrequency) {
        assert_eq!(
            (a.nulls, a.total_non_null, a.unique_count, a.binned),
            (b.nulls, b.total_non_null, b.unique_count, b.binned)
        );
        let rows = |v: &ValueFrequency| {
            v.rows
                .iter()
                .map(|r| (r.label.clone(), r.count))
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(a), rows(b));
    }

    #[test]
    fn raw_values_match_the_in_memory_count() {
        let t = table();
        for col in 0..2 {
            for top in [None, Some(3)] {
                let local = compute_value_frequency(&t, col, top, BinningMode::None).unwrap();
                let mut c = DuckConn::new(t.clone());
                let server = run(
                    &mut c,
                    &source(),
                    &t.columns[col],
                    top,
                    BinningMode::None,
                    &AtomicBool::new(false),
                )
                .unwrap();
                same(&local, &server);
            }
        }
    }

    #[test]
    fn bins_match_the_in_memory_count() {
        let t = table();
        for binning in [BinningMode::Sturges, BinningMode::Custom(4)] {
            let local = compute_value_frequency(&t, 0, None, binning).unwrap();
            let mut c = DuckConn::new(t.clone());
            let server = run(
                &mut c,
                &source(),
                &t.columns[0],
                None,
                binning,
                &AtomicBool::new(false),
            )
            .unwrap();
            same(&local, &server);
        }
    }

    /// Records every statement and answers with a canned one-row table.
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
            let mut t = DataTable::empty();
            t.columns = vec![
                ColumnInfo {
                    name: "a".into(),
                    data_type: "Int64".into(),
                },
                ColumnInfo {
                    name: "b".into(),
                    data_type: "Int64".into(),
                },
                ColumnInfo {
                    name: "c".into(),
                    data_type: "Int64".into(),
                },
            ];
            t.rows = vec![vec![
                CellValue::Int(1),
                CellValue::Int(1),
                CellValue::Int(1),
            ]];
            Ok(t)
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
    fn sql_server_top_values_use_fetch_next() {
        let mut src = source();
        src.conn.engine = DbEngine::Mssql;
        let col = ColumnInfo {
            name: "s".into(),
            data_type: "Utf8".into(),
        };
        let mut c = Recorder(DbEngine::Mssql, vec![]);
        run(
            &mut c,
            &src,
            &col,
            Some(7),
            BinningMode::None,
            &AtomicBool::new(false),
        )
        .unwrap();
        let last = c.1.last().unwrap();
        assert!(
            last.ends_with(" OFFSET 0 ROWS FETCH NEXT 7 ROWS ONLY"),
            "{last}"
        );
    }

    /// MySQL's and SQL Server's default collations fold case and accents, so
    /// grouping by the plain text would merge `Apple` and `apple` into one
    /// label, which a value filter (exact) then splits again.
    #[test]
    fn values_are_counted_by_their_exact_text() {
        let col = ColumnInfo {
            name: "s".into(),
            data_type: "Utf8".into(),
        };
        for (e, ex) in [
            (DbEngine::MySql, "CAST("),
            (DbEngine::Mssql, "COLLATE Latin1_General_100_BIN2"),
        ] {
            let mut src = source();
            src.conn.engine = e;
            let mut c = Recorder(e, vec![]);
            run(
                &mut c,
                &src,
                &col,
                Some(7),
                BinningMode::None,
                &AtomicBool::new(false),
            )
            .unwrap();
            let (totals, list) = (&c.1[0], c.1.last().unwrap());
            assert!(
                totals.contains("COUNT(DISTINCT") && totals.contains(ex),
                "{e:?}: {totals}"
            );
            let group = &list[list.find("GROUP BY").unwrap()..];
            assert!(group.contains(ex), "{e:?}: {list}");
            assert!(list.starts_with("SELECT MIN("), "{e:?}: {list}");
        }
    }
}
