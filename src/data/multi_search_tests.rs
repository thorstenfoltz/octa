//! Unit tests for [`multi_search`](multi_search). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable, SearchMode};

fn table_with(rows: &[&[&str]], cols: &[&str]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    t.rows = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|s| CellValue::String((*s).to_string()))
                .collect()
        })
        .collect();
    t
}

#[test]
fn search_plain_lowercases() {
    let table = table_with(
        &[&["alice", "ENG"], &["bob", "QA"], &["Eve", "eng"]],
        &["name", "team"],
    );
    let matcher = RowMatcher::new("eng", SearchMode::Plain);
    let hits = search_table(&table, &matcher, "in-mem", None, None, 80);
    // alice -> ENG (col 1), Eve -> eng (col 1).
    assert_eq!(hits.len(), 2, "got hits = {hits:?}");
    assert_eq!(hits[0].row, 0);
    assert_eq!(hits[0].col, 1);
    assert_eq!(hits[1].row, 2);
}

#[test]
fn snippet_anchors_around_match() {
    let long = "x".repeat(60) + "needle" + &"y".repeat(40);
    let matcher = RowMatcher::new("needle", SearchMode::Plain);
    let s = snippet(&long, &matcher, 40);
    assert!(s.contains("needle"));
    assert!(s.starts_with("...") || s.starts_with('x'));
    // 40-char cap, including the leading/trailing "..." markers.
    assert!(s.chars().count() <= 40, "snippet too long: {s:?}");
}
