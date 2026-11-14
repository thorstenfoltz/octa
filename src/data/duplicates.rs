//! Find duplicate rows in a `DataTable` by an N-column key.
//!
//! Pure function so integration tests can hit it without a GUI; the
//! `Find Duplicates...` dialog under `app/dialogs/find_duplicates.rs`
//! is the only caller in the binary.
//!
//! Each row's selected columns are keyed by [`cell_key`] and joined with an
//! ASCII unit separator: a number in a number column by its value (`1` and
//! `1.0` are one value), anything else by its text.

use std::collections::HashMap;

use crate::data::{CellValue, DataTable};

/// One cell as a duplicate key. In a number column a number counts by its
/// value, so a whole number read as `1` in one row and `1.0` in the next is
/// one value; anything else counts by its text. Shared by Find duplicates,
/// Drop duplicates and Unique columns, so the three agree.
pub(crate) fn cell_key(data_type: &str, cell: &CellValue) -> String {
    crate::data::is_numeric_data_type(data_type)
        .then(|| crate::data::decimal::value_key(cell))
        .flatten()
        .unwrap_or_else(|| cell.to_string())
}

/// Return the row indices (sorted, ascending) of every row that shares
/// its key-column values with at least one other row in the table.
///
/// `key_cols` empty or table empty -> empty result. Out-of-range
/// indices in `key_cols` are skipped silently rather than erroring;
/// the caller (a UI dialog) has already validated against the live
/// column count.
pub fn find_duplicate_rows(table: &DataTable, key_cols: &[usize]) -> Vec<usize> {
    let row_count = table.row_count();
    if row_count == 0 || key_cols.is_empty() {
        return Vec::new();
    }
    let col_count = table.col_count();
    let valid_cols: Vec<usize> = key_cols
        .iter()
        .copied()
        .filter(|&c| c < col_count)
        .collect();
    if valid_cols.is_empty() {
        return Vec::new();
    }

    let mut groups: HashMap<String, Vec<usize>> = HashMap::with_capacity(row_count.min(1024));
    let mut key = String::new();
    for row in 0..row_count {
        key.clear();
        for &col in &valid_cols {
            if let Some(v) = table.get(row, col) {
                key.push_str(&cell_key(&table.columns[col].data_type, v));
            }
            key.push('\x1F');
        }
        groups.entry(key.clone()).or_default().push(row);
    }

    let mut out: Vec<usize> = groups
        .into_values()
        .filter(|rows| rows.len() >= 2)
        .flatten()
        .collect();
    out.sort_unstable();
    out
}
