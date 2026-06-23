//! Schema-changing saves on DuckDB / SQLite via `write_file_schema_aware`.

use octa::data::CellValue;
use octa::formats::FormatReader;
use octa::formats::duckdb_reader::DuckDbReader;

fn duckdb_reader() -> impl FormatReader {
    DuckDbReader
}

#[test]
fn duckdb_add_column_persists_with_schema_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.duckdb");
    {
        let conn = duckdb::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE t (id INTEGER, name VARCHAR); \
             INSERT INTO t VALUES (1,'a'),(2,'b');",
        )
        .unwrap();
    }
    let reader = duckdb_reader();
    let mut table = reader.read_file(&path).unwrap();

    let idx = table.col_count();
    table.insert_column(idx, "score".to_string(), "Int64".to_string());
    table.set(0, idx, CellValue::Int(10));
    table.set(1, idx, CellValue::Int(20));
    table.apply_edits();

    assert!(
        reader
            .write_file_schema_aware(&path, &table, false)
            .is_err()
    );
    reader.write_file_schema_aware(&path, &table, true).unwrap();

    let reread = reader.read_file(&path).unwrap();
    let names: Vec<&str> = reread.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(
        names.contains(&"score"),
        "score column persisted: {names:?}"
    );
    let score_col = reread
        .columns
        .iter()
        .position(|c| c.name == "score")
        .unwrap();
    let id_col = reread.columns.iter().position(|c| c.name == "id").unwrap();
    for r in 0..reread.row_count() {
        let id = match reread.get(r, id_col).unwrap() {
            CellValue::Int(i) => *i,
            _ => panic!(),
        };
        let score = match reread.get(r, score_col).unwrap() {
            CellValue::Int(i) => *i,
            _ => panic!(),
        };
        assert_eq!(score, id * 10);
    }
}

#[test]
fn duckdb_drop_column_persists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.duckdb");
    {
        let conn = duckdb::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE t (id INTEGER, drop_me VARCHAR, keep VARCHAR); \
             INSERT INTO t VALUES (1,'x','k1'),(2,'y','k2');",
        )
        .unwrap();
    }
    let reader = duckdb_reader();
    let mut table = reader.read_file(&path).unwrap();
    let drop_idx = table
        .columns
        .iter()
        .position(|c| c.name == "drop_me")
        .unwrap();
    table.delete_column(drop_idx);
    table.apply_edits();

    reader.write_file_schema_aware(&path, &table, true).unwrap();
    let reread = reader.read_file(&path).unwrap();
    let names: Vec<&str> = reread.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(!names.contains(&"drop_me"), "dropped: {names:?}");
    assert!(names.contains(&"keep") && names.contains(&"id"));
}

fn sqlite_reader() -> impl FormatReader {
    octa::formats::sqlite_reader::SqliteReader
}

#[test]
fn sqlite_add_and_retype_column_persists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.sqlite");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE t (id INTEGER, qty INTEGER); \
             INSERT INTO t VALUES (1,5),(2,7);",
        )
        .unwrap();
    }
    let reader = sqlite_reader();
    let mut table = reader.read_file(&path).unwrap();

    // Retype qty (Int64 -> Utf8) and add a note column.
    let qty = table.columns.iter().position(|c| c.name == "qty").unwrap();
    table.columns[qty].data_type = "Utf8".to_string();
    table.set(0, qty, CellValue::String("five".to_string()));
    table.set(1, qty, CellValue::String("seven".to_string()));
    let n = table.col_count();
    table.insert_column(n, "note".to_string(), "Utf8".to_string());
    table.set(0, n, CellValue::String("hi".to_string()));
    table.apply_edits();

    assert!(
        reader
            .write_file_schema_aware(&path, &table, false)
            .is_err()
    );
    reader.write_file_schema_aware(&path, &table, true).unwrap();

    let reread = reader.read_file(&path).unwrap();
    let names: Vec<&str> = reread.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"note"));
    let id_col = reread.columns.iter().position(|c| c.name == "id").unwrap();
    let qty_col = reread.columns.iter().position(|c| c.name == "qty").unwrap();
    for r in 0..reread.row_count() {
        let id = match reread.get(r, id_col).unwrap() {
            CellValue::Int(i) => *i,
            _ => panic!(),
        };
        let q = reread.get(r, qty_col).unwrap().to_string();
        assert_eq!(q, if id == 1 { "five" } else { "seven" });
    }
}

#[test]
fn plain_write_file_still_rejects_schema_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.duckdb");
    {
        let conn = duckdb::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE t (id INTEGER); INSERT INTO t VALUES (1);")
            .unwrap();
    }
    let reader = duckdb_reader();
    let mut table = reader.read_file(&path).unwrap();
    let n = table.col_count();
    table.insert_column(n, "extra".to_string(), "Int64".to_string());
    table.apply_edits();
    // The legacy entry point used by convert / partition must keep refusing.
    assert!(reader.write_file(&path, &table).is_err());
}
