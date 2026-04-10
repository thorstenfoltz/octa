mod common;

use common::{ensure_fixtures, fixture_path, sample_table};
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
fn test_reader_supports_write() {
    assert!(PdfReader.supports_write());
}

#[test]
fn test_read_pdf() {
    ensure_fixtures();
    let path = fixture_path("sample.pdf");
    let table = PdfReader.read_file(&path).unwrap();
    assert!(table.row_count() > 0);
    assert_eq!(table.col_count(), 2);
    assert_eq!(table.columns[0].name, "line");
    assert_eq!(table.columns[1].name, "text");
}

#[test]
fn test_read_pdf_format_name() {
    ensure_fixtures();
    let path = fixture_path("sample.pdf");
    let table = PdfReader.read_file(&path).unwrap();
    assert_eq!(table.format_name, Some("PDF".to_string()));
}

#[test]
fn test_write_and_read_back() {
    let table = sample_table();
    let tmp = tempfile::NamedTempFile::with_suffix(".pdf").unwrap();
    PdfReader.write_file(tmp.path(), &table).unwrap();

    let table2 = PdfReader.read_file(tmp.path()).unwrap();
    assert!(table2.row_count() > 0);
    assert_eq!(table2.col_count(), 2);
}

#[test]
fn test_read_pdf_line_numbers() {
    ensure_fixtures();
    let path = fixture_path("sample.pdf");
    let table = PdfReader.read_file(&path).unwrap();
    // First line should have line number 1
    if let Some(val) = table.get(0, 0) {
        assert_eq!(val.to_string(), "1");
    }
}
