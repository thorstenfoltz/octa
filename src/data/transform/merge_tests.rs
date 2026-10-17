use super::*;
use crate::data::{ColumnInfo, DataTable};

fn two_col(rows: &[(&str, &str)]) -> DataTable {
    let mut t = DataTable::empty();
    for name in ["a", "b"] {
        t.columns.push(ColumnInfo {
            name: name.to_string(),
            data_type: "Utf8".to_string(),
        });
    }
    t.rows = rows
        .iter()
        .map(|(a, b)| {
            vec![
                CellValue::String(a.to_string()),
                CellValue::String(b.to_string()),
            ]
        })
        .collect();
    t
}

#[test]
fn merge_with_separator() {
    let t = two_col(&[("John", "Doe"), ("Jane", "Roe")]);
    let out = merge_columns(&t, &[0, 1], " ");
    assert_eq!(out[0], CellValue::String("John Doe".into()));
    assert_eq!(out[1], CellValue::String("Jane Roe".into()));
}

#[test]
fn null_becomes_empty() {
    let mut t = two_col(&[("x", "y")]);
    t.rows[0][1] = CellValue::Null;
    let out = merge_columns(&t, &[0, 1], "-");
    assert_eq!(out[0], CellValue::String("x-".into()));
}
