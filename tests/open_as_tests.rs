//! "Open as...": a file whose extension lies about its format can be re-read
//! through an explicitly named reader. These cover the registry primitive
//! (`reader_by_name` + `read_file`) that `OctaApp::reopen_active_as` is built
//! on, so the mapping from menu entry to reader is pinned by a test.
use std::io::Write;

#[test]
fn json_stored_as_log_reads_as_json_via_reader_by_name() {
    let mut f = tempfile::Builder::new().suffix(".log").tempfile().unwrap();
    write!(f, r#"{{"a": 1, "b": "x"}}"#).unwrap();
    f.flush().unwrap();

    let registry = octa::formats::FormatRegistry::new();
    let reader = registry
        .reader_by_name("JSON")
        .expect("JSON reader must exist by name");
    let table = reader
        .read_file(f.path())
        .expect("reads a JSON body out of a .log file");

    assert_eq!(table.row_count(), 1);
    assert!(table.columns.iter().any(|c| c.name == "a"));
    assert!(table.columns.iter().any(|c| c.name == "b"));
}

#[test]
fn jsonl_stored_as_log_reads_as_json_lines() {
    // Newline-delimited JSON in a .log is the classic mislabelled-file case.
    let mut f = tempfile::Builder::new().suffix(".log").tempfile().unwrap();
    writeln!(f, r#"{{"level": "info", "msg": "one"}}"#).unwrap();
    writeln!(f, r#"{{"level": "warn", "msg": "two"}}"#).unwrap();
    f.flush().unwrap();

    let registry = octa::formats::FormatRegistry::new();
    let reader = registry
        .reader_by_name("JSON Lines")
        .expect("JSON Lines reader must exist by name");
    let table = reader.read_file(f.path()).expect("reads JSONL from .log");

    assert_eq!(table.row_count(), 2);
    assert!(table.columns.iter().any(|c| c.name == "level"));
}

#[test]
fn every_open_as_reader_name_resolves() {
    // Pins the reader names the View -> Open as... menu passes to
    // `reopen_active_as`. A renamed reader breaks this test rather than
    // silently dead-ending a menu entry.
    let registry = octa::formats::FormatRegistry::new();
    for name in [
        "JSON",
        "JSON Lines",
        "CSV",
        "TSV",
        "YAML",
        "TOML",
        "XML",
        "Markdown",
        "Text",
    ] {
        assert!(
            registry.reader_by_name(name).is_some(),
            "Open as... menu references unknown reader {name:?}"
        );
    }
}

#[test]
fn unknown_reader_name_is_none() {
    let registry = octa::formats::FormatRegistry::new();
    assert!(registry.reader_by_name("Nonexistent Format").is_none());
}
