mod common;

use common::{ensure_fixtures, fixture_path};
use octa::formats::FormatReader;
use octa::formats::pdf_reader::PdfReader;

#[test]
fn test_reader_name() {
    assert_eq!(PdfReader.name(), "PDF");
}

#[test]
fn test_reader_extensions() {
    assert_eq!(PdfReader.extensions(), &["pdf"]);
}

#[test]
fn test_reader_is_read_only() {
    assert!(!PdfReader.supports_write());
}

#[test]
fn test_read_pdf() {
    ensure_fixtures();
    let path = fixture_path("sample.pdf");
    let table = PdfReader.read_file(&path).unwrap();
    assert!(table.row_count() > 0);
    assert_eq!(table.col_count(), 3);
    assert_eq!(table.columns[0].name, "page");
    assert_eq!(table.columns[1].name, "line");
    assert_eq!(table.columns[2].name, "text");
}

#[test]
fn test_read_pdf_format_name() {
    ensure_fixtures();
    let path = fixture_path("sample.pdf");
    let table = PdfReader.read_file(&path).unwrap();
    assert_eq!(table.format_name, Some("PDF".to_string()));
}

#[test]
fn test_read_pdf_page_and_line_numbers() {
    ensure_fixtures();
    let path = fixture_path("sample.pdf");
    let table = PdfReader.read_file(&path).unwrap();
    // First row should be on page 1, line 1.
    assert_eq!(table.get(0, 0).map(|v| v.to_string()), Some("1".into()));
    assert_eq!(table.get(0, 1).map(|v| v.to_string()), Some("1".into()));
}

#[test]
fn test_page_texts_from_table_groups_by_page() {
    use octa::data::{CellValue, ColumnInfo, DataTable};
    use octa::formats::pdf_reader::page_texts_from_table;

    let table = DataTable {
        columns: vec![
            ColumnInfo {
                name: "page".to_string(),
                data_type: "Int64".to_string(),
            },
            ColumnInfo {
                name: "line".to_string(),
                data_type: "Int64".to_string(),
            },
            ColumnInfo {
                name: "text".to_string(),
                data_type: "Utf8".to_string(),
            },
        ],
        rows: vec![
            vec![
                CellValue::Int(1),
                CellValue::Int(1),
                CellValue::String("hello".into()),
            ],
            vec![
                CellValue::Int(1),
                CellValue::Int(2),
                CellValue::String("world".into()),
            ],
            vec![
                CellValue::Int(2),
                CellValue::Int(1),
                CellValue::String("page 2".into()),
            ],
        ],
        edits: std::collections::HashMap::new(),
        source_path: None,
        format_name: Some("PDF".into()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    };
    let pages = page_texts_from_table(&table);
    assert_eq!(
        pages,
        vec!["hello\nworld".to_string(), "page 2".to_string()]
    );
}
