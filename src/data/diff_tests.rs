//! Unit tests for [`diff`](diff). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::data::{CellValue, ColumnInfo};

fn table(rows: &[&[&str]]) -> DataTable {
    let mut t = DataTable::empty();
    let ncols = rows.first().map(|r| r.len()).unwrap_or(0);
    t.columns = (0..ncols)
        .map(|i| ColumnInfo {
            name: format!("c{i}"),
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
fn detects_added_removed_shared() {
    let a = table(&[&["1", "x"], &["2", "y"], &["3", "z"]]);
    let b = table(&[&["2", "y"], &["3", "z"], &["4", "w"]]);
    let d = diff_rows(&a, &b);
    assert_eq!(d.only_in_a, vec![0]); // row "1,x"
    assert_eq!(d.only_in_b, vec![2]); // row "4,w"
    assert_eq!(d.shared_keys, 2); // "2,y" and "3,z"
}

#[test]
fn identical_tables_have_no_differences() {
    let a = table(&[&["1"], &["2"]]);
    let b = table(&[&["1"], &["2"]]);
    let d = diff_rows(&a, &b);
    assert!(d.only_in_a.is_empty());
    assert!(d.only_in_b.is_empty());
    assert_eq!(d.shared_keys, 2);
}

#[test]
fn empty_b_makes_every_a_row_unique() {
    let a = table(&[&["1"], &["2"]]);
    let b = table(&[]);
    let d = diff_rows(&a, &b);
    assert_eq!(d.only_in_a, vec![0, 1]);
    assert!(d.only_in_b.is_empty());
    assert_eq!(d.shared_keys, 0);
}
