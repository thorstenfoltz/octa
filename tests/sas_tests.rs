//! Tests for the SAS reader. The `sas7bdat` crate is read-only, so we cannot
//! generate a fixture programmatically; these tests cover the registry
//! plumbing and error path only.
//!
//! Nothing here reads an actual .sas7bdat, so this file did NOT catch the
//! 0.2 -> 0.8 rewrite of `sas_reader.rs`. That was verified by hand against a
//! real file, diffing schema and rows before and after.

use octa::formats::FormatRegistry;

#[test]
fn sas_reader_resolves_via_extension() {
    let registry = FormatRegistry::new();
    let dummy = std::path::Path::new("data.sas7bdat");
    let reader = registry
        .reader_for_path(dummy)
        .expect("reader for sas7bdat");
    assert_eq!(reader.name(), "SAS");
    assert!(!reader.supports_write());
}

#[test]
fn sas_reader_reports_error_on_missing_file() {
    let registry = FormatRegistry::new();
    let path = std::path::Path::new("/nonexistent/missing.sas7bdat");
    let reader = registry.reader_for_path(path).unwrap();
    let err = reader.read_file(path).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("opening SAS file") || msg.contains("missing.sas7bdat"),
        "unexpected error message: {msg}"
    );
}

#[test]
fn sas_extensions_listed_under_registry() {
    let registry = FormatRegistry::new();
    let extensions = registry.all_extensions();
    assert!(extensions.iter().any(|e| e == "sas7bdat"));
}

#[test]
fn extension_resolver_includes_spss_stata_extensions() {
    let registry = FormatRegistry::new();
    let extensions = registry.all_extensions();
    assert!(extensions.iter().any(|e| e == "sav"));
    assert!(extensions.iter().any(|e| e == "zsav"));
    assert!(extensions.iter().any(|e| e == "dta"));
}
