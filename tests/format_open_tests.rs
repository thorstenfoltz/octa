mod common;

use common::{ensure_fixtures, fixture_path};
use octo::formats::FormatRegistry;

#[test]
fn test_open_csv() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.csv");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.col_count(), 3);
    assert_eq!(reader.name(), "CSV");
}

#[test]
fn test_open_tsv() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.tsv");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.col_count(), 3);
    assert_eq!(reader.name(), "TSV");
}

#[test]
fn test_open_json() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.json");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.col_count(), 3);
    assert_eq!(reader.name(), "JSON");
}

#[test]
fn test_open_jsonl() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.jsonl");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.col_count(), 3);
    assert_eq!(reader.name(), "JSON Lines");
}

#[test]
fn test_open_xml() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.xml");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.col_count(), 3);
    assert_eq!(reader.name(), "XML");
}

#[test]
fn test_open_toml() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.toml");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(reader.name(), "TOML");
}

#[test]
fn test_open_yaml() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.yaml");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.col_count(), 3);
    assert_eq!(reader.name(), "YAML");
}

#[test]
fn test_open_markdown() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.md");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert!(table.row_count() > 0);
    assert_eq!(table.col_count(), 1);
    assert_eq!(reader.name(), "Markdown");
}

#[test]
fn test_open_text() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.txt");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.col_count(), 1);
    assert_eq!(reader.name(), "Text");
}

#[test]
fn test_open_parquet() {
    ensure_fixtures();
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.parquet");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.col_count(), 3);
    assert_eq!(reader.name(), "Parquet");
}

#[test]
fn test_open_avro() {
    ensure_fixtures();
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.avro");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.col_count(), 3);
    assert_eq!(reader.name(), "Avro");
}

#[test]
fn test_open_arrow_ipc() {
    ensure_fixtures();
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.arrow");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.col_count(), 3);
    assert_eq!(reader.name(), "Arrow IPC");
}

#[test]
fn test_open_excel() {
    ensure_fixtures();
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.xlsx");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.col_count(), 3);
    assert_eq!(reader.name(), "Excel");
}

#[test]
fn test_open_pdf() {
    ensure_fixtures();
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.pdf");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert!(table.row_count() > 0);
    assert_eq!(reader.name(), "PDF");
}

// --- Round-trip tests: write to temp then read back ---

#[test]
fn test_roundtrip_csv() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.csv");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();

    let tmp = tempfile::NamedTempFile::with_suffix(".csv").unwrap();
    reader.write_file(tmp.path(), &table).unwrap();
    let table2 = reader.read_file(tmp.path()).unwrap();
    assert_eq!(table2.row_count(), table.row_count());
    assert_eq!(table2.col_count(), table.col_count());
}

#[test]
fn test_roundtrip_json() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.json");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();

    let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    reader.write_file(tmp.path(), &table).unwrap();
    let table2 = reader.read_file(tmp.path()).unwrap();
    assert_eq!(table2.row_count(), table.row_count());
    assert_eq!(table2.col_count(), table.col_count());
}

#[test]
fn test_roundtrip_parquet() {
    ensure_fixtures();
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.parquet");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();

    let tmp = tempfile::NamedTempFile::with_suffix(".parquet").unwrap();
    reader.write_file(tmp.path(), &table).unwrap();
    let table2 = reader.read_file(tmp.path()).unwrap();
    assert_eq!(table2.row_count(), table.row_count());
    assert_eq!(table2.col_count(), table.col_count());
}

#[test]
fn test_roundtrip_yaml() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.yaml");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();

    let tmp = tempfile::NamedTempFile::with_suffix(".yaml").unwrap();
    reader.write_file(tmp.path(), &table).unwrap();
    let table2 = reader.read_file(tmp.path()).unwrap();
    assert_eq!(table2.row_count(), table.row_count());
}

#[test]
fn test_roundtrip_xml() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.xml");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();

    let tmp = tempfile::NamedTempFile::with_suffix(".xml").unwrap();
    reader.write_file(tmp.path(), &table).unwrap();
    let table2 = reader.read_file(tmp.path()).unwrap();
    assert_eq!(table2.row_count(), table.row_count());
}

#[test]
fn test_roundtrip_avro() {
    ensure_fixtures();
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.avro");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();

    let tmp = tempfile::NamedTempFile::with_suffix(".avro").unwrap();
    reader.write_file(tmp.path(), &table).unwrap();
    let table2 = reader.read_file(tmp.path()).unwrap();
    assert_eq!(table2.row_count(), table.row_count());
    assert_eq!(table2.col_count(), table.col_count());
}

#[test]
fn test_roundtrip_arrow_ipc() {
    ensure_fixtures();
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.arrow");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();

    let tmp = tempfile::NamedTempFile::with_suffix(".arrow").unwrap();
    reader.write_file(tmp.path(), &table).unwrap();
    let table2 = reader.read_file(tmp.path()).unwrap();
    assert_eq!(table2.row_count(), table.row_count());
    assert_eq!(table2.col_count(), table.col_count());
}

#[test]
fn test_roundtrip_excel() {
    ensure_fixtures();
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.xlsx");

    let xlsx_reader = reg.reader_for_path(&path).unwrap();
    let table = xlsx_reader.read_file(&path).unwrap();

    let tmp = tempfile::NamedTempFile::with_suffix(".xlsx").unwrap();
    xlsx_reader.write_file(tmp.path(), &table).unwrap();
    let table2 = xlsx_reader.read_file(tmp.path()).unwrap();
    assert_eq!(table2.row_count(), table.row_count());
    assert_eq!(table2.col_count(), table.col_count());
}

#[test]
fn test_roundtrip_text() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.txt");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();

    let tmp = tempfile::NamedTempFile::with_suffix(".txt").unwrap();
    reader.write_file(tmp.path(), &table).unwrap();
    let table2 = reader.read_file(tmp.path()).unwrap();
    assert_eq!(table2.row_count(), table.row_count());
}

#[test]
fn test_roundtrip_markdown() {
    let reg = FormatRegistry::new();
    let path = fixture_path("sample.md");
    let reader = reg.reader_for_path(&path).unwrap();
    let table = reader.read_file(&path).unwrap();

    let tmp = tempfile::NamedTempFile::with_suffix(".md").unwrap();
    reader.write_file(tmp.path(), &table).unwrap();
    let table2 = reader.read_file(tmp.path()).unwrap();
    assert_eq!(table2.row_count(), table.row_count());
}
