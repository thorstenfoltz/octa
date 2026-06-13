//! Load-time whitespace normalization.
//!
//! [`trim_string_columns`] strips leading and trailing whitespace from every
//! `CellValue::String` cell in a table, in place. Interior whitespace is
//! never touched. It returns the names of the columns where at least one cell
//! actually changed, so the caller can surface a "trimmed N column(s)" notice.
//!
//! This is a normalization pass, not a tracked edit - it mutates `table.rows`
//! directly and does not push to the undo stack. The app gates it behind the
//! `trim_whitespace_on_load` setting.

use crate::data::{CellValue, DataTable};

/// Sparse record of everything a trim pass changed, enough to undo it. Holds
/// the pre-trim values only for the titles/cells that actually changed, so a
/// revert restores the file to its on-disk whitespace without cloning the
/// whole table.
#[derive(Debug, Clone, Default)]
pub struct TrimUndo {
    /// `(col_idx, original_title)` for every column title that was trimmed.
    pub titles: Vec<(usize, String)>,
    /// `(col_idx, [(row_idx, original_value)])` for every string cell trimmed.
    pub cells: Vec<(usize, Vec<(usize, String)>)>,
}

impl TrimUndo {
    /// Whether any title or cell was recorded (i.e. the pass changed anything).
    pub fn is_empty(&self) -> bool {
        self.titles.is_empty() && self.cells.is_empty()
    }
}

/// Trim leading/trailing whitespace from all string cells **and column
/// titles** in `table`. Returns the (trimmed) names of columns that had their
/// title or at least one cell trimmed, in column order.
pub fn trim_string_columns(table: &mut DataTable) -> Vec<String> {
    trim_string_columns_tracked(table).0
}

/// Like [`trim_string_columns`] but also returns a [`TrimUndo`] log capturing
/// the original values, so the caller can offer an "undo the trim" action.
pub fn trim_string_columns_tracked(table: &mut DataTable) -> (Vec<String>, TrimUndo) {
    let col_count = table.columns.len();
    let mut trimmed_cols = vec![false; col_count];
    let mut undo = TrimUndo::default();

    // Column titles.
    for (col_idx, col) in table.columns.iter_mut().enumerate() {
        let trimmed = col.name.trim();
        if trimmed.len() != col.name.len() {
            undo.titles.push((col_idx, col.name.clone()));
            col.name = trimmed.to_string();
            trimmed_cols[col_idx] = true;
        }
    }

    // Cell values. Per-column undo entries collected lazily so untouched
    // columns cost nothing.
    let mut cell_undo: Vec<Option<Vec<(usize, String)>>> = vec![None; col_count];
    for (row_idx, row) in table.rows.iter_mut().enumerate() {
        for (col_idx, cell) in row.iter_mut().enumerate().take(col_count) {
            if let CellValue::String(s) = cell {
                let trimmed = s.trim();
                if trimmed.len() != s.len() {
                    cell_undo[col_idx]
                        .get_or_insert_with(Vec::new)
                        .push((row_idx, s.clone()));
                    *s = trimmed.to_string();
                    trimmed_cols[col_idx] = true;
                }
            }
        }
    }
    for (col_idx, entry) in cell_undo.into_iter().enumerate() {
        if let Some(rows) = entry {
            undo.cells.push((col_idx, rows));
        }
    }

    let changed = trimmed_cols
        .iter()
        .enumerate()
        .filter(|(_, changed)| **changed)
        .filter_map(|(idx, _)| table.columns.get(idx).map(|c| c.name.clone()))
        .collect();
    (changed, undo)
}

#[cfg(test)]
#[path = "trim_tests.rs"]
mod tests;
