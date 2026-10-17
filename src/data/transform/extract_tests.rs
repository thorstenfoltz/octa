use super::*;
use crate::data::{ColumnInfo, DataTable};

fn table(values: &[&str]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns.push(ColumnInfo {
        name: "src".to_string(),
        data_type: "Utf8".to_string(),
    });
    t.rows = values
        .iter()
        .map(|v| vec![CellValue::String(v.to_string())])
        .collect();
    t
}

#[test]
fn extracts_capture_group() {
    let t = table(&["order #1234 ok", "no number here"]);
    let re = Regex::new(r"#(\d+)").unwrap();
    let out = extract_pattern(&t, 0, &re);
    assert_eq!(out[0], CellValue::String("1234".into()));
    assert_eq!(out[1], CellValue::Null);
}

#[test]
fn whole_match_when_no_group() {
    let t = table(&["abc123def"]);
    let re = Regex::new(r"\d+").unwrap();
    let out = extract_pattern(&t, 0, &re);
    assert_eq!(out[0], CellValue::String("123".into()));
}
