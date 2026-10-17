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
