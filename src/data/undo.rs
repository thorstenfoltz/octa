//! Undo/redo for [`DataTable`]: the [`UndoAction`] log entry and the
//! `undo`/`redo`/`coalesce_undo_since` methods. Split out of `data/mod.rs`.
//! The structural mutations that *push* these actions (insert/delete/move/
//! reorder rows and columns, `apply_order`/`invert_order`) stay in the core
//! module; this file only reverses/replays them.

use std::collections::HashMap;

use super::{CellValue, ColumnInfo, DataTable, MarkColor, MarkKey};

/// An undoable action on the data table.
#[derive(Debug, Clone)]
pub enum UndoAction {
    CellEdit {
        row: usize,
        col: usize,
        old_value: CellValue,
        new_value: CellValue,
    },
    InsertRow {
        index: usize,
    },
    DeleteRow {
        index: usize,
        data: Vec<CellValue>,
        /// Source-row identity if this row came from a DB-backed table.
        /// Restored alongside the row data on undo so subsequent saves don't
        /// mistake the resurrected row for a fresh INSERT.
        db_tag: Option<i64>,
    },
    InsertColumn {
        index: usize,
        name: String,
        data_type: String,
    },
    DeleteColumn {
        index: usize,
        name: String,
        data_type: String,
        data: Vec<CellValue>,
    },
    MoveRow {
        from: usize,
        to: usize,
    },
    MoveColumn {
        from: usize,
        to: usize,
    },
    /// Bulk column reorder via permutation: `order[new_pos] = old_pos`.
    /// Stored as the *forward* mapping; undo reorders by the inverse.
    ReorderColumns {
        order: Vec<usize>,
    },
    SetMark {
        key: MarkKey,
        old_color: Option<MarkColor>,
        new_color: Option<MarkColor>,
    },
    ConvertColumn {
        col_idx: usize,
        old_type: String,
        new_type: String,
        old_values: Vec<CellValue>,
        new_values: Vec<CellValue>,
    },
    /// A column title change (from bulk rename-from-mapping). Undo restores the
    /// old name, redo re-applies the new one.
    RenameColumn {
        index: usize,
        old: String,
        new: String,
    },
    /// A group of actions applied as one logical operation, so a single
    /// undo/redo reverts/replays all of them (e.g. anonymising a column writes
    /// one cell edit per row but should undo in one step).
    Batch(Vec<UndoAction>),
}

