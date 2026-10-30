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
fn test_timestamp_and_date_columns_are_formatted() {
    // Regression: the reader used to dump the raw epoch integer as the cell
    // text (e.g. "1769975775172766") instead of a readable datetime. A
    // TIMESTAMP must come back as a DateTime string and a DATE as a Date
    // string, not as raw numbers.
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "temporal.duckdb");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE t (ts TIMESTAMP, d DATE);
         INSERT INTO t VALUES (TIMESTAMP '2026-02-01 19:56:15.172766', DATE '2026-02-01');",
    )
    .unwrap();
    drop(conn);

    let table = DuckDbReader.read_table(&path, "t").unwrap();
    match table.get(0, 0) {
        Some(CellValue::DateTime(s)) => {
            assert!(
                s.starts_with("2026-02-01 19:56:15"),
                "timestamp should be a formatted datetime, got {s:?}"
            );
            assert!(
                !s.chars().all(|c| c.is_ascii_digit()),
                "timestamp must not be a raw integer string, got {s:?}"
            );
        }
        other => panic!("expected DateTime cell, got {other:?}"),
    }
    match table.get(0, 1) {
        Some(CellValue::Date(s)) => assert_eq!(s, "2026-02-01"),
        other => panic!("expected Date cell, got {other:?}"),
    }
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

/// The synthetic id column appears on SAVE only. Reading must never mutate
/// the file: users export a result to .duckdb, open it once in Octa, and
/// other tools must not suddenly see an `__octa_row_id` counter column.
#[test]
fn test_synthetic_row_id_added_on_save_not_on_read() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "users.duckdb");
    seed_users_db(&path);

    let row_id_count = |path: &std::path::Path| -> i64 {
        let conn = Connection::open(path).unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM information_schema.columns \
             WHERE table_name='users' AND column_name='__octa_row_id'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };

    let mut table = DuckDbReader.read_table(&path, "users").unwrap();
    assert_eq!(row_id_count(&path), 0, "reading must not mutate the file");

    // A save materialises the id column (diff-saves need row identity), and
    // the edit lands on the right row even though the tags were collected
    // before the column existed (backfill = rowid keeps them aligned).
    table.set(1, 1, CellValue::String("Edited".into()));
    table.apply_edits();
    DuckDbReader.write_file(&path, &table).unwrap();
    assert_eq!(row_id_count(&path), 1, "save materialises the id column");
    let reloaded = DuckDbReader.read_table(&path, "users").unwrap();
    assert_eq!(
        reloaded.get(1, 1),
        Some(&CellValue::String("Edited".into()))
    );
    assert_eq!(reloaded.get(0, 1), table.get(0, 1), "other rows untouched");
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

#[test]
fn times_and_intervals_round_trip_through_a_duckdb_file() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "times.duckdb");
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE t (a TIME, b INTERVAL);
             INSERT INTO t VALUES ('14:30:00.25', INTERVAL '30 hours 15 minutes');",
        )
        .unwrap();
    let mut table = DuckDbReader.read_table(&path, "t").unwrap();
    let ns = |h: i64, m: i64| (h * 3600 + m * 60) * 1_000_000_000;
    assert_eq!(table.rows[0][0], CellValue::Time(ns(14, 30) + 250_000_000));
    assert_eq!(table.rows[0][1], CellValue::Duration(ns(30, 15)));
    table.rows[0][0] = CellValue::Time(ns(8, 5));
    table.rows[0][1] = CellValue::Duration(-ns(1, 0));
    DuckDbReader.write_file(&path, &table).unwrap();
    let back = DuckDbReader.read_table(&path, "t").unwrap();
    assert_eq!(back.rows[0][0], CellValue::Time(ns(8, 5)));
    assert_eq!(back.rows[0][1], CellValue::Duration(-ns(1, 0)));
}

// ---- INTERVAL / TIME / TIME_NS survive a save --------------------------------

fn text_rows(path: &std::path::Path, sql: &str) -> Vec<Vec<String>> {
    let conn = Connection::open(path).unwrap();
    let mut stmt = conn.prepare(sql).unwrap();
    let mut rows = stmt.query([]).unwrap();
    let mut out = Vec::new();
    while let Some(r) = rows.next().unwrap() {
        let n = r.as_ref().column_count();
        out.push(
            (0..n)
                .map(|i| r.get::<_, Option<String>>(i).unwrap().unwrap_or_default())
                .collect(),
        );
    }
    out
}

fn seed_month_interval_db(path: &std::path::Path) {
    Connection::open(path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE t (id INTEGER, label VARCHAR, i INTERVAL, tm TIME);
             INSERT INTO t VALUES (1, 'a', INTERVAL '1 month', '14:30:00'),
               (2, 'b', INTERVAL '25 hours', '00:00:00.5'),
               (3, 'c', INTERVAL '1 day', NULL);",
        )
        .unwrap();
}

fn save_edit_label(path: &std::path::Path, allow: bool) {
    let mut table = DuckDbReader.read_table(path, "t").unwrap();
    table.set(1, 1, CellValue::String("B2".into()));
    table.apply_edits();
    DuckDbReader
        .write_file_schema_aware(path, &table, allow, &Default::default())
        .unwrap();
}

