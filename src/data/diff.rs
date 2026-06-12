//! Row-level data diff between two `DataTable`s.
//!
//! Pure function so the CLI (`--diff`) and the MCP `diff_tables` tool can both
//! call it without a GUI. Keying is text-based and matches the convention used
//! by [`crate::data::duplicates`] and the Compare view's RowHashDiff: each
//! row's cells are formatted via `CellValue::to_string()` and joined with an
//! ASCII unit separator (`\x1F`). A Parquet row and a CSV row with the same
//! displayed values therefore compare equal.
//!
//! Columns are compared **positionally** (all columns, in order). The two
//! tables should share the same column order/names for the result to be
//! meaningful - the same caveat the GUI row-diff states.

use std::collections::HashMap;

use crate::data::DataTable;

/// Outcome of [`diff_rows`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowDiff {
    /// Row indices in `a` whose key is absent from `b` (sorted ascending).
    pub only_in_a: Vec<usize>,
    /// Row indices in `b` whose key is absent from `a` (sorted ascending).
    pub only_in_b: Vec<usize>,
    /// Number of distinct row keys present in both tables.
    pub shared_keys: usize,
}

/// Build the whole-row key for `row` (every column, `\x1F`-joined).
fn row_key(table: &DataTable, row: usize) -> String {
    let mut key = String::new();
    for col in 0..table.col_count() {
        if let Some(v) = table.get(row, col) {
            key.push_str(&v.to_string());
        }
        key.push('\x1F');
    }
    key
}

/// Diff two tables by whole-row content. See module docs for the keying rules.
pub fn diff_rows(a: &DataTable, b: &DataTable) -> RowDiff {
    let a_keys: Vec<String> = (0..a.row_count()).map(|r| row_key(a, r)).collect();
    let b_keys: Vec<String> = (0..b.row_count()).map(|r| row_key(b, r)).collect();

    // Distinct-key membership for the "present on the other side?" test.
    let a_set: HashMap<&str, ()> = a_keys.iter().map(|k| (k.as_str(), ())).collect();
    let b_set: HashMap<&str, ()> = b_keys.iter().map(|k| (k.as_str(), ())).collect();

    let only_in_a: Vec<usize> = a_keys
        .iter()
        .enumerate()
        .filter(|(_, k)| !b_set.contains_key(k.as_str()))
        .map(|(i, _)| i)
        .collect();
    let only_in_b: Vec<usize> = b_keys
        .iter()
        .enumerate()
        .filter(|(_, k)| !a_set.contains_key(k.as_str()))
        .map(|(i, _)| i)
        .collect();

    let shared_keys = a_set.keys().filter(|k| b_set.contains_key(**k)).count();

    RowDiff {
        only_in_a,
        only_in_b,
        shared_keys,
    }
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