impl DataTable {
    /// Undo the last action. Returns true if something was undone.
    pub fn undo(&mut self) -> bool {
        // Batch: undo each child via the normal single-action path (reverse
        // order), then record one Batch on the redo stack.
        if matches!(self.undo_stack.last(), Some(UndoAction::Batch(_))) {
            let Some(UndoAction::Batch(children)) = self.undo_stack.pop() else {
                unreachable!()
            };
            let mut redone: Vec<UndoAction> = Vec::with_capacity(children.len());
            for child in children.into_iter().rev() {
                self.undo_stack.push(child);
                self.undo();
                if let Some(done) = self.redo_stack.pop() {
                    redone.push(done);
                }
            }
            redone.reverse();
            self.redo_stack.push(UndoAction::Batch(redone));
            return true;
        }
        if let Some(action) = self.undo_stack.pop() {
            match action.clone() {
                UndoAction::CellEdit {
                    row,
                    col,
                    old_value,
                    ..
                } => {
                    self.edits.insert((row, col), old_value);
                }
                UndoAction::InsertRow { index } => {
                    if index < self.rows.len() {
                        self.rows.remove(index);
                        if let Some(meta) = self.db_meta.as_mut()
                            && index < meta.row_tags.len()
                        {
                            meta.row_tags.remove(index);
                        }
                        // Shift edits back
                        let mut new_edits = HashMap::new();
                        for (&(r, c), v) in &self.edits {
                            if r < index {
                                new_edits.insert((r, c), v.clone());
                            } else if r > index {
                                new_edits.insert((r - 1, c), v.clone());
                            }
                        }
                        self.edits = new_edits;
                    }
                }
                UndoAction::DeleteRow {
                    index,
                    data,
                    db_tag,
                } => {
                    self.rows.insert(index, data);
                    if let Some(meta) = self.db_meta.as_mut() {
                        let ins = index.min(meta.row_tags.len());
                        meta.row_tags.insert(ins, db_tag);
                    }
                    // Shift edits forward
                    let mut new_edits = HashMap::new();
                    for (&(r, c), v) in &self.edits {
                        if r < index {
                            new_edits.insert((r, c), v.clone());
                        } else {
                            new_edits.insert((r + 1, c), v.clone());
                        }
                    }
                    self.edits = new_edits;
                }
                UndoAction::InsertColumn { index, .. } => {
                    if index < self.columns.len() {
                        self.columns.remove(index);
                        for row in &mut self.rows {
                            if index < row.len() {
                                row.remove(index);
                            }
                        }
                        let mut new_edits = HashMap::new();
                        for (&(r, c), v) in &self.edits {
                            if c < index {
                                new_edits.insert((r, c), v.clone());
                            } else if c > index {
                                new_edits.insert((r, c - 1), v.clone());
                            }
                        }
                        self.edits = new_edits;
                    }
                }
                UndoAction::DeleteColumn {
                    index,
                    name,
                    data_type,
                    data,
                } => {
                    self.columns.insert(index, ColumnInfo { name, data_type });
                    for (row_idx, row) in self.rows.iter_mut().enumerate() {
                        let val = data.get(row_idx).cloned().unwrap_or(CellValue::Null);
                        let ins = index.min(row.len());
                        row.insert(ins, val);
                    }
                    let mut new_edits = HashMap::new();
                    for (&(r, c), v) in &self.edits {
                        if c < index {
                            new_edits.insert((r, c), v.clone());
                        } else {
                            new_edits.insert((r, c + 1), v.clone());
                        }
                    }
                    self.edits = new_edits;
                }
                UndoAction::MoveRow { from, to } => {
                    // Reverse the move
                    if to < self.rows.len() && from < self.rows.len() {
                        let row = self.rows.remove(to);
                        self.rows.insert(from, row);
                    }
                }
                UndoAction::MoveColumn { from, to } => {
                    if to < self.columns.len() && from < self.columns.len() {
                        let col = self.columns.remove(to);
                        self.columns.insert(from, col);
                        for row in &mut self.rows {
                            if to < row.len() {
                                let val = row.remove(to);
                                let ins = from.min(row.len());
                                row.insert(ins, val);
                            }
                        }
                    }
                }
                UndoAction::ReorderColumns { ref order } => {
                    let inv = Self::invert_order(order);
                    self.apply_order(&inv);
                    self.structural_changes = true;
                }
                UndoAction::SetMark { key, old_color, .. } => match old_color {
                    Some(c) => {
                        self.marks.insert(key, c);
                    }
                    None => {
                        self.marks.remove(&key);
                    }
                },
                UndoAction::ConvertColumn {
                    col_idx,
                    ref old_type,
                    ref old_values,
                    ..
                } => {
                    if col_idx < self.columns.len() {
                        self.columns[col_idx].data_type = old_type.clone();
                        for (row_idx, row) in self.rows.iter_mut().enumerate() {
                            if col_idx < row.len()
                                && let Some(val) = old_values.get(row_idx)
                            {
                                row[col_idx] = val.clone();
                            }
                        }
                        // Restore edits for this column from old values
                        let edit_keys: Vec<(usize, usize)> = self
                            .edits
                            .keys()
                            .filter(|(_, c)| *c == col_idx)
                            .copied()
                            .collect();
                        for key in edit_keys {
                            if let Some(val) = old_values.get(key.0) {
                                self.edits.insert(key, val.clone());
                            }
                        }
                    }
                }
                UndoAction::RenameColumn { index, ref old, .. } => {
                    if index < self.columns.len() {
                        self.columns[index].name = old.clone();
                        self.structural_changes = true;
                    }
                }
                UndoAction::Batch(_) => {}
            }
            self.redo_stack.push(action);
            true
        } else {
            false
        }
    }

