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
mod tests {
    use super::*;

    fn marks(items: &[MarkKey]) -> HashMap<MarkKey, MarkColor> {
        items
            .iter()
            .cloned()
            .map(|k| (k, MarkColor::Yellow))
            .collect()
    }

    #[test]
    fn row_marks_kept() {
        let m = marks(&[MarkKey::Row(2), MarkKey::Row(5)]);
        let ks = mark_keep_set(&m, MarkFilterCellMode::RowsOnly);
        assert_eq!(ks.rows, HashSet::from([2, 5]));
        assert!(ks.cols.is_empty());
    }

    #[test]
    fn column_marks_kept() {
        let m = marks(&[MarkKey::Column(1)]);
        let ks = mark_keep_set(&m, MarkFilterCellMode::RowsOnly);
        assert_eq!(ks.cols, HashSet::from([1]));
        assert!(ks.rows.is_empty());
    }

    #[test]
    fn cell_rows_only() {
        let m = marks(&[MarkKey::Cell(3, 2)]);
        let ks = mark_keep_set(&m, MarkFilterCellMode::RowsOnly);
        assert_eq!(ks.rows, HashSet::from([3]));
        assert!(ks.cols.is_empty());
    }

    #[test]
    fn cell_columns_only() {
        let m = marks(&[MarkKey::Cell(3, 2)]);
        let ks = mark_keep_set(&m, MarkFilterCellMode::Columns);
        assert_eq!(ks.cols, HashSet::from([2]));
        assert!(ks.rows.is_empty());
    }

    #[test]
    fn cell_intersect() {
        let m = marks(&[MarkKey::Cell(3, 2)]);
        let ks = mark_keep_set(&m, MarkFilterCellMode::Intersect);
        assert_eq!(ks.rows, HashSet::from([3]));
        assert_eq!(ks.cols, HashSet::from([2]));
    }

    #[test]
    fn union_of_kinds() {
        let m = marks(&[MarkKey::Row(1), MarkKey::Column(4), MarkKey::Cell(2, 0)]);
        let ks = mark_keep_set(&m, MarkFilterCellMode::Intersect);
        assert_eq!(ks.rows, HashSet::from([1, 2]));
        assert_eq!(ks.cols, HashSet::from([0, 4]));
    }
}
