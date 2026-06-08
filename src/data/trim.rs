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
mod tests {
    use super::*;
    use crate::data::ColumnInfo;

    fn table(cols: &[&str], rows: Vec<Vec<CellValue>>) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = cols
            .iter()
            .map(|n| ColumnInfo {
                name: n.to_string(),
                data_type: "Utf8".to_string(),
            })
            .collect();
        t.rows = rows;
        t
    }

    #[test]
    fn trims_leading_and_trailing() {
        let mut t = table(
            &["a", "b"],
            vec![vec![
                CellValue::String("  hi  ".into()),
                CellValue::String("ok".into()),
            ]],
        );
        let changed = trim_string_columns(&mut t);
        assert_eq!(changed, vec!["a".to_string()]);
        assert_eq!(t.rows[0][0], CellValue::String("hi".into()));
        assert_eq!(t.rows[0][1], CellValue::String("ok".into()));
    }

    #[test]
    fn preserves_interior_whitespace() {
        let mut t = table(&["a"], vec![vec![CellValue::String("  a  b  ".into())]]);
        let changed = trim_string_columns(&mut t);
        assert_eq!(changed, vec!["a".to_string()]);
        assert_eq!(t.rows[0][0], CellValue::String("a  b".into()));
    }

    #[test]
    fn leaves_non_string_cells() {
        let mut t = table(
            &["n", "s"],
            vec![vec![CellValue::Int(5), CellValue::String("x".into())]],
        );
        let changed = trim_string_columns(&mut t);
        assert!(changed.is_empty());
        assert_eq!(t.rows[0][0], CellValue::Int(5));
    }

    #[test]
    fn reports_each_affected_column() {
        let mut t = table(
            &["a", "b", "c"],
            vec![
                vec![
                    CellValue::String("x ".into()),
                    CellValue::String("y".into()),
                    CellValue::String(" z".into()),
                ],
                vec![
                    CellValue::String("p".into()),
                    CellValue::String("q".into()),
                    CellValue::String("r".into()),
                ],
            ],
        );
        let changed = trim_string_columns(&mut t);
        assert_eq!(changed, vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn undo_log_restores_original_values() {
        let mut t = table(
            &[" a ", "b"],
            vec![
                vec![
                    CellValue::String("  hi  ".into()),
                    CellValue::String("ok".into()),
                ],
                vec![
                    CellValue::String("x".into()),
                    CellValue::String(" y ".into()),
                ],
            ],
        );
        let (changed, undo) = trim_string_columns_tracked(&mut t);
        assert!(!changed.is_empty());
        assert!(!undo.is_empty());
        // Trim happened.
        assert_eq!(t.columns[0].name, "a");
        assert_eq!(t.rows[0][0], CellValue::String("hi".into()));
        assert_eq!(t.rows[1][1], CellValue::String("y".into()));
        // Replay the undo log by hand and confirm it restores the originals.
        for (col_idx, title) in &undo.titles {
            t.columns[*col_idx].name = title.clone();
        }
        for (col_idx, cells) in &undo.cells {
            for (row_idx, val) in cells {
                t.rows[*row_idx][*col_idx] = CellValue::String(val.clone());
            }
        }
        assert_eq!(t.columns[0].name, " a ");
        assert_eq!(t.rows[0][0], CellValue::String("  hi  ".into()));
        assert_eq!(t.rows[1][1], CellValue::String(" y ".into()));
    }
}
