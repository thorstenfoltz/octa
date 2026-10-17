//! Merge several columns into one.

use super::cell_text;
use crate::data::{CellValue, DataTable};

/// Join the text of `cols` (in the given order) with `separator`, one new
/// `String` cell per row. Out-of-range column indices contribute an empty
/// string.
pub fn merge_columns(table: &DataTable, cols: &[usize], separator: &str) -> Vec<CellValue> {
    let n = table.row_count();
    (0..n)
        .map(|r| {
            let joined = cols
                .iter()
                .map(|&c| cell_text(table, r, c))
                .collect::<Vec<_>>()
                .join(separator);
            CellValue::String(joined)
        })
        .collect()
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
