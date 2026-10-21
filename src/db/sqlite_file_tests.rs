use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::db::{DbConnection, DbConnector, DbEngine, DbWriteMode};

fn sample_file(dir: &Path) -> PathBuf {
    let path = dir.join("shop.sqlite");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, price REAL, note BLOB);
             INSERT INTO items VALUES (1, 'pen', 1.5, X'00FF'), (2, 'ink', NULL, NULL);
             CREATE VIEW cheap AS SELECT * FROM items WHERE price < 2;",
        )
        .unwrap();
    path
}

fn connector(path: &Path, writes: bool) -> Box<dyn DbConnector> {
    let conn = DbConnection::file(
        "s".into(),
        "shop",
        DbEngine::Sqlite,
        &path.to_string_lossy(),
        writes,
    );
    crate::db::connect(&conn, None, None).unwrap()
}

#[test]
fn lists_main_and_its_tables_and_views() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = connector(&sample_file(dir.path()), false);
    assert_eq!(c.list_schemas(None).unwrap(), vec!["main"]);
    assert_eq!(c.list_tables(None, "main").unwrap(), vec!["cheap", "items"]);
}

#[test]
fn a_query_keeps_declared_types_and_values() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = connector(&sample_file(dir.path()), false);
    let t = c
        .query("SELECT id, name, price, note, price * 2 AS twice FROM items ORDER BY id")
        .unwrap();
    let types: Vec<&str> = t.columns.iter().map(|c| c.data_type.as_str()).collect();
    assert_eq!(types, ["Int64", "Utf8", "Float64", "Binary", "Float64"]);
    assert_eq!(
        t.rows[0],
        vec![
            CellValue::Int(1),
            CellValue::String("pen".into()),
            CellValue::Float(1.5),
            CellValue::Binary(vec![0, 255]),
            CellValue::Float(3.0),
        ]
    );
    assert_eq!(t.rows[1][2], CellValue::Null);
}

#[test]
fn a_read_only_connection_is_refused_by_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = connector(&sample_file(dir.path()), false);
    let err = c.execute("DELETE FROM items").unwrap_err();
    assert!(format!("{err:#}").contains("readonly"), "{err:#}");
}

#[test]
fn a_file_that_is_no_database_fails_at_the_first_action() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.sqlite");
    std::fs::write(&path, "hello, this is text and no database at all").unwrap();
    let mut c = connector(&path, false);
    let err = c.query("SELECT 1").unwrap_err();
    assert!(format!("{err:#}").contains("not a database"), "{err:#}");
}

#[test]
fn writes_land_in_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = sample_file(dir.path());
    let mut c = connector(&path, true);
    c.execute("INSERT INTO items (id, name) VALUES (3, 'cap')")
        .unwrap();
    let n: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("SELECT count(*) FROM items", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3);
}

#[test]
fn write_table_creates_then_appends() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = connector(&sample_file(dir.path()), true);
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "Label".into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::Int(1), CellValue::String("it's".into())],
        vec![CellValue::Int(2), CellValue::Null],
    ];
    c.write_table(None, "main", "fresh", DbWriteMode::Create, &t)
        .unwrap();
    c.write_table(None, "main", "fresh", DbWriteMode::Append, &t)
        .unwrap();
    let back = c
        .query("SELECT \"Label\" FROM fresh ORDER BY rowid")
        .unwrap();
    assert_eq!(back.row_count(), 4);
    assert_eq!(back.rows[0][0], CellValue::String("it's".into()));
}

#[test]
fn cancel_stops_a_long_query() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = connector(&sample_file(dir.path()), false);
    let cancel = c.cancel_handle().expect("a file connection can cancel");
    let done = Arc::new(AtomicBool::new(false));
    let finished = done.clone();
    // Keep asking until the query is over: an interrupt sent before the
    // file is open has nothing to stop.
    let stopper = std::thread::spawn(move || {
        while !finished.load(Ordering::SeqCst) {
            cancel();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    });
    let res = c.query(
        "WITH RECURSIVE r(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM r) \
         SELECT count(*) FROM r",
    );
    done.store(true, Ordering::SeqCst);
    stopper.join().unwrap();
    let err = res.expect_err("an endless query has to be stopped");
    assert!(format!("{err:#}").contains("interrupt"), "{err:#}");
}