    /// Redo the last undone action. Returns true if something was redone.
    pub fn redo(&mut self) -> bool {
        // Batch: redo each child via the normal single-action path (forward
        // order), then record one Batch on the undo stack.
        if matches!(self.redo_stack.last(), Some(UndoAction::Batch(_))) {
            let Some(UndoAction::Batch(children)) = self.redo_stack.pop() else {
                unreachable!()
            };
            let mut undone: Vec<UndoAction> = Vec::with_capacity(children.len());
            for child in children.into_iter() {
                self.redo_stack.push(child);
                self.redo();
                if let Some(done) = self.undo_stack.pop() {
                    undone.push(done);
                }
            }
            self.undo_stack.push(UndoAction::Batch(undone));
            return true;
        }
        if let Some(action) = self.redo_stack.pop() {
            match action.clone() {
                UndoAction::CellEdit {
                    row,
                    col,
                    new_value,
                    ..
                } => {
                    self.edits.insert((row, col), new_value);
                }
                UndoAction::InsertRow { index } => {
                    let row = vec![CellValue::Null; self.columns.len()];
                    let idx = index.min(self.rows.len());
                    self.rows.insert(idx, row);
                    if let Some(meta) = self.db_meta.as_mut() {
                        meta.row_tags.insert(idx.min(meta.row_tags.len()), None);
                    }
                    let mut new_edits = HashMap::new();
                    for (&(r, c), v) in &self.edits {
                        if r < idx {
                            new_edits.insert((r, c), v.clone());
                        } else {
                            new_edits.insert((r + 1, c), v.clone());
                        }
                    }
                    self.edits = new_edits;
                }
                UndoAction::DeleteRow { index, .. } => {
                    if index < self.rows.len() {
                        self.rows.remove(index);
                        if let Some(meta) = self.db_meta.as_mut()
                            && index < meta.row_tags.len()
                        {
                            meta.row_tags.remove(index);
                        }
                        let mut new_edits = HashMap::new();
                        for (&(r, c), v) in &self.edits {
                            if r < index {
                                new_edits.insert((r, c), v.clone());
                            } else if r > index {
                                new_edits.insert((r - 1, c), v.clone());
                            }
                        }
                        self.edits = new_edits;
                    }
                }
                UndoAction::InsertColumn {
                    index,
                    name,
                    data_type,
                } => {
                    let idx = index.min(self.columns.len());
                    self.columns.insert(idx, ColumnInfo { name, data_type });
                    for row in &mut self.rows {
                        row.insert(idx, CellValue::Null);
                    }
                    let mut new_edits = HashMap::new();
                    for (&(r, c), v) in &self.edits {
                        if c < idx {
                            new_edits.insert((r, c), v.clone());
                        } else {
                            new_edits.insert((r, c + 1), v.clone());
                        }
                    }
                    self.edits = new_edits;
                }
                UndoAction::DeleteColumn { index, .. } => {
                    if index < self.columns.len() {
                        self.columns.remove(index);
                        for row in &mut self.rows {
                            if index < row.len() {
                                row.remove(index);
                            }
                        }
                        let mut new_edits = HashMap::new();
                        for (&(r, c), v) in &self.edits {
                            if c < index {
                                new_edits.insert((r, c), v.clone());
                            } else if c > index {
                                new_edits.insert((r, c - 1), v.clone());
                            }
                        }
                        self.edits = new_edits;
                    }
                }
                UndoAction::MoveRow { from, to } => {
                    if from < self.rows.len() && to < self.rows.len() {
                        let row = self.rows.remove(from);
                        self.rows.insert(to, row);
                    }
                }
                UndoAction::MoveColumn { from, to } => {
                    if from < self.columns.len() && to < self.columns.len() {
                        let col = self.columns.remove(from);
                        self.columns.insert(to, col);
                        for row in &mut self.rows {
                            if from < row.len() {
                                let val = row.remove(from);
                                let ins = to.min(row.len());
                                row.insert(ins, val);
                            }
                        }
                    }
                }
                UndoAction::ReorderColumns { ref order } => {
                    self.apply_order(order);
                    self.structural_changes = true;
                }
                UndoAction::SetMark { key, new_color, .. } => match new_color {
                    Some(c) => {
                        self.marks.insert(key, c);
                    }
                    None => {
                        self.marks.remove(&key);
                    }
                },
                UndoAction::ConvertColumn {
                    col_idx,
                    ref new_type,
                    ref new_values,
                    ..
                } => {
                    if col_idx < self.columns.len() {
                        self.columns[col_idx].data_type = new_type.clone();
                        for (row_idx, row) in self.rows.iter_mut().enumerate() {
                            if col_idx < row.len()
                                && let Some(val) = new_values.get(row_idx)
                            {
                                row[col_idx] = val.clone();
                            }
                        }
                        // Restore edits for this column from new values
                        let edit_keys: Vec<(usize, usize)> = self
                            .edits
                            .keys()
                            .filter(|(_, c)| *c == col_idx)
                            .copied()
                            .collect();
                        for key in edit_keys {
                            if let Some(val) = new_values.get(key.0) {
                                self.edits.insert(key, val.clone());
                            }
                        }
                    }
                }
                UndoAction::RenameColumn { index, ref new, .. } => {
                    if index < self.columns.len() {
                        self.columns[index].name = new.clone();
                        self.structural_changes = true;
                    }
                }
                UndoAction::Batch(_) => {}
            }
            self.undo_stack.push(action);
            true
        } else {
            false
        }
    }

