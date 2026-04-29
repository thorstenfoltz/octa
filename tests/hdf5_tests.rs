//! Tests for the HDF5 reader. The compound-dataset path was previously
//! stubbed and returned the literal `(compound)` placeholder for every
//! cell — these tests are the regression for that fix.

mod common;

use common::fixture_path;
use octa::data::CellValue;
use octa::formats::FormatRegistry;

#[test]
fn hdf5_reader_resolves_via_extension() {
    let registry = FormatRegistry::new();
    let dummy = std::path::Path::new("data.h5");
    let reader = registry.reader_for_path(dummy).expect("reader for h5");
    assert_eq!(reader.name(), "HDF5");
    assert!(!reader.supports_write());
}

#[test]
fn hdf5_compound_dataset_decodes_real_values() {
    let path = fixture_path("compound.h5");
    if !path.exists() {
        // Fixture is checked in; skip silently if missing in unusual sandbox.
        return;
    }
    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).expect("read compound.h5");

    // Schema: id (Int64), score (Float32), name (Utf8) — see the h5py
    // fixture generator in the test setup.
    assert_eq!(table.columns.len(), 3);
    assert_eq!(table.columns[0].name, "id");
    assert_eq!(table.columns[0].data_type, "Int64");
    assert_eq!(table.columns[1].name, "score");
    assert_eq!(table.columns[1].data_type, "Float32");
    assert_eq!(table.columns[2].name, "name");
    assert_eq!(table.columns[2].data_type, "Utf8");

    assert_eq!(table.rows.len(), 3);

    // Regression: every cell used to be `CellValue::String("(compound)")`.
    // The values below are what was actually written to the fixture.
    match &table.rows[0][0] {
        CellValue::Int(v) => assert_eq!(*v, 1),
        other => panic!("row 0 col 0 — expected Int(1), got {:?}", other),
    }
    match &table.rows[0][1] {
        CellValue::Float(v) => assert!((*v - 1.5_f64).abs() < 1e-6, "got {v}"),
        other => panic!("row 0 col 1 — expected Float(1.5), got {:?}", other),
    }
    match &table.rows[0][2] {
        CellValue::String(s) => assert_eq!(s, "alice"),
        other => panic!("row 0 col 2 — expected String(alice), got {:?}", other),
    }

    match &table.rows[2][0] {
        CellValue::Int(v) => assert_eq!(*v, 3),
        other => panic!("row 2 col 0 — expected Int(3), got {:?}", other),
    }
    match &table.rows[2][2] {
        CellValue::String(s) => assert_eq!(s, "charlie"),
        other => panic!("row 2 col 2 — expected String(charlie), got {:?}", other),
    }

    // No cell should still be the placeholder.
    for (r, row) in table.rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            if let CellValue::String(s) = cell {
                assert!(
                    s != "(compound)",
                    "row {r} col {c} still shows the placeholder"
                );
            }
        }
    }
}
