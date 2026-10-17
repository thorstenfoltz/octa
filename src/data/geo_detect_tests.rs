use super::*;
use crate::data::{ColumnInfo, DataTable};

fn table(cols: &[(&str, &str)], rows: &[Vec<CellValue>]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|(n, ty)| ColumnInfo {
            name: n.to_string(),
            data_type: ty.to_string(),
        })
        .collect();
    t.rows = rows.to_vec();
    t
}

#[test]
fn detects_named_numeric_pair() {
    let t = table(
        &[
            ("city", "Utf8"),
            ("Latitude", "Float64"),
            ("Longitude", "Float64"),
        ],
        &[
            vec![
                CellValue::String("A".into()),
                CellValue::Float(51.5),
                CellValue::Float(-0.12),
            ],
            vec![
                CellValue::String("B".into()),
                CellValue::Float(40.7),
                CellValue::Float(-74.0),
            ],
        ],
    );
    assert_eq!(detect_lat_lon(&t), Some((1, 2)));
}

#[test]
fn rejects_out_of_range() {
    let t = table(
        &[("lat", "Float64"), ("lon", "Float64")],
        &[vec![CellValue::Float(999.0), CellValue::Float(5.0)]],
    );
    assert_eq!(detect_lat_lon(&t), None);
}

#[test]
fn rejects_text_columns() {
    let t = table(
        &[("lat", "Utf8"), ("lon", "Utf8")],
        &[vec![
            CellValue::String("north".into()),
            CellValue::String("west".into()),
        ]],
    );
    assert_eq!(detect_lat_lon(&t), None);
}

#[test]
fn ignores_unrelated_columns() {
    let t = table(
        &[("platform", "Utf8"), ("longing", "Utf8")],
        &[vec![
            CellValue::String("x".into()),
            CellValue::String("y".into()),
        ]],
    );
    assert_eq!(detect_lat_lon(&t), None);
}

#[test]
fn parses_string_coords() {
    let t = table(
        &[("lat", "Utf8"), ("lng", "Utf8")],
        &[vec![
            CellValue::String("48.85".into()),
            CellValue::String("2.35".into()),
        ]],
    );
    assert_eq!(detect_lat_lon(&t), Some((0, 1)));
}
