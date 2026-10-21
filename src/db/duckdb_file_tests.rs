use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::data::CellValue;
use crate::db::{DbConnection, DbConnector, DbEngine};

pub(super) fn sample_file(dir: &Path) -> PathBuf {
    let path = dir.join("shop.duckdb");
    duckdb::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE SCHEMA archive;
             CREATE TABLE items (id INTEGER PRIMARY KEY, name VARCHAR NOT NULL, price DOUBLE, d DATE);
             INSERT INTO items VALUES (1, 'pen', 1.5, DATE '2024-03-07'), (2, 'ink', NULL, NULL);
             CREATE VIEW cheap AS SELECT * FROM items WHERE price < 2;",
        )
        .unwrap();
    path
}

pub(super) fn connector(path: &Path, writes: bool) -> Box<dyn DbConnector> {
    let conn = DbConnection::file(
        "d".into(),
        "shop",
        DbEngine::DuckDb,
        &path.to_string_lossy(),
        writes,
    );
    crate::db::connect(&conn, None, None).unwrap()
}

#[test]
fn lists_the_files_schemas_and_tables() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = connector(&sample_file(dir.path()), false);
    assert_eq!(c.list_schemas(None).unwrap(), vec!["archive", "main"]);
    assert_eq!(c.list_tables(None, "main").unwrap(), vec!["cheap", "items"]);
}

#[test]
fn a_query_carries_the_result_types() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = connector(&sample_file(dir.path()), false);
    let t = c
        .query("SELECT id, name, price, d FROM items ORDER BY id")
        .unwrap();
    let types: Vec<&str> = t.columns.iter().map(|c| c.data_type.as_str()).collect();
    assert_eq!(types, ["Int32", "Utf8", "Float64", "Date32"]);
    assert_eq!(t.rows[0][1], CellValue::String("pen".into()));
    assert_eq!(t.rows[0][3].to_string(), "2024-03-07");
    assert_eq!(t.rows[1][2], CellValue::Null);
}

#[test]
fn a_read_only_connection_is_refused_by_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = connector(&sample_file(dir.path()), false);
    let err = c.execute("DELETE FROM items").unwrap_err();
    assert!(format!("{err:#}").contains("read-only"), "{err:#}");
}

#[test]
fn a_file_that_is_no_database_fails_at_the_first_action() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.duckdb");
    std::fs::write(&path, "hello, this is text and no database at all").unwrap();
    let mut c = connector(&path, false);
    let err = c.query("SELECT 1").unwrap_err();
    assert!(format!("{err:#}").contains("not a valid DuckDB"), "{err:#}");
}

#[test]
fn a_transaction_holds_one_connection_until_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = connector(&sample_file(dir.path()), true);
    c.execute("BEGIN").unwrap();
    c.execute("INSERT INTO items VALUES (3, 'cap', 2.0, NULL)")
        .unwrap();
    let inside = c.query("SELECT count(*) FROM items").unwrap();
    assert_eq!(inside.rows[0][0], CellValue::Int(3));
    c.execute("ROLLBACK").unwrap();
    let after = c.query("SELECT count(*) FROM items").unwrap();
    assert_eq!(
        after.rows[0][0],
        CellValue::Int(2),
        "ROLLBACK undid the insert"
    );
}

#[test]
fn cancel_stops_a_long_query() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = connector(&sample_file(dir.path()), false);
    let cancel = c.cancel_handle().expect("a file connection can cancel");
    let done = Arc::new(AtomicBool::new(false));
    let finished = done.clone();
    let stopper = std::thread::spawn(move || {
        while !finished.load(Ordering::SeqCst) {
            cancel();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    });
    let res = c.query("SELECT sum(i * i % 7) FROM range(10000000000000) t(i)");
    done.store(true, Ordering::SeqCst);
    stopper.join().unwrap();
    let err = res.expect_err("an endless query has to be stopped");
    assert!(
        format!("{err:#}").to_lowercase().contains("interrupt"),
        "{err:#}"
    );
}

use crate::db::write_back::{RowIdentity, apply_write_back, build_write_back_plan};

