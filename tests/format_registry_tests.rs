use octo::formats::FormatRegistry;
use std::path::PathBuf;

#[test]
fn test_registry_has_readers() {
    let reg = FormatRegistry::new();
    let exts = reg.all_extensions();
    assert!(!exts.is_empty());
}

#[test]
fn test_reader_for_csv() {
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&PathBuf::from("data.csv"));
    assert!(reader.is_some());
    assert_eq!(reader.unwrap().name(), "CSV");
}

#[test]
fn test_reader_for_json() {
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&PathBuf::from("data.json"));
    assert!(reader.is_some());
    assert_eq!(reader.unwrap().name(), "JSON");
}

#[test]
fn test_reader_for_jsonl() {
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&PathBuf::from("data.jsonl"));
    assert!(reader.is_some());
    assert_eq!(reader.unwrap().name(), "JSON Lines");
}

#[test]
fn test_reader_for_parquet() {
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&PathBuf::from("data.parquet"));
    assert!(reader.is_some());
    assert_eq!(reader.unwrap().name(), "Parquet");
}

#[test]
fn test_reader_for_tsv() {
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&PathBuf::from("data.tsv"));
    assert!(reader.is_some());
    assert_eq!(reader.unwrap().name(), "TSV");
}

#[test]
fn test_reader_for_xlsx() {
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&PathBuf::from("data.xlsx"));
    assert!(reader.is_some());
    assert_eq!(reader.unwrap().name(), "Excel");
}

#[test]
fn test_reader_for_toml() {
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&PathBuf::from("config.toml"));
    assert!(reader.is_some());
    assert_eq!(reader.unwrap().name(), "TOML");
}

#[test]
fn test_reader_for_yaml() {
    let reg = FormatRegistry::new();
    for ext in &["data.yaml", "data.yml"] {
        let reader = reg.reader_for_path(&PathBuf::from(ext));
        assert!(reader.is_some(), "No reader for {}", ext);
        assert_eq!(reader.unwrap().name(), "YAML");
    }
}

#[test]
fn test_reader_for_xml() {
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&PathBuf::from("data.xml"));
    assert!(reader.is_some());
    assert_eq!(reader.unwrap().name(), "XML");
}

#[test]
fn test_reader_for_avro() {
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&PathBuf::from("data.avro"));
    assert!(reader.is_some());
    assert_eq!(reader.unwrap().name(), "Avro");
}

#[test]
fn test_reader_case_insensitive() {
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&PathBuf::from("DATA.CSV"));
    assert!(reader.is_some());
    assert_eq!(reader.unwrap().name(), "CSV");
}

#[test]
fn test_reader_unknown_extension_falls_back_to_text() {
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&PathBuf::from("data.xyz"));
    assert!(reader.is_some());
    assert_eq!(reader.unwrap().name(), "Text");
}

#[test]
fn test_reader_no_extension_falls_back_to_text() {
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&PathBuf::from("noext"));
    assert!(reader.is_some());
    assert_eq!(reader.unwrap().name(), "Text");
}

#[test]
fn test_all_extensions() {
    let reg = FormatRegistry::new();
    let exts = reg.all_extensions();
    assert!(exts.contains(&"csv".to_string()));
    assert!(exts.contains(&"json".to_string()));
    assert!(exts.contains(&"parquet".to_string()));
    assert!(exts.contains(&"xlsx".to_string()));
}

#[test]
fn test_format_descriptions() {
    let reg = FormatRegistry::new();
    let descs = reg.format_descriptions();
    assert!(!descs.is_empty());
    let labels: Vec<&str> = descs.iter().map(|(n, _)| n.as_str()).collect();
    assert!(labels.iter().any(|l| l.contains(".csv")));
    assert!(labels.iter().any(|l| l.contains(".parquet")));
}

#[test]
fn test_reader_for_markdown() {
    let reg = FormatRegistry::new();
    for ext in &["doc.md", "doc.markdown", "doc.mdown", "doc.mkd"] {
        let reader = reg.reader_for_path(&PathBuf::from(ext));
        assert!(reader.is_some(), "No reader for {}", ext);
        assert_eq!(reader.unwrap().name(), "Markdown");
    }
}

#[test]
fn test_reader_for_text() {
    let reg = FormatRegistry::new();
    for ext in &["file.txt", "file.log", "file.cfg", "file.ini", "file.conf"] {
        let reader = reg.reader_for_path(&PathBuf::from(ext));
        assert!(reader.is_some(), "No reader for {}", ext);
        assert_eq!(reader.unwrap().name(), "Text");
    }
}

#[test]
fn test_reader_for_arrow_ipc() {
    let reg = FormatRegistry::new();
    for ext in &["data.arrow", "data.ipc", "data.feather"] {
        let reader = reg.reader_for_path(&PathBuf::from(ext));
        assert!(reader.is_some(), "No reader for {}", ext);
        assert_eq!(reader.unwrap().name(), "Arrow IPC");
    }
}

#[test]
fn test_reader_for_pdf() {
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&PathBuf::from("report.pdf"));
    assert!(reader.is_some());
    assert_eq!(reader.unwrap().name(), "PDF");
}

#[test]
fn test_all_extensions_includes_new_formats() {
    let reg = FormatRegistry::new();
    let exts = reg.all_extensions();
    assert!(exts.contains(&"txt".to_string()));
    assert!(exts.contains(&"md".to_string()));
    assert!(exts.contains(&"log".to_string()));
}

#[test]
fn test_format_descriptions_includes_all() {
    let reg = FormatRegistry::new();
    let descs = reg.format_descriptions();
    let labels: Vec<&str> = descs.iter().map(|(n, _)| n.as_str()).collect();
    assert!(labels.iter().any(|l| l.contains(".txt")));
    assert!(labels.iter().any(|l| l.contains(".md")));
    assert!(labels.iter().any(|l| l.contains(".parquet")));
    assert!(labels.iter().any(|l| l.contains(".csv")));
    assert!(labels.iter().any(|l| l.contains(".json")));
    assert!(labels.iter().any(|l| l.contains(".xlsx")));
}

#[test]
fn test_fallback_various_unknown_extensions() {
    let reg = FormatRegistry::new();
    for ext in &["data.xyz", "file.zzz", "test.banana", "doc.rs"] {
        let reader = reg.reader_for_path(&PathBuf::from(ext));
        assert!(reader.is_some(), "No fallback reader for {}", ext);
        assert_eq!(reader.unwrap().name(), "Text", "Wrong fallback for {}", ext);
    }
}