use crate::db::write_back::{RowIdentity, apply_write_back, build_write_back_plan};
use crate::db::{RowKeyCandidate, choose_row_key, row_key_sql};

/// The sidebar's row-key lookup, as `db_browser` runs it.
fn row_key(c: &mut dyn DbConnector, table: &str) -> Vec<String> {
    let t = c
        .query(&row_key_sql(DbEngine::Sqlite, None, "main", table))
        .unwrap();
    let rows: Vec<RowKeyCandidate> = t
        .rows
        .iter()
        .map(|r| RowKeyCandidate {
            constraint_type: r[0].to_string(),
            constraint_name: r[1].to_string(),
            column_name: r[2].to_string(),
            nullable: r[3].to_string().eq_ignore_ascii_case("YES"),
        })
        .collect();
    choose_row_key(&rows)
}

#[test]
fn the_row_key_is_the_primary_key_then_a_plain_unique_index() {
    let dir = tempfile::tempdir().unwrap();
    let path = sample_file(dir.path());
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE pair (y INT NOT NULL, z INT NOT NULL, PRIMARY KEY (y, z));
             CREATE TABLE coded (a INT NOT NULL, b TEXT NOT NULL, c INT NOT NULL);
             CREATE UNIQUE INDEX coded_expr ON coded (a, lower(b));
             CREATE UNIQUE INDEX coded_part ON coded (c) WHERE c > 0;
             CREATE UNIQUE INDEX coded_ok ON coded (c, a);
             CREATE TABLE loose (v INT);",
        )
        .unwrap();
    let mut c = connector(&path, false);
    assert_eq!(row_key(&mut *c, "items"), ["id"]);
    assert_eq!(row_key(&mut *c, "pair"), ["y", "z"], "in key order");
    // An expression index and a partial index do not address one row.
    assert_eq!(row_key(&mut *c, "coded"), ["c", "a"]);
    assert!(row_key(&mut *c, "loose").is_empty());
}

/// Load `items` the way a writable sidebar tab does.
fn loaded(c: &mut dyn DbConnector) -> DataTable {
    let mut t = c.query("SELECT * FROM main.items ORDER BY id").unwrap();
    let original = t
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| (i as i64, r.clone()))
        .collect();
    t.db_meta = Some(crate::data::DbRowMeta {
        table_name: "items".into(),
        schema: Some("main".into()),
        row_tags: (0..t.rows.len()).map(|i| Some(i as i64)).collect(),
        original,
        original_columns: t.columns.iter().map(|c| c.name.clone()).collect(),
    });
    t
}

#[test]
fn an_edited_tab_saves_back_by_key_and_by_full_row() {
    for identity in [
        RowIdentity::Key(vec!["id".into()]),
        RowIdentity::FullRow(vec![
            "id".into(),
            "name".into(),
            "price".into(),
            "note".into(),
        ]),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut c = connector(&sample_file(dir.path()), true);
        let mut t = loaded(&mut *c);
        t.rows[1][1] = CellValue::String("ink, blue".into());
        let plan = build_write_back_plan(&t, &identity).unwrap();
        apply_write_back(
            &mut *c,
            DbEngine::Sqlite,
            "main",
            "items",
            &t.columns,
            &identity,
            &plan,
        )
        .unwrap();
        let back = c.query("SELECT name, note FROM items ORDER BY id").unwrap();
        assert_eq!(
            back.rows[1][0],
            CellValue::String("ink, blue".into()),
            "{identity:?}"
        );
        // The untouched row keeps its bytes.
        assert_eq!(
            back.rows[0][1],
            CellValue::Binary(vec![0, 255]),
            "{identity:?}"
        );
    }
}
