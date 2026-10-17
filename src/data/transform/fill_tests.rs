use super::*;
use crate::data::{ColumnInfo, DataTable};

fn col(values: &[Option<&str>]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns.push(ColumnInfo {
        name: "c".to_string(),
        data_type: "Utf8".to_string(),
    });
    t.rows = values
        .iter()
        .map(|v| {
            vec![match v {
                Some(s) => CellValue::String(s.to_string()),
                None => CellValue::Null,
            }]
        })
        .collect();
    t
}

#[test]
fn down_propagates_and_keeps_leading_null() {
    let t = col(&[None, Some("A"), None, None, Some("B"), None]);
    let out = fill_down(&t, 0);
    assert_eq!(out[0], CellValue::Null);
    assert_eq!(out[2], CellValue::String("A".into()));
    assert_eq!(out[3], CellValue::String("A".into()));
    assert_eq!(out[5], CellValue::String("B".into()));
}

#[test]
fn up_propagates_and_keeps_trailing_null() {
    let t = col(&[None, Some("A"), None, Some("B"), None]);
    let out = fill_up(&t, 0);
    assert_eq!(out[0], CellValue::String("A".into()));
    assert_eq!(out[2], CellValue::String("B".into()));
    assert_eq!(out[4], CellValue::Null);
}