    /// Fold every undo action pushed since `start_len` into a single
    /// [`UndoAction::Batch`], so the whole operation undoes/redoes in one step.
    /// No-op when 0 or 1 actions were pushed since `start_len`.
    pub fn coalesce_undo_since(&mut self, start_len: usize) {
        if self.undo_stack.len() <= start_len + 1 {
            return;
        }
        let children: Vec<UndoAction> = self.undo_stack.split_off(start_len);
        self.undo_stack.push(UndoAction::Batch(children));
    }
}

#[cfg(test)]
mod undo_tests {
    use crate::data::*;

    fn one_col(values: &[&str]) -> DataTable {
        let mut t = DataTable::empty();
        t.columns.push(ColumnInfo {
            name: "a".into(),
            data_type: "Utf8".into(),
        });
        t.rows = values
            .iter()
            .map(|v| vec![CellValue::String(v.to_string())])
            .collect();
        t
    }

    #[test]
    fn batch_undo_is_one_step() {
        let mut t = one_col(&["x", "y"]);
        let start = t.undo_stack.len();
        t.set(0, 0, CellValue::String("X".into()));
        t.set(1, 0, CellValue::String("Y".into()));
        t.coalesce_undo_since(start);
        assert_eq!(t.undo_stack.len(), start + 1);
        assert!(t.undo());
        assert_eq!(t.get(0, 0).unwrap().to_string(), "x");
        assert_eq!(t.get(1, 0).unwrap().to_string(), "y");
        assert!(t.redo());
        assert_eq!(t.get(0, 0).unwrap().to_string(), "X");
        assert_eq!(t.get(1, 0).unwrap().to_string(), "Y");
    }

    #[test]
    fn coalesce_noop_for_zero_or_one() {
        let mut t = one_col(&["x"]);
        let start = t.undo_stack.len();
        t.coalesce_undo_since(start);
        assert_eq!(t.undo_stack.len(), start);
        t.set(0, 0, CellValue::String("Z".into()));
        let one = t.undo_stack.len();
        t.coalesce_undo_since(one - 1);
        assert_eq!(t.undo_stack.len(), one);
    }

    #[test]
    fn rename_column_undo_redo_round_trip() {
        let mut t = one_col(&["x"]);
        t.rename_column(0, "renamed".into());
        assert_eq!(t.columns[0].name, "renamed");
        assert!(t.structural_changes);
        assert!(t.undo());
        assert_eq!(t.columns[0].name, "a");
        assert!(t.redo());
        assert_eq!(t.columns[0].name, "renamed");
    }

    #[test]
    fn rename_column_noop_when_unchanged() {
        let mut t = one_col(&["x"]);
        let start = t.undo_stack.len();
        t.rename_column(0, "a".into());
        assert_eq!(t.undo_stack.len(), start, "no-op rename must not push undo");
    }

    #[test]
    fn bulk_rename_undoes_as_one_step() {
        let mut t = DataTable::empty();
        for n in ["a", "b"] {
            t.columns.push(ColumnInfo {
                name: n.into(),
                data_type: "Utf8".into(),
            });
        }
        let start = t.undo_stack.len();
        t.rename_column(0, "x".into());
        t.rename_column(1, "y".into());
        t.coalesce_undo_since(start);
        assert_eq!(t.undo_stack.len(), start + 1);
        assert!(t.undo());
        assert_eq!(t.columns[0].name, "a");
        assert_eq!(t.columns[1].name, "b");
        assert!(t.redo());
        assert_eq!(t.columns[0].name, "x");
        assert_eq!(t.columns[1].name, "y");
    }
}
