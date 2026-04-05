use octo::data::CellValue;
use octo::formats::csv_reader::*;
use octo::formats::FormatReader;
use std::io::Write;
use tempfile::NamedTempFile;

// --- infer_cell_value ---

#[test]
fn test_infer_empty_is_null() {
    assert_eq!(infer_cell_value(""), CellValue::Null);
}

#[test]
fn test_infer_bool() {
    assert_eq!(infer_cell_value("true"), CellValue::Bool(true));
    assert_eq!(infer_cell_value("false"), CellValue::Bool(false));
    assert_eq!(infer_cell_value("TRUE"), CellValue::Bool(true));
    assert_eq!(infer_cell_value("False"), CellValue::Bool(false));
}

#[test]
fn test_infer_int() {
    assert_eq!(infer_cell_value("42"), CellValue::Int(42));
    assert_eq!(infer_cell_value("-7"), CellValue::Int(-7));
    assert_eq!(infer_cell_value("0"), CellValue::Int(0));
}

#[test]
fn test_infer_float() {
    assert_eq!(infer_cell_value("3.14"), CellValue::Float(3.14));
    assert_eq!(infer_cell_value("-0.5"), CellValue::Float(-0.5));
}

#[test]
fn test_infer_date() {
    assert_eq!(
        infer_cell_value("2024-01-15"),
        CellValue::Date("2024-01-15".into())
    );
}

#[test]
fn test_infer_datetime() {
    assert_eq!(
        infer_cell_value("2024-01-15 10:30:00"),
        CellValue::DateTime("2024-01-15 10:30:00".into())
    );
    assert_eq!(
        infer_cell_value("2024-01-15T10:30:00"),
        CellValue::DateTime("2024-01-15T10:30:00".into())
    );
}

#[test]
fn test_infer_datetime_with_timezone() {
    assert_eq!(
        infer_cell_value("2024-01-15T10:30:00Z"),
        CellValue::DateTime("2024-01-15T10:30:00Z".into())
    );
    assert_eq!(
        infer_cell_value("2024-01-15T10:30:00+01:00"),
        CellValue::DateTime("2024-01-15T10:30:00+01:00".into())
    );
    assert_eq!(
        infer_cell_value("2024-01-15T10:30:00.123Z"),
        CellValue::DateTime("2024-01-15T10:30:00.123Z".into())
    );
    assert_eq!(
        infer_cell_value("2024-01-15T10:30:00.123+05:30"),
        CellValue::DateTime("2024-01-15T10:30:00.123+05:30".into())
    );
}

#[test]
fn test_infer_datetime_with_fractional_seconds() {
    assert_eq!(
        infer_cell_value("2024-01-15 10:30:00.123"),
        CellValue::DateTime("2024-01-15 10:30:00.123".into())
    );
    assert_eq!(
        infer_cell_value("2024-01-15T10:30:00.456789"),
        CellValue::DateTime("2024-01-15T10:30:00.456789".into())
    );
}

#[test]
fn test_infer_string_fallback() {
    assert_eq!(
        infer_cell_value("hello world"),
        CellValue::String("hello world".into())
    );
}

// --- detect_delimiter ---

#[test]
fn test_detect_comma_delimiter() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "a,b,c").unwrap();
    writeln!(f, "1,2,3").unwrap();
    writeln!(f, "4,5,6").unwrap();
    assert_eq!(detect_delimiter(f.path()), Some(b','));
}

#[test]
fn test_detect_semicolon_delimiter() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "a;b;c").unwrap();
    writeln!(f, "1;2;3").unwrap();
    assert_eq!(detect_delimiter(f.path()), Some(b';'));
}

#[test]
fn test_detect_tab_delimiter() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "a\tb\tc").unwrap();
    writeln!(f, "1\t2\t3").unwrap();
    assert_eq!(detect_delimiter(f.path()), Some(b'\t'));
}

#[test]
fn test_detect_empty_file_returns_none() {
    let f = NamedTempFile::new().unwrap();
    assert_eq!(detect_delimiter(f.path()), None);
}

// --- read/write round-trip ---

#[test]
fn test_csv_round_trip() {
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(f, "name,age,active").unwrap();
    writeln!(f, "Alice,30,true").unwrap();
    writeln!(f, "Bob,25,false").unwrap();

    let table = CsvReader.read_file(f.path()).unwrap();
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.col_count(), 3);
    assert_eq!(table.columns[0].name, "name");
    assert_eq!(table.get(0, 0), Some(&CellValue::String("Alice".into())));
    assert_eq!(table.get(0, 1), Some(&CellValue::Int(30)));
    assert_eq!(table.get(1, 2), Some(&CellValue::Bool(false)));

    let out = NamedTempFile::with_suffix(".csv").unwrap();
    CsvReader.write_file(out.path(), &table).unwrap();
    let table2 = CsvReader.read_file(out.path()).unwrap();
    assert_eq!(table2.row_count(), 2);
    assert_eq!(table2.col_count(), 3);
    assert_eq!(table2.get(0, 0), Some(&CellValue::String("Alice".into())));
}

#[test]
fn test_tsv_round_trip() {
    let mut f = NamedTempFile::with_suffix(".tsv").unwrap();
    writeln!(f, "x\ty").unwrap();
    writeln!(f, "1\t2").unwrap();

    let table = TsvReader.read_file(f.path()).unwrap();
    assert_eq!(table.row_count(), 1);
    assert_eq!(table.get(0, 0), Some(&CellValue::Int(1)));

    let out = NamedTempFile::with_suffix(".tsv").unwrap();
    TsvReader.write_file(out.path(), &table).unwrap();
    let table2 = TsvReader.read_file(out.path()).unwrap();
    assert_eq!(table2.get(0, 0), Some(&CellValue::Int(1)));
}

// --- column type refinement ---

#[test]
fn test_column_type_refinement_int() {
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(f, "val").unwrap();
    writeln!(f, "1").unwrap();
    writeln!(f, "2").unwrap();
    writeln!(f, "3").unwrap();
    let table = CsvReader.read_file(f.path()).unwrap();
    assert_eq!(table.columns[0].data_type, "Int64");
}

#[test]
fn test_column_type_refinement_float() {
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(f, "val").unwrap();
    writeln!(f, "1.5").unwrap();
    writeln!(f, "2.5").unwrap();
    let table = CsvReader.read_file(f.path()).unwrap();
    assert_eq!(table.columns[0].data_type, "Float64");
}

#[test]
fn test_column_type_refinement_bool() {
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(f, "val").unwrap();
    writeln!(f, "true").unwrap();
    writeln!(f, "false").unwrap();
    let table = CsvReader.read_file(f.path()).unwrap();
    assert_eq!(table.columns[0].data_type, "Boolean");
}

#[test]
fn test_column_type_mixed_becomes_string() {
    let mut f = NamedTempFile::with_suffix(".csv").unwrap();
    writeln!(f, "val").unwrap();
    writeln!(f, "42").unwrap();
    writeln!(f, "hello").unwrap();
    let table = CsvReader.read_file(f.path()).unwrap();
    assert_eq!(table.columns[0].data_type, "Utf8");
}
