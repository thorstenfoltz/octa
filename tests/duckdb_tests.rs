use duckdb::Connection;
use octa::data::CellValue;
use octa::formats::FormatReader;
use octa::formats::duckdb_reader::DuckDbReader;
use tempfile::TempDir;

fn fresh_db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let p = dir.path().join(name);
    if p.exists() {
        std::fs::remove_file(&p).unwrap();
    }
    p
}

fn seed_users_db(path: &std::path::Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE users (id INTEGER, name VARCHAR, score DOUBLE, active BOOLEAN);
         INSERT INTO users VALUES (1, 'Alice', 9.5, TRUE);
         INSERT INTO users VALUES (2, 'Bob', 7.0, FALSE);
         INSERT INTO users VALUES (3, 'Charlie', 8.25, TRUE);",
    )
    .unwrap();
}

fn seed_two_table_db(path: &std::path::Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE alpha (x INTEGER);
         INSERT INTO alpha VALUES (1), (2);
         CREATE TABLE beta (y VARCHAR);
         INSERT INTO beta VALUES ('hi');",
    )
    .unwrap();
}

#[test]
fn test_list_tables() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "two.duckdb");
    seed_two_table_db(&path);
    let tables = DuckDbReader.list_tables(&path).unwrap().unwrap();
    let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn test_read_table_populates_db_meta() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "users.duckdb");
    seed_users_db(&path);
    let table = DuckDbReader.read_table(&path, "users").unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.col_count(), 4);
    let meta = table.db_meta.as_ref().expect("db_meta must be populated");
    assert_eq!(meta.table_name, "users");
    assert_eq!(meta.row_tags.len(), 3);
    assert!(meta.row_tags.iter().all(|t| t.is_some()));
    // synthetic id column must be hidden from user-visible schema
    assert_eq!(meta.original_columns, vec!["id", "name", "score", "active"]);
    assert!(table.columns.iter().all(|c| c.name != "__octa_row_id"));
}

#[test]
fn test_read_first_table_when_no_name_given() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "two.duckdb");
    seed_two_table_db(&path);
    let table = DuckDbReader.read_file(&path).unwrap();
    assert_eq!(table.db_meta.as_ref().unwrap().table_name, "alpha");
}

#[test]
fn test_diff_save_update_only_changed_row() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "users.duckdb");
    seed_users_db(&path);
    let mut table = DuckDbReader.read_table(&path, "users").unwrap();
    table.set(0, 1, CellValue::String("Alicia".into()));
    table.apply_edits();
    DuckDbReader.write_file(&path, &table).unwrap();

    let reloaded = DuckDbReader.read_table(&path, "users").unwrap();
    assert_eq!(reloaded.row_count(), 3);
    assert_eq!(
        reloaded.get(0, 1),
        Some(&CellValue::String("Alicia".into()))
    );
    assert_eq!(reloaded.get(1, 1), Some(&CellValue::String("Bob".into())));
}

#[test]
fn test_diff_save_insert_new_row() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "users.duckdb");
    seed_users_db(&path);
    let mut table = DuckDbReader.read_table(&path, "users").unwrap();
    table.insert_row(table.row_count());
    let new_idx = table.row_count() - 1;
    table.set(new_idx, 0, CellValue::Int(4));
    table.set(new_idx, 1, CellValue::String("Dave".into()));
    table.set(new_idx, 2, CellValue::Float(6.0));
    table.set(new_idx, 3, CellValue::Bool(true));
    table.apply_edits();
    DuckDbReader.write_file(&path, &table).unwrap();

    let reloaded = DuckDbReader.read_table(&path, "users").unwrap();
    assert_eq!(reloaded.row_count(), 4);
    assert_eq!(reloaded.get(3, 1), Some(&CellValue::String("Dave".into())));
    assert_eq!(reloaded.get(3, 2), Some(&CellValue::Float(6.0)));
}

#[test]
fn test_diff_save_delete_row() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "users.duckdb");
    seed_users_db(&path);
    let mut table = DuckDbReader.read_table(&path, "users").unwrap();
    table.delete_row(1);
    DuckDbReader.write_file(&path, &table).unwrap();

    let reloaded = DuckDbReader.read_table(&path, "users").unwrap();
    assert_eq!(reloaded.row_count(), 2);
    let names: Vec<_> = (0..reloaded.row_count())
        .map(|r| reloaded.get(r, 1).cloned())
        .collect();
    assert_eq!(names[0], Some(CellValue::String("Alice".into())));
    assert_eq!(names[1], Some(CellValue::String("Charlie".into())));
}

#[test]
fn test_save_rejects_schema_change() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "users.duckdb");
    seed_users_db(&path);
    let mut table = DuckDbReader.read_table(&path, "users").unwrap();
    table.columns[1].name = "renamed".into();
    let err = DuckDbReader.write_file(&path, &table).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("schema"));
}

#[test]
fn test_save_rejects_table_without_db_meta() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "users.duckdb");
    seed_users_db(&path);
    let mut table = DuckDbReader.read_table(&path, "users").unwrap();
    table.db_meta = None;
    let err = DuckDbReader.write_file(&path, &table).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("database"));
}

#[test]
fn test_combined_insert_update_delete() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "users.duckdb");
    seed_users_db(&path);
    let mut table = DuckDbReader.read_table(&path, "users").unwrap();
    table.set(0, 2, CellValue::Float(10.0));
    table.apply_edits();
    table.delete_row(1);
    table.insert_row(table.row_count());
    let new_idx = table.row_count() - 1;
    table.set(new_idx, 0, CellValue::Int(5));
    table.set(new_idx, 1, CellValue::String("Eve".into()));
    table.set(new_idx, 2, CellValue::Float(5.5));
    table.set(new_idx, 3, CellValue::Bool(false));
    table.apply_edits();

    DuckDbReader.write_file(&path, &table).unwrap();
    let reloaded = DuckDbReader.read_table(&path, "users").unwrap();
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
    assert_eq!(DuckDbReader.extensions(), &["duckdb", "ddb"]);
    assert!(DuckDbReader.supports_write());
}

#[test]
fn test_synthetic_row_id_persisted_after_write() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "users.duckdb");
    seed_users_db(&path);
    DuckDbReader.read_table(&path, "users").unwrap();

    let conn = Connection::open(&path).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM information_schema.columns \
             WHERE table_name='users' AND column_name='__octa_row_id'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "synthetic id column should be added on read");
}

#[test]
fn test_unchanged_save_preserves_data() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "users.duckdb");
    seed_users_db(&path);
    let table = DuckDbReader.read_table(&path, "users").unwrap();
    DuckDbReader.write_file(&path, &table).unwrap();
    let reloaded = DuckDbReader.read_table(&path, "users").unwrap();
    assert_eq!(reloaded.row_count(), 3);
    for r in 0..3 {
        for c in 0..4 {
            assert_eq!(reloaded.get(r, c), table.get(r, c), "row {r} col {c}");
        }
    }
}
