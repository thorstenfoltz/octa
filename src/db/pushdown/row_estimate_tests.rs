use super::super::test_support::{DuckConn, source};
use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn src(engine: DbEngine, catalog: Option<&str>) -> ServerSource {
    let mut s = source();
    s.conn.engine = engine;
    s.catalog = catalog.map(str::to_string);
    s.schema = "sales".into();
    s.table = "o'rders".into();
    s
}

#[test]
fn each_engine_asks_its_own_catalog() {
    use DbEngine::*;
    let has = |e, cat, needle: &str| {
        let sql = estimate_sql(&src(e, cat)).unwrap_or_default();
        assert!(sql.contains(needle), "{e:?}: {sql}");
    };
    has(Postgres, None, "c.relname = 'o''rders'");
    has(Postgres, None, "pg_catalog.pg_class");
    has(Postgres, None, "SELECT c.reltuples FROM");
    has(Redshift, None, "svv_table_info");
    has(Redshift, None, "SELECT tbl_rows FROM");
    has(MySql, None, "information_schema.TABLES");
    has(MySql, None, "SELECT TABLE_ROWS FROM");
    has(Mssql, None, "p.index_id IN (0, 1)");
    has(Mssql, None, "SELECT SUM(p.rows) FROM");
    has(Oracle, None, "ALL_TABLES");
    has(Oracle, None, "SELECT NUM_ROWS FROM");
    has(ClickHouse, None, "system.tables");
    has(ClickHouse, None, "SELECT total_rows FROM");
    has(Exasol, None, "EXA_ALL_TABLES");
    has(Exasol, None, "SELECT TABLE_ROW_COUNT FROM");
    has(Snowflake, Some("DB"), "\"DB\".INFORMATION_SCHEMA.TABLES");
    has(Snowflake, Some("DB"), "SELECT ROW_COUNT FROM");
    has(BigQuery, Some("proj"), "`proj`.`sales`.__TABLES__");
    has(BigQuery, Some("proj"), "SELECT row_count FROM");
    has(Trino, None, "SHOW STATS FOR");
    assert_eq!(estimate_sql(&src(Athena, None)), None);
    assert_eq!(estimate_sql(&src(Databricks, None)), None);
    assert!(exact_metadata(Snowflake) && exact_metadata(ClickHouse) && exact_metadata(Exasol));
    assert!(!exact_metadata(Postgres));
}

fn one_cell(v: CellValue) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "n".into(),
        data_type: "Float64".into(),
    }];
    t.rows = vec![vec![v]];
    t
}

#[test]
fn a_never_analysed_table_has_no_estimate() {
    assert_eq!(
        read_estimate(DbEngine::Postgres, &one_cell(CellValue::Float(-1.0))),
        None
    );
    assert_eq!(
        read_estimate(DbEngine::Oracle, &one_cell(CellValue::Null)),
        None
    );
    assert_eq!(
        read_estimate(DbEngine::Postgres, &one_cell(CellValue::Float(0.0))),
        None
    );
    assert_eq!(
        read_estimate(DbEngine::MySql, &one_cell(CellValue::Int(4812))),
        Some(4812)
    );
    assert_eq!(read_estimate(DbEngine::MySql, &DataTable::empty()), None);
}

#[test]
fn a_figure_sent_as_text_is_read() {
    // Oracle, ClickHouse and Redshift send their counts as text.
    let text = |s: &str| one_cell(CellValue::String(s.into()));
    assert_eq!(read_estimate(DbEngine::Oracle, &text("4812")), Some(4812));
    assert_eq!(read_estimate(DbEngine::ClickHouse, &text("-1")), None);
}

#[test]
fn trino_reads_the_summary_row() {
    let mut t = DataTable::empty();
    t.columns = [
        "column_name",
        "data_size",
        "distinct_values_count",
        "nulls_fraction",
        "row_count",
    ]
    .iter()
    .map(|n| ColumnInfo {
        name: (*n).into(),
        data_type: "Utf8".into(),
    })
    .collect();
    t.rows = vec![
        vec![
            CellValue::String("id".into()),
            CellValue::Null,
            CellValue::Float(9.0),
            CellValue::Float(0.0),
            CellValue::Null,
        ],
        vec![
            CellValue::Null,
            CellValue::Null,
            CellValue::Null,
            CellValue::Null,
            CellValue::Float(1234.0),
        ],
    ];
    assert_eq!(read_estimate(DbEngine::Trino, &t), Some(1234));
}

#[test]
fn a_refused_catalog_query_falls_back_to_count() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "a".into(),
        data_type: "Int64".into(),
    }];
    t.rows = (0..7).map(|i| vec![CellValue::Int(i)]).collect();
    let mut c = DuckConn::new(t);
    c.fail_on = vec!["pg_class"];
    let got = row_count(
        &mut c,
        &source(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        got,
        RowCount {
            rows: 7,
            estimate: false
        }
    );
    assert!(c.log.iter().any(|q| q.contains("COUNT(*)")));
}

/// A catalog view holding one row for `sales.data`, laid out as
/// `[figure, schema, table]` under `cols`.
fn catalog_view(cols: [&str; 3], figure: i64) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: cols[0].into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: cols[1].into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: cols[2].into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = vec![vec![
        CellValue::Int(figure),
        CellValue::String("sales".into()),
        CellValue::String("data".into()),
    ]];
    t
}

/// Seven real rows in `data`, so a catalog figure that differs from 7
/// proves which of the two answered. Oracle and Exasol name their catalog
/// views without a schema, so DuckDB can stand in for them.
fn conn_with_catalog(view: &'static str, cols: [&str; 3], figure: i64) -> DuckConn {
    let mut data = DataTable::empty();
    data.columns = vec![ColumnInfo {
        name: "a".into(),
        data_type: "Int64".into(),
    }];
    data.rows = (0..7).map(|i| vec![CellValue::Int(i)]).collect();
    DuckConn::with_tables(data, vec![(view, catalog_view(cols, figure))])
}

/// `sales.data`, the table `catalog_view` describes.
fn data_src(engine: DbEngine) -> ServerSource {
    ServerSource {
        table: "data".into(),
        ..src(engine, None)
    }
}

fn counted(c: &DuckConn) -> bool {
    c.log.iter().any(|q| q.contains("COUNT(*)"))
}

#[test]
fn the_catalog_figure_is_used_and_flagged_as_an_estimate() {
    let mut c = conn_with_catalog("ALL_TABLES", ["NUM_ROWS", "OWNER", "TABLE_NAME"], 123_456);
    let got = row_count(
        &mut c,
        &data_src(DbEngine::Oracle),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        got,
        RowCount {
            rows: 123_456,
            estimate: true
        }
    );
    assert!(!counted(&c), "{:?}", c.log);
}

#[test]
fn an_exact_catalog_figure_is_not_flagged_as_an_estimate() {
    let mut c = conn_with_catalog(
        "EXA_ALL_TABLES",
        ["TABLE_ROW_COUNT", "TABLE_SCHEMA", "TABLE_NAME"],
        98_765,
    );
    let got = row_count(
        &mut c,
        &data_src(DbEngine::Exasol),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        got,
        RowCount {
            rows: 98_765,
            estimate: false
        }
    );
    assert!(!counted(&c), "{:?}", c.log);
}
