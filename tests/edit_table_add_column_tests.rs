//! Integration tests for the `add_column` op in the shared edit engine and
//! the schema-aware writer gate for DB files.

use octa::data::{CellValue, EditOp, apply_edit_ops};
use octa::formats::FormatReader;
use octa::formats::duckdb_reader::DuckDbReader;

fn duckdb_reader() -> impl FormatReader {
    DuckDbReader
}

// ---------------------------------------------------------------------------
// CSV round-trip
// ---------------------------------------------------------------------------

#[test]
fn add_column_round_trips_on_csv() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.csv");
    std::fs::write(&path, "id,v\n1,10\n2,20\n").unwrap();

    let registry = octa::formats::FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    let mut table = reader.read_file(&path).unwrap();

    let ops = vec![EditOp::AddColumn {
        name: "double".into(),
        expression: "v * 2".into(),
    }];
    let summary = apply_edit_ops(&mut table, &ops).unwrap();
    assert_eq!(summary.columns_added, 1);

    // Write back.
    reader.write_file(&path, &table).unwrap();

    // Re-read and check.
    let reread = reader.read_file(&path).unwrap();
    let double_col = reread
        .columns
        .iter()
        .position(|c| c.name == "double")
        .expect("double column must exist after round-trip");

    let values: Vec<i64> = (0..reread.row_count())
        .map(|r| match reread.get(r, double_col).unwrap() {
            CellValue::Int(i) => *i,
            CellValue::Float(f) => *f as i64,
            other => panic!("unexpected cell value: {other:?}"),
        })
        .collect();
    assert_eq!(values, vec![20, 40]);
}

// ---------------------------------------------------------------------------
// DuckDB schema-change gate + backup
// ---------------------------------------------------------------------------

#[test]
fn add_column_to_duckdb_is_schema_change_gated_with_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.duckdb");

    // Seed with two rows.
    {
        let conn = duckdb::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE t (id INTEGER, v INTEGER); \
             INSERT INTO t VALUES (1,10),(2,20);",
        )
        .unwrap();
    }

    let reader = duckdb_reader();
    let mut table = reader.read_file(&path).unwrap();

    let ops = vec![EditOp::AddColumn {
        name: "double".into(),
        expression: "v * 2".into(),
    }];
    apply_edit_ops(&mut table, &ops).unwrap();

    // Schema changes must be refused when allow=false.
    assert!(
        reader
            .write_file_schema_aware(&path, &table, false)
            .is_err(),
        "schema change must be refused when allow_schema_changes=false"
    );

    // backup_existing_file must produce a sidecar and return Some(_).
    let backup = octa::formats::backup_existing_file(&path)
        .unwrap()
        .expect("backup must be created for an existing file");
    assert!(backup.exists(), "backup file must exist on disk");

    // With allow=true the write succeeds.
    reader.write_file_schema_aware(&path, &table, true).unwrap();

    // Re-read and verify the new column.
    let reread = reader.read_file(&path).unwrap();
    let col_names: Vec<&str> = reread.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(
        col_names.contains(&"double"),
        "double column must persist: {col_names:?}"
    );

    let double_col = reread
        .columns
        .iter()
        .position(|c| c.name == "double")
        .unwrap();
    let v_col = reread.columns.iter().position(|c| c.name == "v").unwrap();

    for r in 0..reread.row_count() {
        let v = match reread.get(r, v_col).unwrap() {
            CellValue::Int(i) => *i,
            other => panic!("unexpected v: {other:?}"),
        };
        let d = match reread.get(r, double_col).unwrap() {
            CellValue::Int(i) => *i,
            CellValue::Float(f) => *f as i64,
            other => panic!("unexpected double: {other:?}"),
        };
        assert_eq!(d, v * 2, "row {r}: double should be v*2");
    }
}
