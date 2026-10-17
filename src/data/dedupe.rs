use crate::data::{CellValue, DataTable};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepWhich {
    First,
    Last,
}

fn row_key(table: &DataTable, row: usize, key_cols: &[usize]) -> String {
    let cols: Vec<usize> = if key_cols.is_empty() {
        (0..table.col_count()).collect()
    } else {
        key_cols.to_vec()
    };
    let mut key = String::new();
    for c in cols {
        if let Some(v) = table.get(row, c) {
            key.push_str(&v.to_string());
        }
        key.push('\u{1F}');
    }
    key
}

/// Set of original row indices to keep: the first or last occurrence of each
/// key. Shared by [`dedupe_rows`] and [`dedupe_dropped_indices`].
fn keep_set(table: &DataTable, key_cols: &[usize], keep: KeepWhich) -> HashSet<usize> {
    let n = table.row_count();
    let order: Vec<usize> = match keep {
        KeepWhich::First => (0..n).collect(),
        KeepWhich::Last => (0..n).rev().collect(),
    };
    let mut seen: HashSet<String> = HashSet::with_capacity(n);
    let mut keep: HashSet<usize> = HashSet::with_capacity(n);
    for row in order {
        if seen.insert(row_key(table, row, key_cols)) {
            keep.insert(row);
        }
    }
    keep
}

/// Remove duplicate rows by `key_cols` (empty = whole row). Keeps the first or
/// last occurrence of each key; surviving rows stay in original order.
pub fn dedupe_rows(table: &DataTable, key_cols: &[usize], keep: KeepWhich) -> DataTable {
    let kept = keep_set(table, key_cols, keep);
    let mut out = DataTable::empty();
    out.columns = table.columns.clone();
    out.rows = (0..table.row_count())
        .filter(|r| kept.contains(r))
        .map(|r| {
            (0..table.col_count())
                .map(|c| table.get(r, c).cloned().unwrap_or(CellValue::Null))
                .collect()
        })
        .collect();
    out.structural_changes = true;
    out
}

/// Original row indices that [`dedupe_rows`] would drop, in **descending**
/// order so a caller can `delete_row` them in sequence without shifting later
/// targets. Lets the GUI build its undo batch without reconstructing the result.
pub fn dedupe_dropped_indices(
    table: &DataTable,
    key_cols: &[usize],
    keep: KeepWhich,
) -> Vec<usize> {
    let kept = keep_set(table, key_cols, keep);
    (0..table.row_count())
        .rev()
        .filter(|r| !kept.contains(r))
        .collect()
}

#[cfg(test)]
#[path = "dedupe_tests.rs"]
mod tests;
