//! Tests for the R Dataset reader. The fixture is `cran/readr`'s
//! `tests/testthat/fixtures/test-non-ascii-1152.rds`, downloaded once
//! during development and committed under `tests/fixtures/sample.rds`.
//! It is a real `data.frame` saved by `saveRDS()` with non-ASCII column
//! names, so it exercises both the reader and UTF-8 handling.

mod common;

use common::fixture_path;
use octa::data::CellValue;
use octa::formats::FormatRegistry;

#[test]
fn rds_reader_resolves_via_extension() {
    let registry = FormatRegistry::new();
    for ext in &["data.rds", "data.rdata", "data.rda"] {
        let p = std::path::Path::new(ext);
        let reader = registry.reader_for_path(p).expect("reader for r dataset");
        assert_eq!(reader.name(), "R Dataset");
        assert!(!reader.supports_write());
    }
}

#[test]
fn rds_reads_real_dataframe_fixture() {
    let path = fixture_path("sample.rds");
    if !path.exists() {
        return; // checked-in fixture; skip if unavailable
    }
    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).expect("read sample.rds");

    // The readr fixture has 14 columns and 10 rows; column 0 is a Japanese
    // name. We don't pin every column but we do assert the shape and a few
    // anchor values to catch regressions in the reader.
    assert_eq!(table.columns.len(), 14);
    assert_eq!(table.rows.len(), 10);

    // Column 0 is a non-ASCII Utf8 column with the value "東京都" everywhere.
    assert_eq!(table.columns[0].data_type, "Utf8");
    match &table.rows[0][0] {
        CellValue::String(s) => assert_eq!(s, "東京都"),
        other => panic!("expected non-ASCII string, got {:?}", other),
    }

    // Column 3 is a logical column whose values were all NA in the fixture
    // — they should round-trip as Null, not as the literal string "NA".
    for row in &table.rows {
        assert!(matches!(row[3], CellValue::Null));
    }

    // Column 4 is numeric.
    assert_eq!(table.columns[4].data_type, "Float64");
    assert!(matches!(table.rows[0][4], CellValue::Float(_)));
}

#[test]
fn rds_rejects_rdata_workspace_with_clear_error() {
    let registry = FormatRegistry::new();
    let path = std::path::Path::new("/tmp/does-not-exist.rdata");
    let reader = registry.reader_for_path(path).unwrap();
    let err = reader.read_file(path).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("RData workspace") || msg.contains("saveRDS"),
        "expected workspace-not-supported error, got: {msg}"
    );
}
