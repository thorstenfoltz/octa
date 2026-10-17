use super::*;
use crate::data::summary::{SummaryStat, build_summary_table};
use crate::data::{CellValue, ColumnInfo};
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
    t.rows = (0..40)
        .map(|i| {
            vec![
                if i % 7 == 0 {
                    CellValue::Null
                } else {
                    CellValue::Int(i % 9)
                },
                if i % 5 == 0 {
                    CellValue::String(String::new())
                } else {
                    CellValue::String(format!("v{}", i % 3))
                },
            ]
        })
        .collect();
    t
}

/// Stats both paths compute exactly. Quartiles are left out: SUMMARIZE's
/// are approximate, the server's interpolate; the next test pins those.
#[test]
fn matches_the_in_memory_summary_on_the_same_rows() {
    let t = table();
    let exact = [
        SummaryStat::Min,
        SummaryStat::Max,
        SummaryStat::Sum,
        SummaryStat::Mean,
        SummaryStat::Std,
        SummaryStat::Mode,
        SummaryStat::ModeCount,
        SummaryStat::NotNullCount,
        SummaryStat::NullCount,
        SummaryStat::NullPercent,
        SummaryStat::UniqueCount,
        SummaryStat::DistinctRatio,
        SummaryStat::TextLenMin,
        SummaryStat::TextLenMax,
        SummaryStat::TotalRows,
    ];
    let local = build_summary_table(&t, &exact).unwrap();
    let mut c = DuckConn::new(t.clone());
    let (server, parts) = run(
        &mut c,
        &source(),
        &t,
        t.row_count(),
        &exact,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(parts.is_empty());
    assert_eq!(server.columns.len(), local.columns.len());
    // Column 1 is the type: SUMMARIZE says BIGINT, the server path says
    // the tab's own Int64. Every statistic column must agree.
    for r in 0..local.row_count() {
        for col in 2..local.col_count() {
            let (a, b) = (local.get(r, col).unwrap(), server.get(r, col).unwrap());
            match (cell_f64(a), cell_f64(b)) {
                (Some(x), Some(y)) => assert!(
                    (x - y).abs() < 1e-9,
                    "r{r} {}: {x} vs {y}",
                    local.columns[col].name
                ),
                _ => assert_eq!(
                    a.to_string(),
                    b.to_string(),
                    "r{r} {}",
                    local.columns[col].name
                ),
            }
        }
    }
}

/// A float's text length is Octa's form ("3.0"), whatever the server
/// writes, and the note names the parts.
#[test]
fn float_text_length_comes_from_the_loaded_rows() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "f".into(),
        data_type: "Float64".into(),
    }];
    t.rows = [3.0, 12.5, 100.0]
        .iter()
        .map(|v| vec![CellValue::Float(*v)])
        .collect();
    let stats = [SummaryStat::TextLenMin, SummaryStat::TextLenMax];
    let want = build_summary_table(&t, &stats).unwrap();
    let mut c = DuckConn::new(t.clone());
    let (got, parts) = run(&mut c, &source(), &t, 3, &stats, &AtomicBool::new(false)).unwrap();
    for col in 2..want.col_count() {
        assert_eq!(
            got.get(0, col).unwrap().to_string(),
            want.get(0, col).unwrap().to_string(),
            "{}",
            want.columns[col].name
        );
    }
    assert_eq!(
        parts.by_design,
        vec!["text_len_min, text_len_max (Float)".to_string()]
    );
    assert!(
        c.log
            .iter()
            .all(|q| !q.contains("LENGTH(") && !q.contains("length(")),
        "{:?}",
        c.log
    );
}

#[test]
fn quartiles_interpolate_on_the_server() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "x".into(),
        data_type: "Int64".into(),
    }];
    t.rows = (1..=4).map(|i| vec![CellValue::Int(i)]).collect();
    let stats = [
        SummaryStat::Q25,
        SummaryStat::Median,
        SummaryStat::Q75,
        SummaryStat::Iqr,
    ];
    let mut c = DuckConn::new(t.clone());
    let (s, _) = run(&mut c, &source(), &t, 4, &stats, &AtomicBool::new(false)).unwrap();
    let v = |col: usize| cell_f64(s.get(0, col).unwrap()).unwrap();
    // Columns follow the SummaryStat enum order: median, iqr, q25, q75.
    assert_eq!((v(2), v(3), v(4), v(5)), (2.5, 1.5, 1.75, 3.25));
}

/// A refused mode query blanks its cell; a refused quartile query falls back to the loaded rows.
#[test]
fn a_refused_mode_or_quartile_query_keeps_the_rest() {
    let t = table();
    let stats = [
        SummaryStat::Min,
        SummaryStat::Mode,
        SummaryStat::Median,
        SummaryStat::NullCount,
    ];
    let mut c = DuckConn::new(t.clone());
    c.fail_on = vec!["octa_v", "PERCENTILE_CONT"];
    let (s, parts) = run(
        &mut c,
        &source(),
        &t,
        t.row_count(),
        &stats,
        &AtomicBool::new(false),
    )
    .unwrap();
    // The refused quartile query is the server's refusal, not the engine's gap.
    assert!(parts.engine.is_empty());
    assert_eq!(parts.failed, vec!["median".to_string()]);
    let col = |name: &str| s.columns.iter().position(|c| c.name == name).unwrap();
    // Row 0 is the numeric column `n`; the aggregate scan still answers.
    let local = build_summary_table(&t, &stats).unwrap();
    for id in ["null_count", "min"] {
        assert_eq!(
            s.get(0, col(id)).unwrap().to_string(),
            local.get(0, col(id)).unwrap().to_string(),
            "{id}"
        );
    }
    // Mode stays blank; the refused median is filled from the loaded rows.
    assert_eq!(s.get(0, col("mode")).unwrap().to_string(), "");
    assert_ne!(local.get(0, col("mode")).unwrap().to_string(), "");
    assert_eq!(
        s.get(0, col("median")).unwrap().to_string(),
        local.get(0, col("median")).unwrap().to_string()
    );
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
fn sql_server_sql_shape() {
    let mut src = source();
    src.conn.engine = DbEngine::Mssql;
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "x".into(),
        data_type: "Int64".into(),
    }];
    let mut c = Recorder(DbEngine::Mssql, vec![]);
    run(
        &mut c,
        &src,
        &t,
        0,
        &[SummaryStat::Sum, SummaryStat::Mode],
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(c.1.iter().any(|s| s.contains("SUM(CAST(")), "{:?}", c.1);
    assert!(
        c.1.iter()
            .any(|s| s.ends_with(" OFFSET 0 ROWS FETCH NEXT 1 ROWS ONLY")),
        "{:?}",
        c.1
    );
}
