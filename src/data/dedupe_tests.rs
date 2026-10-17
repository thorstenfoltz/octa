use super::*;
use crate::data::ColumnInfo;

fn tbl() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "k".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "n".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::String("a".into()), CellValue::Int(1)],
        vec![CellValue::String("a".into()), CellValue::Int(2)],
        vec![CellValue::String("b".into()), CellValue::Int(3)],
    ];
    t
}

#[test]
fn keep_first_drops_later_dupe() {
    let out = dedupe_rows(&tbl(), &[0], KeepWhich::First);
    assert_eq!(out.row_count(), 2);
    assert_eq!(out.get(0, 1), Some(&CellValue::Int(1)));
}

#[test]
fn keep_last_keeps_later_dupe() {
    let out = dedupe_rows(&tbl(), &[0], KeepWhich::Last);
    assert_eq!(out.row_count(), 2);
    assert_eq!(out.get(0, 1), Some(&CellValue::Int(2)));
}

#[test]
fn whole_row_key_when_no_key_cols() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "a".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "b".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::String("x".into()), CellValue::Int(1)],
        vec![CellValue::String("x".into()), CellValue::Int(1)],
        vec![CellValue::String("x".into()), CellValue::Int(2)],
    ];
    let out = dedupe_rows(&t, &[], KeepWhich::First);
    assert_eq!(out.row_count(), 2);
}

#[test]
fn no_dupes_returns_all_rows() {
    let t = tbl();
    let out = dedupe_rows(&t, &[0], KeepWhich::First);
    // "a" appears twice, "b" once -> 2 unique keys
    assert_eq!(out.row_count(), 2);
}

#[test]
fn original_order_preserved() {
    let out = dedupe_rows(&tbl(), &[0], KeepWhich::First);
    // "a" (first) comes before "b"
    assert_eq!(out.get(0, 0), Some(&CellValue::String("a".into())));
    assert_eq!(out.get(1, 0), Some(&CellValue::String("b".into())));
}
