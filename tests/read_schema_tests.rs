//! `read_schema` must agree with `read_file().columns` for every format, or a
//! drift scan would report differences the reader does not see.

mod common;

use octa::formats::FormatRegistry;

fn assert_agrees(path: &std::path::Path) {
    let reg = FormatRegistry::new();
    let reader = reg
        .reader_for_path(path)
        .unwrap_or_else(|| panic!("no reader for {}", path.display()));
    let full = reader
        .read_file(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let quick = reader
        .read_schema(path)
        .unwrap_or_else(|e| panic!("read_schema {}: {e}", path.display()));
    assert_eq!(
        quick.len(),
        full.columns.len(),
        "{} column count disagrees",
        path.display()
    );
    for (a, b) in quick.iter().zip(full.columns.iter()) {
        assert_eq!(a.name, b.name, "{} column name", path.display());
        assert_eq!(
            a.data_type,
            b.data_type,
            "{} type of {}",
            path.display(),
            a.name
        );
    }
}

#[test]
fn parquet_schema_matches_a_full_read() {
    common::ensure_fixtures();
    assert_agrees(&common::fixture_path("sample.parquet"));
}

#[test]
fn arrow_schema_matches_a_full_read() {
    common::ensure_fixtures();
    assert_agrees(&common::fixture_path("sample.arrow"));
}

/// The defaulted body must work for a format that has no override.
#[test]
fn csv_falls_back_to_the_default_body() {
    assert_agrees(&common::fixture_path("sample.csv"));
}
