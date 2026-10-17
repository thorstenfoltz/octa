//! Fill empty cells from the previous (down) or next (up) non-empty cell.

use super::is_empty;
use crate::data::{CellValue, DataTable};

/// Return `col`'s values with each empty cell replaced by the most recent
/// non-empty value above it (classic spreadsheet "fill down"). Leading empties
/// stay `Null`.
pub fn fill_down(table: &DataTable, col: usize) -> Vec<CellValue> {
    let n = table.row_count();
    let mut out = Vec::with_capacity(n);
    let mut last: Option<CellValue> = None;
    for r in 0..n {
        let cur = table.get(r, col).cloned().unwrap_or(CellValue::Null);
        if is_empty(&cur) {
            out.push(last.clone().unwrap_or(CellValue::Null));
        } else {
            last = Some(cur.clone());
            out.push(cur);
        }
    }
    out
}

/// Mirror of [`fill_down`] working upward: empties take the next non-empty
/// value below them. Trailing empties stay `Null`.
pub fn fill_up(table: &DataTable, col: usize) -> Vec<CellValue> {
    let n = table.row_count();
    let mut out = vec![CellValue::Null; n];
    let mut next: Option<CellValue> = None;
    for r in (0..n).rev() {
        let cur = table.get(r, col).cloned().unwrap_or(CellValue::Null);
        if is_empty(&cur) {
            out[r] = next.clone().unwrap_or(CellValue::Null);
        } else {
            next = Some(cur.clone());
            out[r] = cur;
        }
    }
    out
}

#[cfg(test)]
#[path = "fill_tests.rs"]
mod tests;
