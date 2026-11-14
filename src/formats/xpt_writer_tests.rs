use super::*;
use crate::data::ColumnInfo;
use crate::formats::FormatReader;
use crate::formats::xpt_reader::XptReader;

fn table(columns: &[(&str, &str)], rows: Vec<Vec<CellValue>>) -> DataTable {
    DataTable {
        columns: columns
            .iter()
            .map(|(name, data_type)| ColumnInfo {
                name: name.to_string(),
                data_type: data_type.to_string(),
            })
            .collect(),
        rows,
        ..DataTable::empty()
    }
}

fn round_trip(source: &DataTable) -> DataTable {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("my-data 1.xpt");
    XptReader.write_file(&path, source).unwrap();
    XptReader.read_file(&path).unwrap()
}

#[test]
fn every_kind_of_cell_comes_back() {
    let s = |v: &str| CellValue::String(v.to_string());
    let source = table(
        &[
            ("n", "Int64"),
            ("x", "Float64"),
            ("ok", "Boolean"),
            ("d", "Date"),
            ("at", "DateTime"),
            ("t", "Time"),
            ("name", "Utf8"),
            ("amount", "Decimal128(10, 2)"),
            ("empty", "Utf8"),
        ],
        vec![
            vec![
                CellValue::Int(-7),
                CellValue::Float(0.1),
                CellValue::Bool(true),
                CellValue::Date("1959-12-31".into()),
                CellValue::DateTime("2024-02-29 23:59:58".into()),
                CellValue::Time(3_723 * NANOS_PER_SEC),
                s("Käse"),
                s("12.50"),
                CellValue::Null,
            ],
            vec![CellValue::Null; 9],
            vec![
                CellValue::Int(1 << 40),
                CellValue::Float(-1.5e-30),
                CellValue::Bool(false),
                CellValue::Date("2024-03-01".into()),
                CellValue::DateTime("1960-01-01 00:00:00".into()),
                CellValue::Time(0),
                s("a longer text"),
                CellValue::Float(3.25),
                CellValue::Null,
            ],
        ],
    );
    let back = round_trip(&source);
    let cells = |r: usize| -> Vec<String> { back.rows[r].iter().map(|c| c.to_string()).collect() };
    assert_eq!(
        cells(0),
        [
            "-7",
            "0.1",
            "1",
            "1959-12-31",
            "2024-02-29 23:59:58",
            "01:02:03",
            "Käse",
            "12.5",
            ""
        ]
    );
    assert!(back.rows[1].iter().all(|c| *c == CellValue::Null));
    assert_eq!(back.rows[2][0], CellValue::Int(1 << 40));
    assert_eq!(back.rows[2][1], CellValue::Float(-1.5e-30));
    assert_eq!(back.rows[2][6], s("a longer text"));
    assert_eq!(back.rows[2][7], CellValue::Float(3.25));
    let types: Vec<&str> = back.columns.iter().map(|c| c.data_type.as_str()).collect();
    assert_eq!(
        &types[..5],
        ["Int64", "Float64", "Int64", "Date", "DateTime"]
    );
    assert_eq!(types[6], "Utf8");
}

#[test]
fn a_column_of_mixed_cells_is_written_as_text() {
    let source = table(
        &[("v", "Utf8")],
        vec![
            vec![CellValue::Int(1)],
            vec![CellValue::String("two".into())],
        ],
    );
    let back = round_trip(&source);
    assert_eq!(back.columns[0].data_type, "Utf8");
    assert_eq!(back.rows[1][0].to_string(), "two");
}

#[test]
fn the_dataset_is_named_after_the_file() {
    assert_eq!(dataset_name(Path::new("/tmp/my-data 1.xpt")), "MYDATA1");
    assert_eq!(dataset_name(Path::new("2024.xpt")), "DATA");
    assert_eq!(dataset_name(Path::new("a_very_long_name.xpt")), "A_VERY_L");
}

#[test]
fn a_name_sas_cannot_hold_is_refused_by_name() {
    for name in ["unit price", "1st", "größe", &"x".repeat(33)] {
        let source = table(&[(name, "Int64")], vec![vec![CellValue::Int(1)]]);
        let dir = tempfile::tempdir().unwrap();
        let err = XptReader
            .write_file(&dir.path().join("a.xpt"), &source)
            .unwrap_err();
        assert!(format!("{err:#}").contains(name), "{name}: {err:#}");
    }
    let twice = table(&[("a", "Int64"), ("A", "Int64")], vec![]);
    let now = chrono::NaiveDateTime::default();
    assert!(encode(&twice, "X", now).is_err());
}

#[test]
fn long_text_or_a_long_name_needs_version_8() {
    let now = chrono::NaiveDateTime::default();
    let short = table(
        &[("a", "Utf8")],
        vec![vec![CellValue::String("x".repeat(200))]],
    );
    assert_eq!(&encode(&short, "X", now).unwrap()[20..27], b"LIBRARY");
    let long = table(
        &[("a", "Utf8")],
        vec![vec![CellValue::String("x".repeat(201))]],
    );
    assert_eq!(&encode(&long, "X", now).unwrap()[20..25], b"LIBV8");
    assert_eq!(round_trip(&long).rows[0][0].to_string().len(), 201);
    let named = table(&[("nine_long", "Int64")], vec![vec![CellValue::Int(1)]]);
    assert_eq!(round_trip(&named).columns[0].name, "nine_long");
}

#[test]
fn a_number_too_large_is_refused_and_one_too_small_is_zero() {
    assert!(ibm_float(1e80).is_none());
    assert!(ibm_float(f64::INFINITY).is_none());
    assert_eq!(ibm_float(1e-200), Some([0; 8]));
    assert_eq!(ibm_float(1.0), Some([0x41, 0x10, 0, 0, 0, 0, 0, 0]));
    assert_eq!(ibm_float(-8.0), Some([0xC1, 0x80, 0, 0, 0, 0, 0, 0]));
}

#[test]
fn a_file_of_several_datasets_is_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("two.xpt");
    let one = table(&[("a", "Int64")], vec![vec![CellValue::Int(1)]]);
    let now = chrono::NaiveDateTime::default();
    let mut bytes = encode(&one, "ONE", now).unwrap();
    // A second member: everything after the three library records, again.
    let member = bytes[3 * RECORD..].to_vec();
    bytes.extend(member);
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(XptReader.list_tables(&path).unwrap().unwrap().len(), 2);
    let err = XptReader.write_file(&path, &one).unwrap_err();
    assert!(format!("{err:#}").contains("several datasets"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}
