//! Unit tests for [`compare`](compare). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

fn table(headers: &[&str], rows: &[&[&str]]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = headers
        .iter()
        .map(|h| ColumnInfo {
            name: (*h).to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    t.rows = rows
        .iter()
        .map(|r| r.iter().map(|s| CellValue::String(s.to_string())).collect())
        .collect();
    t
}

#[test]
fn ordered_reports_changed_cells_and_trailing_rows() {
    let a = table(&["id", "v"], &[&["1", "x"], &["2", "y"], &["3", "z"]]);
    let b = table(&["id", "v"], &[&["1", "x"], &["2", "Y"]]);
    let r = compare_ordered(&a, &b);
    assert_eq!(r.unchanged, 1); // row 0 identical
    assert_eq!(r.changed.len(), 1);
    assert_eq!(r.changed[0].row_a, 1);
    assert_eq!(r.changed[0].changed_columns, vec!["v".to_string()]);
    assert_eq!(r.only_in_a, vec![2]); // trailing "3,z"
    assert!(r.only_in_b.is_empty());
}

#[test]
fn join_matches_added_removed_changed() {
    let a = table(&["id", "v"], &[&["1", "x"], &["2", "y"], &["3", "z"]]);
    let b = table(&["id", "v"], &[&["2", "Y"], &["3", "z"], &["4", "w"]]);
    let r = compare_join(&a, &b, &["id".to_string()]).unwrap();
    assert_eq!(r.only_in_a, vec![0]); // id 1
    assert_eq!(r.only_in_b, vec![2]); // id 4
    assert_eq!(r.changed.len(), 1); // id 2: v changed
    assert_eq!(r.changed[0].changed_columns, vec!["v".to_string()]);
    assert_eq!(r.unchanged, 1); // id 3 identical
}

#[test]
fn join_handles_columns_in_different_order() {
    let a = table(&["id", "v"], &[&["1", "x"]]);
    let b = table(&["v", "id"], &[&["x", "1"]]);
    let r = compare_join(&a, &b, &["id".to_string()]).unwrap();
    assert_eq!(r.unchanged, 1);
    assert!(r.changed.is_empty());
}

#[test]
fn join_duplicate_keys_pair_in_order() {
    let a = table(&["id", "v"], &[&["1", "a"], &["1", "b"]]);
    let b = table(&["id", "v"], &[&["1", "a"], &["1", "c"]]);
    let r = compare_join(&a, &b, &["id".to_string()]).unwrap();
    assert_eq!(r.unchanged, 1); // first 1,a pair
    assert_eq!(r.changed.len(), 1); // second 1,b vs 1,c
}

#[test]
fn join_missing_key_column_errors() {
    let a = table(&["id"], &[&["1"]]);
    let b = table(&["other"], &[&["1"]]);
    assert!(compare_join(&a, &b, &["id".to_string()]).is_err());
}

#[test]
fn build_table_emits_paired_changed_rows() {
    let a = table(&["id", "v"], &[&["1", "x"]]);
    let b = table(&["id", "v"], &[&["1", "X"]]);
    let r = compare_ordered(&a, &b);
    let out = build_compare_table(&a, &b, &r);
    assert_eq!(out.col_count(), 4); // status, changed_columns, id, v
    assert_eq!(out.row_count(), 2); // changed_a + changed_b
    assert_eq!(out.get(0, 0).unwrap().to_string(), "changed_a");
    assert_eq!(out.get(1, 0).unwrap().to_string(), "changed_b");
    assert_eq!(out.get(0, 1).unwrap().to_string(), "v");
}