#[test]
fn an_edited_tab_saves_back_and_a_blob_survives() {
    let dir = tempfile::tempdir().unwrap();
    let path = sample_file(dir.path());
    duckdb::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE files (id INTEGER PRIMARY KEY, body BLOB, label VARCHAR);
             INSERT INTO files VALUES (1, from_hex('00ff10'), 'a');",
        )
        .unwrap();
    let mut c = connector(&path, true);
    let mut t = c.query("SELECT * FROM main.files ORDER BY id").unwrap();
    let original = t
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| (i as i64, r.clone()))
        .collect();
    t.db_meta = Some(crate::data::DbRowMeta {
        table_name: "files".into(),
        schema: Some("main".into()),
        row_tags: vec![Some(0)],
        original,
        original_columns: t.columns.iter().map(|c| c.name.clone()).collect(),
    });
    t.rows[0][2] = CellValue::String("b".into());
    // A new row carries a blob through sql_literal.
    t.rows.push(vec![
        CellValue::Int(2),
        CellValue::Binary(vec![1, 2]),
        CellValue::String("c".into()),
    ]);
    t.db_meta.as_mut().unwrap().row_tags.push(None);
    let identity = RowIdentity::Key(vec!["id".into()]);
    let plan = build_write_back_plan(&t, &identity).unwrap();
    apply_write_back(
        &mut *c,
        DbEngine::DuckDb,
        "main",
        "files",
        &t.columns,
        &identity,
        &plan,
    )
    .unwrap();
    let back = c
        .query("SELECT body, label FROM files ORDER BY id")
        .unwrap();
    assert_eq!(back.rows[0][0], CellValue::Binary(vec![0, 255, 16]));
    assert_eq!(back.rows[0][1], CellValue::String("b".into()));
    assert_eq!(back.rows[1][0], CellValue::Binary(vec![1, 2]));
}

use crate::data::summary::{SummaryStat, build_summary_table};
use crate::data::value_frequency::{BinningMode, compute_value_frequency};
use crate::data::{ColumnInfo, DataTable};
use crate::db::DbWriteMode;
use crate::db::pushdown::{ServerSource, cell_f64};

/// The rows of `summary_tests.rs`, written into a real DuckDB file.
fn analysed() -> (
    tempfile::TempDir,
    Box<dyn DbConnector>,
    DataTable,
    ServerSource,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.duckdb");
    drop(duckdb::Connection::open(&path).unwrap());
    let mut c = connector(&path, true);
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
    c.write_table(None, "main", "data", DbWriteMode::Create, &t)
        .unwrap();
    let src = ServerSource {
        conn: DbConnection::file(
            "d".into(),
            "a",
            DbEngine::DuckDb,
            &path.to_string_lossy(),
            false,
        ),
        catalog: None,
        schema: "main".into(),
        table: "data".into(),
        filter: None,
        derived: Vec::new(),
    };
    (dir, c, t, src)
}

#[test]
fn the_server_summary_equals_the_one_on_the_loaded_rows() {
    let (_dir, mut c, t, src) = analysed();
    let stats = [
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
    let local = build_summary_table(&t, &stats).unwrap();
    let (server, parts) = crate::db::pushdown::summary::run(
        &mut *c,
        &src,
        &t,
        t.row_count(),
        &stats,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(parts.is_empty(), "{parts:?}");
    for r in 0..local.row_count() {
        for col in 2..local.col_count() {
            let (a, b) = (local.get(r, col).unwrap(), server.get(r, col).unwrap());
            match (cell_f64(a), cell_f64(b)) {
                (Some(x), Some(y)) => assert!((x - y).abs() < 1e-9, "r{r} c{col}: {x} vs {y}"),
                _ => assert_eq!(a.to_string(), b.to_string(), "r{r} c{col}"),
            }
        }
    }
}

#[test]
fn the_server_value_counts_equal_the_ones_on_the_loaded_rows() {
    let (_dir, mut c, t, src) = analysed();
    for col in 0..2 {
        let local = compute_value_frequency(&t, col, None, BinningMode::None).unwrap();
        let server = crate::db::pushdown::value_frequency::run(
            &mut *c,
            &src,
            &t.columns[col],
            None,
            BinningMode::None,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            (local.nulls, local.total_non_null, local.unique_count),
            (server.nulls, server.total_non_null, server.unique_count),
            "column {col}"
        );
        let rows = |v: &crate::data::value_frequency::ValueFrequency| {
            v.rows
                .iter()
                .map(|r| (r.label.clone(), r.count))
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(&local), rows(&server), "column {col}");
    }
}

#[test]
fn explain_plans_on_the_file_without_running() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = connector(&sample_file(dir.path()), false);
    let plan =
        crate::sql::explain::explain_db(&mut *c, "SELECT name FROM items WHERE price > 1").unwrap();
    assert!(!plan.steps.is_empty(), "no steps");
    assert!(!plan.server_text.trim().is_empty(), "no server text");
}