fn assert_month_interval_survives(allow: bool) {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "m.duckdb");
    seed_month_interval_db(&path);
    let q = "SELECT CAST(i AS VARCHAR), CAST(tm AS VARCHAR) FROM t ORDER BY id";
    let before = text_rows(&path, q);
    save_edit_label(&path, allow);
    assert_eq!(text_rows(&path, q), before);
    assert_eq!(
        text_rows(&path, "SELECT typeof(i), typeof(tm) FROM t LIMIT 1"),
        vec![vec!["INTERVAL".to_string(), "TIME".to_string()]]
    );
    assert_eq!(
        text_rows(&path, "SELECT label FROM t WHERE id = 2"),
        vec![vec!["B2".to_string()]]
    );
}

#[test]
fn a_month_interval_column_survives_an_unrelated_edit() {
    assert_month_interval_survives(true);
}

#[test]
fn a_month_interval_column_survives_an_unrelated_edit_with_write_protection() {
    assert_month_interval_survives(false);
}

#[test]
fn an_interval_without_months_keeps_its_day_part_after_an_edit() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "d.duckdb");
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE t (id INTEGER, label VARCHAR, i INTERVAL);
             INSERT INTO t VALUES (1, 'a', INTERVAL '1 day'), (2, 'b', INTERVAL '24 hours');",
        )
        .unwrap();
    let mut table = DuckDbReader.read_table(&path, "t").unwrap();
    assert!(table.columns[2].data_type.starts_with("Duration"));
    table.set(0, 1, CellValue::String("x".into()));
    table.set(1, 1, CellValue::String("y".into()));
    table.apply_edits();
    DuckDbReader
        .write_file_schema_aware(&path, &table, false, &Default::default())
        .unwrap();
    assert_eq!(
        text_rows(&path, "SELECT CAST(i AS VARCHAR) FROM t ORDER BY id"),
        vec![vec!["1 day".to_string()], vec!["24:00:00".to_string()]]
    );
}

#[test]
fn editing_a_month_interval_cell_writes_it_back() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "e.duckdb");
    seed_month_interval_db(&path);
    let mut table = DuckDbReader.read_table(&path, "t").unwrap();
    table.set(0, 2, CellValue::String("2 months 3 days".into()));
    table.apply_edits();
    DuckDbReader
        .write_file_schema_aware(&path, &table, false, &Default::default())
        .unwrap();
    assert_eq!(
        text_rows(&path, "SELECT CAST(i AS VARCHAR) FROM t WHERE id = 1"),
        vec![vec!["2 months 3 days".to_string()]]
    );
}

#[test]
fn month_interval_text_is_what_duckdb_prints() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "p.duckdb");
    let vals = [
        "INTERVAL '1 month'",
        "INTERVAL '14 months'",
        "INTERVAL '1 year 2 months'",
        "INTERVAL '2 months 3 days 04:00:00'",
        "INTERVAL '-1 month -2 days'",
        "INTERVAL '1 month 00:00:00.5'",
        "INTERVAL '12 months'",
        "INTERVAL '1 month -03:00:00'",
    ];
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE t (id INTEGER, i INTERVAL)")
        .unwrap();
    for (n, v) in vals.iter().enumerate() {
        conn.execute_batch(&format!("INSERT INTO t VALUES ({n}, {v})"))
            .unwrap();
    }
    drop(conn);
    let expected = text_rows(&path, "SELECT CAST(i AS VARCHAR) FROM t ORDER BY id");
    let table = DuckDbReader.read_table(&path, "t").unwrap();
    let got: Vec<String> = (0..vals.len())
        .map(|r| table.get(r, 1).unwrap().to_string())
        .collect();
    let want: Vec<String> = expected.into_iter().map(|r| r[0].clone()).collect();
    assert_eq!(got, want);
}

#[test]
fn a_time_ns_value_keeps_its_nanoseconds() {
    let dir = TempDir::new().unwrap();
    let path = fresh_db_path(&dir, "n.duckdb");
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE t (id INTEGER, label VARCHAR, tm TIME_NS);
             INSERT INTO t VALUES (1, 'a', '14:30:00.123456789'), (2, 'b', '01:00:00.000000001');",
        )
        .unwrap();
    let q = "SELECT CAST(tm AS VARCHAR) FROM t ORDER BY id";
    let before = text_rows(&path, q);
    let mut table = DuckDbReader.read_table(&path, "t").unwrap();
    table.set(0, 1, CellValue::String("x".into()));
    table.apply_edits();
    DuckDbReader
        .write_file_schema_aware(&path, &table, false, &Default::default())
        .unwrap();
    assert_eq!(text_rows(&path, q), before);
    // Editing the cell itself keeps nanoseconds too.
    let mut table = DuckDbReader.read_table(&path, "t").unwrap();
    table.set(1, 2, CellValue::String("01:00:00.000000007".into()));
    table.apply_edits();
    DuckDbReader
        .write_file_schema_aware(&path, &table, false, &Default::default())
        .unwrap();
    assert_eq!(
        text_rows(
            &path,
            "SELECT CAST(tm AS VARCHAR), typeof(tm) FROM t WHERE id = 2"
        ),
        vec![vec![
            "01:00:00.000000007".to_string(),
            "TIME_NS".to_string()
        ]]
    );
}
