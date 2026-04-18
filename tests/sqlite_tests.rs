use octa::data::{CellValue, DataTable};
use octa::formats::FormatReader;
use octa::formats::sqlite_reader::SqliteReader;
use rusqlite::Connection;
use tempfile::NamedTempFile;

fn seed_users_db(path: &std::path::Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL, active INTEGER);
         INSERT INTO users (name, score, active) VALUES ('Alice', 9.5, 1);
         INSERT INTO users (name, score, active) VALUES ('Bob', 7.0, 0);
         INSERT INTO users (name, score, active) VALUES ('Charlie', 8.25, 1);",
    )
    .unwrap();
}

fn seed_two_table_db(path: &std::path::Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE a (x INTEGER);
         INSERT INTO a VALUES (1), (2);
         CREATE TABLE b (y TEXT);
         INSERT INTO b VALUES ('hello');",
    )
    .unwrap();
}

#[test]
fn test_list_tables() {
    let f = NamedTempFile::with_suffix(".sqlite").unwrap();
    seed_two_table_db(f.path());
    let tables = SqliteReader.list_tables(f.path()).unwrap().unwrap();
    let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b"]);
}

#[test]
fn test_read_table_populates_db_meta() {
    let f = NamedTempFile::with_suffix(".sqlite").unwrap();
    seed_users_db(f.path());
    let table = SqliteReader.read_table(f.path(), "users").unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.col_count(), 4);
    let meta = table.db_meta.as_ref().expect("db_meta must be populated");
    assert_eq!(meta.table_name, "users");
    assert_eq!(meta.row_tags.len(), 3);
    assert!(meta.row_tags.iter().all(|t| t.is_some()));
    assert_eq!(meta.original.len(), 3);
    assert_eq!(meta.original_columns, vec!["id", "name", "score", "active"]);
}

#[test]
fn test_read_first_table_when_no_name_given() {
    let f = NamedTempFile::with_suffix(".sqlite").unwrap();
    seed_two_table_db(f.path());
    let table = SqliteReader.read_file(f.path()).unwrap();
    assert_eq!(table.db_meta.as_ref().unwrap().table_name, "a");
}

#[test]
fn test_diff_save_update_only_changed_row() {
    let f = NamedTempFile::with_suffix(".sqlite").unwrap();
    seed_users_db(f.path());
    let mut table = SqliteReader.read_table(f.path(), "users").unwrap();
    table.set(0, 1, CellValue::String("Alicia".into()));
    table.apply_edits();
    SqliteReader.write_file(f.path(), &table).unwrap();

    let reloaded = SqliteReader.read_table(f.path(), "users").unwrap();
    assert_eq!(reloaded.row_count(), 3);
    assert_eq!(
        reloaded.get(0, 1),
        Some(&CellValue::String("Alicia".into()))
    );
    assert_eq!(reloaded.get(1, 1), Some(&CellValue::String("Bob".into())));
}

#[test]
fn test_diff_save_insert_new_row() {
    let f = NamedTempFile::with_suffix(".sqlite").unwrap();
    seed_users_db(f.path());
    let mut table = SqliteReader.read_table(f.path(), "users").unwrap();
    table.insert_row(table.row_count());
    let new_idx = table.row_count() - 1;
    table.set(new_idx, 1, CellValue::String("Dave".into()));
    table.set(new_idx, 2, CellValue::Float(6.0));
    table.set(new_idx, 3, CellValue::Int(1));
    table.apply_edits();
    SqliteReader.write_file(f.path(), &table).unwrap();

    let reloaded = SqliteReader.read_table(f.path(), "users").unwrap();
    assert_eq!(reloaded.row_count(), 4);
    assert_eq!(reloaded.get(3, 1), Some(&CellValue::String("Dave".into())));
    assert_eq!(reloaded.get(3, 2), Some(&CellValue::Float(6.0)));
}

#[test]
fn test_diff_save_delete_row() {
    let f = NamedTempFile::with_suffix(".sqlite").unwrap();
    seed_users_db(f.path());
    let mut table = SqliteReader.read_table(f.path(), "users").unwrap();
    table.delete_row(1);
    SqliteReader.write_file(f.path(), &table).unwrap();

    let reloaded = SqliteReader.read_table(f.path(), "users").unwrap();
    assert_eq!(reloaded.row_count(), 2);
    let names: Vec<_> = (0..reloaded.row_count())
        .map(|r| reloaded.get(r, 1).cloned())
        .collect();
    assert_eq!(names[0], Some(CellValue::String("Alice".into())));
    assert_eq!(names[1], Some(CellValue::String("Charlie".into())));
}

