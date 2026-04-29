//! Tests for the DBF (dBase) reader/writer.

mod common;

use common::{ensure_fixtures, fixture_path};
use octa::data::CellValue;
use octa::formats::FormatRegistry;

#[test]
fn dbf_reader_resolves_via_extension() {
    let registry = FormatRegistry::new();
    let dummy = std::path::Path::new("data.dbf");
    let reader = registry.reader_for_path(dummy).expect("reader for dbf");
    assert_eq!(reader.name(), "DBF");
    assert!(reader.supports_write());
}

#[test]
fn dbf_extensions_listed_under_registry() {
    let registry = FormatRegistry::new();
    assert!(registry.all_extensions().iter().any(|e| e == "dbf"));
}

#[test]
fn dbf_round_trip_via_write_then_read() {
    ensure_fixtures();
    let registry = FormatRegistry::new();
    let path = fixture_path("sample.dbf");
    let reader = registry.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).expect("read sample.dbf");

    // Three columns from the sample fixture: id (Int64 -> Numeric), name
    // (Utf8 -> Character), active (Boolean -> Logical). Column count and
    // ordering must match what we wrote.
    assert_eq!(table.columns.len(), 3);
    assert_eq!(table.columns[0].name, "id");
    assert_eq!(table.columns[1].name, "name");
    assert_eq!(table.columns[2].name, "active");

    // After write/read round-trip, the names round-trip; numeric goes Float
    // because dBase Numeric is stored as ASCII.
    assert_eq!(table.rows.len(), 3);
    match &table.rows[0][1] {
        CellValue::String(s) => assert_eq!(s, "Alice"),
        other => panic!("expected name string, got {:?}", other),
    }
    match &table.rows[0][2] {
        CellValue::Bool(b) => assert!(*b),
        other => panic!("expected boolean true, got {:?}", other),
    }
}

#[test]
fn dbf_reads_real_world_stations_fixture() {
    // `stations.dbf` is the dBase 0.7 crate's own test fixture, downloaded
    // from upstream (`tmontaigu/dbase-rs`) and committed locally.
    let path = fixture_path("stations.dbf");
    if !path.exists() {
        return;
    }
    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).expect("read stations.dbf");
    assert_eq!(table.columns.len(), 4);
    assert!(table.rows.len() >= 80);
    let names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["name", "marker-col", "marker-sym", "line"]);
    match &table.rows[0][0] {
        CellValue::String(s) => assert_eq!(s, "Van Dorn Street"),
        other => panic!("expected first station name, got {:?}", other),
    }
}

#[test]
fn dbf_writer_rejects_binary_columns() {
    use octa::data::{ColumnInfo, DataTable};
    use std::collections::HashMap;

    let table = DataTable {
        columns: vec![ColumnInfo {
            name: "blob".into(),
            data_type: "Binary".into(),
        }],
        rows: vec![vec![CellValue::Binary(vec![1, 2, 3])]],
        edits: HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    };

    let registry = FormatRegistry::new();
    let dummy = std::path::Path::new("dummy.dbf");
    let reader = registry.reader_for_path(dummy).unwrap();
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let err = reader.write_file(tmp.path(), &table).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("Binary"),
        "expected Binary rejection error, got: {msg}"
    );
}
