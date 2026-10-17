//! Pure logic for the "Filter to marked" action: given the set of colour marks
//! on a table, work out which rows and columns should remain visible.
//!
//! Marked rows always keep their row; marked columns always keep their column.
//! Marked *cells* are ambiguous, so the caller picks a [`MarkFilterCellMode`].

use crate::data::{MarkColor, MarkKey};
use std::collections::{HashMap, HashSet};

/// How "Filter to marked" treats marked cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum MarkFilterCellMode {
    /// Keep the cell's row (default). Columns are unaffected by cell marks.
    #[default]
    RowsOnly,
    /// Keep the cell's column. Rows are unaffected by cell marks.
    Columns,
    /// Keep both the cell's row and its column.
    Intersect,
}

impl MarkFilterCellMode {
    pub const ALL: &'static [MarkFilterCellMode] = &[
        MarkFilterCellMode::RowsOnly,
        MarkFilterCellMode::Columns,
        MarkFilterCellMode::Intersect,
    ];

    /// i18n key for the localized label (under `[mark_filter_cell_mode]`).
    pub fn i18n_key(self) -> &'static str {
        match self {
            MarkFilterCellMode::RowsOnly => "mark_filter_cell_mode.rows_only",
            MarkFilterCellMode::Columns => "mark_filter_cell_mode.columns",
            MarkFilterCellMode::Intersect => "mark_filter_cell_mode.intersect",
        }
    }
}

/// The rows and columns to keep when filtering to marks. An empty `rows` means
/// "no row constraint" (keep all rows); an empty `cols` means "no column
/// constraint" (keep all columns). The caller enforces that interpretation.
pub struct MarkKeepSet {
    pub rows: HashSet<usize>,
    pub cols: HashSet<usize>,
}

/// Compute the rows/columns to keep from the current marks under `mode`.
pub fn mark_keep_set(marks: &HashMap<MarkKey, MarkColor>, mode: MarkFilterCellMode) -> MarkKeepSet {
    let mut rows = HashSet::new();
    let mut cols = HashSet::new();
    for key in marks.keys() {
        match key {
            MarkKey::Row(r) => {
                rows.insert(*r);
            }
            MarkKey::Column(c) => {
                cols.insert(*c);
            }
            MarkKey::Cell(r, c) => match mode {
                MarkFilterCellMode::RowsOnly => {
                    rows.insert(*r);
                }
                MarkFilterCellMode::Columns => {
                    cols.insert(*c);
                }
                MarkFilterCellMode::Intersect => {
                    rows.insert(*r);
                    cols.insert(*c);
                }
            },
        }
    }
    MarkKeepSet { rows, cols }
}

#[cfg(test)]
#[path = "mark_filter_tests.rs"]
mod tests;