#[test]
fn test_save_rejects_schema_change() {
    let f = NamedTempFile::with_suffix(".sqlite").unwrap();
    seed_users_db(f.path());
    let mut table = SqliteReader.read_table(f.path(), "users").unwrap();
    table.columns[1].name = "renamed".into();
    let err = SqliteReader.write_file(f.path(), &table).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("schema"));
}

#[test]
fn test_save_rejects_table_without_db_meta() {
    let f = NamedTempFile::with_suffix(".sqlite").unwrap();
    seed_users_db(f.path());
    let mut table = SqliteReader.read_table(f.path(), "users").unwrap();
    table.db_meta = None;
    let err = SqliteReader.write_file(f.path(), &table).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("database"));
}

#[test]
fn test_unchanged_save_is_noop() {
    let f = NamedTempFile::with_suffix(".sqlite").unwrap();
    seed_users_db(f.path());
    let table = SqliteReader.read_table(f.path(), "users").unwrap();
    SqliteReader.write_file(f.path(), &table).unwrap();
    let reloaded = SqliteReader.read_table(f.path(), "users").unwrap();
    assert_eq!(reloaded.row_count(), 3);
    for r in 0..3 {
        for c in 0..4 {
            assert_eq!(reloaded.get(r, c), table.get(r, c), "row {r} col {c}");
        }
    }
}

#[test]
fn test_combined_insert_update_delete() {
    let f = NamedTempFile::with_suffix(".sqlite").unwrap();
    seed_users_db(f.path());
    let mut table = SqliteReader.read_table(f.path(), "users").unwrap();
    // Update Alice
    table.set(0, 2, CellValue::Float(10.0));
    table.apply_edits();
    // Delete Bob
    table.delete_row(1);
    // Insert new row at the end
    table.insert_row(table.row_count());
    let new_idx = table.row_count() - 1;
    table.set(new_idx, 1, CellValue::String("Eve".into()));
    table.set(new_idx, 2, CellValue::Float(5.5));
    table.set(new_idx, 3, CellValue::Int(0));
    table.apply_edits();

    SqliteReader.write_file(f.path(), &table).unwrap();
    let reloaded = SqliteReader.read_table(f.path(), "users").unwrap();
    assert_eq!(reloaded.row_count(), 3);
    assert_eq!(reloaded.get(0, 1), Some(&CellValue::String("Alice".into())));
    assert_eq!(reloaded.get(0, 2), Some(&CellValue::Float(10.0)));
    assert_eq!(
        reloaded.get(1, 1),
        Some(&CellValue::String("Charlie".into()))
    );
    assert_eq!(reloaded.get(2, 1), Some(&CellValue::String("Eve".into())));
}

#[test]
fn test_extension_recognition() {
    assert_eq!(SqliteReader.extensions(), &["sqlite", "sqlite3", "db"]);
    assert!(SqliteReader.supports_write());
}

#[test]
fn test_null_values_round_trip() {
    let f = NamedTempFile::with_suffix(".sqlite").unwrap();
    let conn = Connection::open(f.path()).unwrap();
    conn.execute_batch(
        "CREATE TABLE t (a TEXT, b INTEGER);
         INSERT INTO t VALUES ('x', NULL);
         INSERT INTO t VALUES (NULL, 7);",
    )
    .unwrap();
    drop(conn);

    let table = SqliteReader.read_table(f.path(), "t").unwrap();
    assert_eq!(table.get(0, 1), Some(&CellValue::Null));
    assert_eq!(table.get(1, 0), Some(&CellValue::Null));

    SqliteReader.write_file(f.path(), &table).unwrap();
    let reloaded = SqliteReader.read_table(f.path(), "t").unwrap();
    assert_eq!(reloaded.get(0, 1), Some(&CellValue::Null));
    assert_eq!(reloaded.get(1, 0), Some(&CellValue::Null));
}

#[test]
fn test_empty_database_read_errors() {
    let f = NamedTempFile::with_suffix(".sqlite").unwrap();
    let conn = Connection::open(f.path()).unwrap();
    drop(conn);
    let result = SqliteReader.read_file(f.path());
    assert!(result.is_err());
}

// Sanity check that read_file works through DataTable's existing API.
fn _ensure_datatable_compiles(_t: &DataTable) {}
