//! Content-based format detection: a file with a missing or wrong extension
//! still resolves to the right reader via `FormatRegistry::reader_for_path`.

mod common;

use common::{ensure_fixtures, fixture_path};
use octa::formats::FormatRegistry;
use std::fs;

#[test]
fn sniffs_extensionless_parquet() {
    ensure_fixtures();
    let src = fixture_path("sample.parquet");
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("noext_data");
    fs::copy(&src, &dst).unwrap();

    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&dst).expect("a reader");
    assert_eq!(reader.name(), "Parquet");
}

#[test]
fn sniffs_csv_without_extension() {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("mystery");
    fs::write(&dst, "a,b,c\n1,2,3\n4,5,6\n").unwrap();

    let registry = FormatRegistry::new();
    assert_eq!(registry.reader_for_path(&dst).unwrap().name(), "CSV");
}

#[test]
fn sniffs_tsv_without_extension() {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("mystery_tsv");
    fs::write(&dst, "a\tb\tc\n1\t2\t3\n4\t5\t6\n").unwrap();

    let registry = FormatRegistry::new();
    assert_eq!(registry.reader_for_path(&dst).unwrap().name(), "TSV");
}

#[test]
fn sniffs_json_array_without_extension() {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("payload");
    fs::write(&dst, r#"[{"a":1},{"a":2}]"#).unwrap();

    let registry = FormatRegistry::new();
    assert_eq!(registry.reader_for_path(&dst).unwrap().name(), "JSON");
}

#[test]
fn sniffs_jsonl_without_extension() {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("events");
    fs::write(&dst, "{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n").unwrap();

    let registry = FormatRegistry::new();
    assert_eq!(registry.reader_for_path(&dst).unwrap().name(), "JSON Lines");
}

#[test]
fn unstructured_prose_falls_back_to_text() {
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("notes");
    fs::write(&dst, "just some prose without structure\nmore prose here\n").unwrap();

    let registry = FormatRegistry::new();
    assert_eq!(registry.reader_for_path(&dst).unwrap().name(), "Text");
}

#[test]
fn known_extension_is_not_overridden_by_content() {
    // A `.csv` extension wins even though the content is also valid JSON-ish;
    // sniffing only runs when the extension doesn't match a reader.
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("real.csv");
    fs::write(&dst, "a,b\n1,2\n").unwrap();

    let registry = FormatRegistry::new();
    assert_eq!(registry.reader_for_path(&dst).unwrap().name(), "CSV");
}
