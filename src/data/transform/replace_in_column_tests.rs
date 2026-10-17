use super::*;
use crate::data::{ColumnInfo, DataTable, SearchMode};

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
fn plain_replace_all_occurrences() {
    let t = table(&["a-a-a", "b-b"]);
    let m = RowMatcher::new("-", SearchMode::Plain);
    let out = replace_in_column(&t, 0, &m, "_");
    assert_eq!(out[0], CellValue::String("a_a_a".into()));
    assert_eq!(out[1], CellValue::String("b_b".into()));
}

#[test]
fn regex_replace() {
    let t = table(&["2024-01-02"]);
    let m = RowMatcher::new(r"-", SearchMode::Regex);
    let out = replace_in_column(&t, 0, &m, "/");
    assert_eq!(out[0], CellValue::String("2024/01/02".into()));
}

#[test]
fn null_untouched() {
    let mut t = table(&["x"]);
    t.rows[0][0] = CellValue::Null;
    let m = RowMatcher::new("x", SearchMode::Plain);
    let out = replace_in_column(&t, 0, &m, "y");
    assert_eq!(out[0], CellValue::Null);
}
