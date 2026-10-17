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
fn delimiter_split_pads_short_rows() {
    let t = table(&["a,b,c", "x,y", "z"]);
    let out = split_column(&t, 0, &SplitSpec::Delimiter(",".into())).unwrap();
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].0, "src_1");
    assert_eq!(out[2].1[1], CellValue::Null); // "x,y" has no 3rd part
    assert_eq!(out[0].1[2], CellValue::String("z".into()));
}

#[test]
fn regex_split() {
    let t = table(&["a1b2c"]);
    let out = split_column(&t, 0, &SplitSpec::Regex(r"\d".into())).unwrap();
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].1[0], CellValue::String("a".into()));
    assert_eq!(out[2].1[0], CellValue::String("c".into()));
}

#[test]
fn fixed_width_split() {
    let t = table(&["abcdef"]);
    let out = split_column(&t, 0, &SplitSpec::FixedWidth(2)).unwrap();
    assert_eq!(out.len(), 3);
    assert_eq!(out[1].1[0], CellValue::String("cd".into()));
}

#[test]
fn invalid_regex_errors() {
    let t = table(&["x"]);
    assert!(split_column(&t, 0, &SplitSpec::Regex("(".into())).is_err());
}
