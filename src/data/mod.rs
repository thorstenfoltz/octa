pub mod chart;
pub mod chart_export;
pub mod compare;
pub mod compare_schemas;
pub mod conditional_format;
pub mod correlation;
pub mod date_infer;
pub mod dedupe;
pub mod describe;
pub mod diff;
pub mod duplicates;
pub mod encoding;
pub mod formulas;
pub mod fuzzy_duplicates;
pub mod geo_detect;
pub mod impute;
pub mod inventory;
pub mod join;
pub mod json_util;
pub mod links;
pub mod mark_filter;
pub mod multi_search;
pub mod num_format;
pub mod outliers;
pub mod partition;
pub mod pii;
pub mod pivot;
pub mod quality;
pub mod rename_map;
pub mod sample;
pub mod schema_export;
pub mod search;
pub mod summary;
pub mod table_edits;
pub mod time_calc;
pub mod transform;
pub mod transpose;
pub mod trim;
pub mod union;
pub mod unique_columns;
pub mod validate_schema;
pub mod validation;
pub mod value_frequency;

pub use formulas::{
    FormulaBadCell, FormulaOutcome, evaluate_formula, evaluate_formula_with_diagnostics,
};
pub use table_edits::{EditColRef, EditOp, apply_edit_ops, compute_column_values};

use std::collections::HashMap;

mod cell_value;
mod marks;
mod undo;
mod view_enums;

pub use cell_value::{
    BinaryDisplayMode, CellValue, can_convert_value, cmp_cell_values, convert_value,
    is_numeric_data_type, wildcard_to_regex,
};
pub use marks::{MarkColor, MarkKey};
pub use undo::UndoAction;
pub use view_enums::{
    CompareMode, MapMode, MarkdownLayout, SearchMode, SearchResultMode, ViewMode,
};

/// Column metadata
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
}

/// Metadata associating rows of a `DataTable` with rows in a source database
/// table. Set by the SQLite / DuckDB readers and consumed by their writers to
/// produce INSERT / UPDATE / DELETE statements rather than overwriting.
#[derive(Debug, Clone)]
pub struct DbRowMeta {
    /// Name of the source table.
    pub table_name: String,
    /// Source schema for DuckDB (e.g. `Some("analytics")`); `None` for SQLite
    /// (no schema concept) and for DuckDB's default `main` schema. Read-time
    /// readers and the diff-save writers both consult this so a table loaded
    /// from `analytics.q4_sales` is written back to the same schema rather
    /// than `main`.
    pub schema: Option<String>,
    /// Per-row source identity, parallel to `DataTable.rows`.
    /// `None` = inserted by the user since load (becomes an INSERT on save).
    /// `Some(tag)` = original row from the DB (rowid for SQLite, sequential for DuckDB).
    pub row_tags: Vec<Option<i64>>,
    /// Snapshot of original row values keyed by tag, used to detect cell-level
    /// changes for UPDATE statements.
    pub original: HashMap<i64, Vec<CellValue>>,
    /// Original column names at load time. Save fails if columns no longer
    /// match - schema-altering edits aren't supported on DB-backed tables.
    pub original_columns: Vec<String>,
}

/// The core data model: an unbounded table of cells.
/// Rows and columns are stored as a flat Vec-of-Vecs (row-major).
/// Edits are tracked separately so the original data is preserved.
#[derive(Debug, Clone)]
pub struct DataTable {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<CellValue>>,
    /// Tracks edited cells: (row, col) -> new value
    pub edits: HashMap<(usize, usize), CellValue>,
    /// Source file path (if any)
    pub source_path: Option<String>,
    /// Format name that produced this table
    pub format_name: Option<String>,
    /// Whether structural changes have been made (add/delete/move rows/cols)
    pub structural_changes: bool,
    /// Total row count in the source file (when loading was truncated)
    pub total_rows: Option<usize>,
    /// File-level index of the first loaded row (for windowed loading)
    pub row_offset: usize,
    /// Color marks on cells, rows, and columns
    pub marks: HashMap<MarkKey, MarkColor>,
    /// Undo stack
    pub undo_stack: Vec<UndoAction>,
    /// Redo stack (cleared on new action)
    pub redo_stack: Vec<UndoAction>,
    /// Per-row identity for tables loaded from a database.
    /// Kept aligned with `rows` by structural row operations.
    pub db_meta: Option<DbRowMeta>,
}

impl DataTable {
    pub fn empty() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            edits: HashMap::new(),
            source_path: None,
            format_name: None,
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn col_count(&self) -> usize {
        self.columns.len()
    }

    /// Get a cell value, respecting edits.
    pub fn get(&self, row: usize, col: usize) -> Option<&CellValue> {
        if let Some(edited) = self.edits.get(&(row, col)) {
            return Some(edited);
        }
        self.rows.get(row).and_then(|r| r.get(col))
    }

    /// Set a cell value (tracked as an edit), with undo support.
    pub fn set(&mut self, row: usize, col: usize, value: CellValue) {
        if row < self.rows.len() && col < self.columns.len() {
            let old_value = self.get(row, col).cloned().unwrap_or(CellValue::Null);
            self.undo_stack.push(UndoAction::CellEdit {
                row,
                col,
                old_value,
                new_value: value.clone(),
            });
            self.redo_stack.clear();
            self.edits.insert((row, col), value);
        }
    }

    /// Check if a cell has been edited.
    pub fn is_edited(&self, row: usize, col: usize) -> bool {
        self.edits.contains_key(&(row, col))
    }

    /// Discard all edits.
    pub fn discard_edits(&mut self) {
        self.edits.clear();
    }

    /// Insert a new empty row at the given index.
    /// If index >= row_count, appends at the end.
    pub fn insert_row(&mut self, index: usize) {
        self.structural_changes = true;
        let row = vec![CellValue::Null; self.columns.len()];
        let idx = index.min(self.rows.len());
        self.undo_stack.push(UndoAction::InsertRow { index: idx });
        self.redo_stack.clear();
        self.rows.insert(idx, row);
        if let Some(meta) = self.db_meta.as_mut() {
            meta.row_tags.insert(idx, None);
        }
        // Shift edits at or after the insertion point down by 1
        let mut new_edits = HashMap::new();
        for (&(r, c), v) in &self.edits {
            if r < idx {
                new_edits.insert((r, c), v.clone());
            } else {
                new_edits.insert((r + 1, c), v.clone());
            }
        }
        self.edits = new_edits;
        // Shift row marks
        let mark_keys: Vec<MarkKey> = self.marks.keys().cloned().collect();
        let mut new_marks = HashMap::new();
        for key in mark_keys {
            let color = self.marks.remove(&key).unwrap();
            let new_key = match key {
                MarkKey::Row(r) if r >= idx => MarkKey::Row(r + 1),
                MarkKey::Cell(r, c) if r >= idx => MarkKey::Cell(r + 1, c),
                other => other,
            };
            new_marks.insert(new_key, color);
        }
        self.marks = new_marks;
    }

    /// Delete a row by index.
    pub fn delete_row(&mut self, index: usize) {
        self.structural_changes = true;
        if index < self.rows.len() {
            // Build the full row data (with edits applied) for undo
            let row_data: Vec<CellValue> = (0..self.columns.len())
                .map(|c| self.get(index, c).cloned().unwrap_or(CellValue::Null))
                .collect();
            let db_tag = self
                .db_meta
                .as_ref()
                .and_then(|m| m.row_tags.get(index).copied())
                .flatten();
            self.undo_stack.push(UndoAction::DeleteRow {
                index,
                data: row_data,
                db_tag,
            });
            self.redo_stack.clear();
            self.rows.remove(index);
            if let Some(meta) = self.db_meta.as_mut()
                && index < meta.row_tags.len()
            {
                meta.row_tags.remove(index);
            }
            // Clean up edits referencing this row or higher
            let mut new_edits = HashMap::new();
            for (&(r, c), v) in &self.edits {
                if r < index {
                    new_edits.insert((r, c), v.clone());
                } else if r > index {
                    new_edits.insert((r - 1, c), v.clone());
                }
            }
            self.edits = new_edits;
            // Shift row marks
            let mark_keys: Vec<MarkKey> = self.marks.keys().cloned().collect();
            let mut new_marks = HashMap::new();
            for key in mark_keys {
                let color = self.marks.remove(&key).unwrap();
                match key {
                    MarkKey::Row(r) if r == index => continue,
                    MarkKey::Cell(r, _) if r == index => continue,
                    MarkKey::Row(r) if r > index => {
                        new_marks.insert(MarkKey::Row(r - 1), color);
                    }
                    MarkKey::Cell(r, c) if r > index => {
                        new_marks.insert(MarkKey::Cell(r - 1, c), color);
                    }
                    other => {
                        new_marks.insert(other, color);
                    }
                }
            }
            self.marks = new_marks;
        }
    }

    /// Insert a new column at the given index with a given name and data type.
    /// If index >= col_count, appends at the end.
    pub fn insert_column(&mut self, index: usize, name: String, data_type: String) {
        self.structural_changes = true;
        let idx = index.min(self.columns.len());
        self.undo_stack.push(UndoAction::InsertColumn {
            index: idx,
            name: name.clone(),
            data_type: data_type.clone(),
        });
        self.redo_stack.clear();
        self.columns.insert(idx, ColumnInfo { name, data_type });
        for row in &mut self.rows {
            row.insert(idx, CellValue::Null);
        }
        // Shift edits at or after the insertion point right by 1
        let mut new_edits = HashMap::new();
        for (&(r, c), v) in &self.edits {
            if c < idx {
                new_edits.insert((r, c), v.clone());
            } else {
                new_edits.insert((r, c + 1), v.clone());
            }
        }
        self.edits = new_edits;
        // Shift column marks
        let mark_keys: Vec<MarkKey> = self.marks.keys().cloned().collect();
        let mut new_marks = HashMap::new();
        for key in mark_keys {
            let color = self.marks.remove(&key).unwrap();
            let new_key = match key {
                MarkKey::Column(c) if c >= idx => MarkKey::Column(c + 1),
                MarkKey::Cell(r, c) if c >= idx => MarkKey::Cell(r, c + 1),
                other => other,
            };
            new_marks.insert(new_key, color);
        }
        self.marks = new_marks;
    }

    /// Delete a column by index.
    pub fn delete_column(&mut self, col_idx: usize) {
        self.structural_changes = true;
        if col_idx < self.columns.len() {
            let col_info = &self.columns[col_idx];
            let col_data: Vec<CellValue> = (0..self.rows.len())
                .map(|r| self.get(r, col_idx).cloned().unwrap_or(CellValue::Null))
                .collect();
            self.undo_stack.push(UndoAction::DeleteColumn {
                index: col_idx,
                name: col_info.name.clone(),
                data_type: col_info.data_type.clone(),
                data: col_data,
            });
            self.redo_stack.clear();
            self.columns.remove(col_idx);
            for row in &mut self.rows {
                if col_idx < row.len() {
                    row.remove(col_idx);
                }
            }
            let mut new_edits = HashMap::new();
            for (&(r, c), v) in &self.edits {
                if c < col_idx {
                    new_edits.insert((r, c), v.clone());
                } else if c > col_idx {
                    new_edits.insert((r, c - 1), v.clone());
                }
            }
            self.edits = new_edits;
            // Shift column marks
            let mark_keys: Vec<MarkKey> = self.marks.keys().cloned().collect();
            let mut new_marks = HashMap::new();
            for key in mark_keys {
                let color = self.marks.remove(&key).unwrap();
                match key {
                    MarkKey::Column(c) if c == col_idx => continue,
                    MarkKey::Cell(_, c) if c == col_idx => continue,
                    MarkKey::Column(c) if c > col_idx => {
                        new_marks.insert(MarkKey::Column(c - 1), color);
                    }
                    MarkKey::Cell(r, c) if c > col_idx => {
                        new_marks.insert(MarkKey::Cell(r, c - 1), color);
                    }
                    other => {
                        new_marks.insert(other, color);
                    }
                }
            }
            self.marks = new_marks;
        }
    }

    /// Move a row from `from` to `to`. Both must be valid indices.
    pub fn move_row(&mut self, from: usize, to: usize) {
        if from == to || from >= self.rows.len() || to >= self.rows.len() {
            return;
        }
        self.structural_changes = true;
        self.undo_stack.push(UndoAction::MoveRow { from, to });
        self.redo_stack.clear();
        let row = self.rows.remove(from);
        self.rows.insert(to, row);
        if let Some(meta) = self.db_meta.as_mut()
            && from < meta.row_tags.len()
        {
            let tag = meta.row_tags.remove(from);
            meta.row_tags.insert(to.min(meta.row_tags.len()), tag);
        }
        // Remap edits
        let mut new_edits = HashMap::new();
        for (&(r, c), v) in &self.edits {
            let new_r = if r == from {
                to
            } else if from < to {
                // Row moved down: rows in (from, to] shift up by 1
                if r > from && r <= to { r - 1 } else { r }
            } else {
                // Row moved up: rows in [to, from) shift down by 1
                if r >= to && r < from { r + 1 } else { r }
            };
            new_edits.insert((new_r, c), v.clone());
        }
        self.edits = new_edits;
    }

    /// Move a column from `from` to `to`. Both must be valid indices.
    pub fn move_column(&mut self, from: usize, to: usize) {
        if from == to || from >= self.columns.len() || to >= self.columns.len() {
            return;
        }
        self.structural_changes = true;
        self.undo_stack.push(UndoAction::MoveColumn { from, to });
        self.redo_stack.clear();
        let col_info = self.columns.remove(from);
        self.columns.insert(to, col_info);
        for row in &mut self.rows {
            if from < row.len() {
                let val = row.remove(from);
                let ins = to.min(row.len());
                row.insert(ins, val);
            }
        }
        // Remap edits
        let mut new_edits = HashMap::new();
        for (&(r, c), v) in &self.edits {
            let new_c = if c == from {
                to
            } else if from < to {
                if c > from && c <= to { c - 1 } else { c }
            } else {
                if c >= to && c < from { c + 1 } else { c }
            };
            new_edits.insert((r, new_c), v.clone());
        }
        self.edits = new_edits;
    }

    /// Reorder all columns according to a permutation.
    /// `order[new_pos] = old_pos` - i.e. the column that was at `old_pos` moves to `new_pos`.
    /// The `order` slice must be a valid permutation of `0..col_count`.
    pub fn reorder_columns(&mut self, order: &[usize]) {
        let n = self.columns.len();
        if order.len() != n {
            return;
        }
        self.structural_changes = true;
        self.undo_stack.push(UndoAction::ReorderColumns {
            order: order.to_vec(),
        });
        self.redo_stack.clear();

        self.apply_order(order);
    }

    /// Apply a column permutation without touching the undo/redo stacks.
    /// `order[new_pos] = old_pos`.
    fn apply_order(&mut self, order: &[usize]) {
        let n = self.columns.len();
        if order.len() != n {
            return;
        }

        // Reorder column metadata
        let old_cols = self.columns.clone();
        for (new_pos, &old_pos) in order.iter().enumerate() {
            self.columns[new_pos] = old_cols[old_pos].clone();
        }

        // Reorder each row's cell data
        for row in &mut self.rows {
            let old_row = row.clone();
            for (new_pos, &old_pos) in order.iter().enumerate() {
                row[new_pos] = old_row[old_pos].clone();
            }
        }

        // Remap edits: build reverse mapping (old_pos -> new_pos)
        let mut old_to_new = vec![0usize; n];
        for (new_pos, &old_pos) in order.iter().enumerate() {
            old_to_new[old_pos] = new_pos;
        }
        let mut new_edits = HashMap::new();
        for (&(r, c), v) in &self.edits {
            if c < n {
                new_edits.insert((r, old_to_new[c]), v.clone());
            }
        }
        self.edits = new_edits;
    }

    /// Compute the inverse of a column permutation (`inv[order[i]] = i`).
    fn invert_order(order: &[usize]) -> Vec<usize> {
        let mut inv = vec![0usize; order.len()];
        for (new_pos, &old_pos) in order.iter().enumerate() {
            if old_pos < inv.len() {
                inv[old_pos] = new_pos;
            }
        }
        inv
    }

    /// Sort all rows by the values in the given column, ascending or descending.
    /// Edits are applied first so sorting uses the current visible values.
    pub fn sort_rows_by_column(&mut self, col_idx: usize, ascending: bool) {
        if col_idx >= self.columns.len() || self.rows.is_empty() {
            return;
        }
        // Merge pending edits into rows first so we sort on the actual visible values
        self.apply_edits();
        self.structural_changes = true;

        self.rows.sort_by(|a, b| {
            let va = a.get(col_idx).unwrap_or(&CellValue::Null);
            let vb = b.get(col_idx).unwrap_or(&CellValue::Null);
            let cmp = cmp_cell_values(va, vb);
            if ascending { cmp } else { cmp.reverse() }
        });
    }

    /// Sort rows by several columns at once. `keys` lists `(col_idx, ascending)`
    /// in priority order: the first key is the primary sort, later keys break
    /// ties. Out-of-range columns and an empty key list are ignored. A single
    /// stable sort keeps the relative order of rows that are equal on every key.
    pub fn sort_rows_by_columns(&mut self, keys: &[(usize, bool)]) {
        let valid: Vec<(usize, bool)> = keys
            .iter()
            .copied()
            .filter(|&(c, _)| c < self.columns.len())
            .collect();
        if valid.is_empty() || self.rows.is_empty() {
            return;
        }
        // Merge pending edits so we sort on the actual visible values.
        self.apply_edits();
        self.structural_changes = true;

        self.rows.sort_by(|a, b| {
            for &(col_idx, ascending) in &valid {
                let va = a.get(col_idx).unwrap_or(&CellValue::Null);
                let vb = b.get(col_idx).unwrap_or(&CellValue::Null);
                let cmp = cmp_cell_values(va, vb);
                let cmp = if ascending { cmp } else { cmp.reverse() };
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
            }
            std::cmp::Ordering::Equal
        });
    }

    /// Apply all edits to the underlying data (merges edits into rows).
    /// Call this before saving to produce a clean DataTable.
    pub fn apply_edits(&mut self) {
        for (&(r, c), v) in &self.edits {
            if r < self.rows.len() && c < self.columns.len() {
                self.rows[r][c] = v.clone();
            }
        }
        self.edits.clear();
    }

    /// Return a fresh `DataTable` containing the same columns but only the
    /// rows whose indices appear in `row_indices`, in the order given.
    ///
    /// Used by **Save As** when the user has an active row filter (text
    /// search and/or column filters): the on-disk file should reflect what
    /// is visible, not the full unfiltered table. Edits are applied as part
    /// of the clone so the writer sees committed values. The returned table
    /// has `db_meta: None` (rowids no longer line up) and inherits no marks /
    /// undo state (irrelevant to the writer).
    pub fn clone_with_rows(&self, row_indices: &[usize]) -> Self {
        let mut rows: Vec<Vec<CellValue>> = Vec::with_capacity(row_indices.len());
        for &src in row_indices {
            if src >= self.rows.len() {
                continue;
            }
            let mut row = self.rows[src].clone();
            // Apply pending edits inline (avoid mutating self).
            for (c, cell) in row.iter_mut().enumerate().take(self.columns.len()) {
                if let Some(v) = self.edits.get(&(src, c)) {
                    *cell = v.clone();
                }
            }
            rows.push(row);
        }
        Self {
            columns: self.columns.clone(),
            rows,
            edits: HashMap::new(),
            source_path: self.source_path.clone(),
            format_name: self.format_name.clone(),
            structural_changes: true,
            total_rows: None,
            row_offset: 0,
            marks: HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        }
    }

    /// Treat current header names as a real first data row. Column names are
    /// reset to defaults (`column_1`..`column_N`) and types are widened to
    /// Utf8 since the header strings may not parse as the original types.
    pub fn promote_headers_to_row(&mut self) {
        self.apply_edits();
        let new_row: Vec<CellValue> = self
            .columns
            .iter()
            .map(|c| CellValue::String(c.name.clone()))
            .collect();
        for (i, col) in self.columns.iter_mut().enumerate() {
            col.name = format!("column_{}", i + 1);
            col.data_type = "Utf8".to_string();
        }
        self.rows.insert(0, new_row);
        if let Some(meta) = self.db_meta.as_mut() {
            meta.row_tags.insert(0, None);
        }
        // Shift row keys (edits + marks) by +1
        let mut new_edits = HashMap::new();
        for (&(r, c), v) in &self.edits {
            new_edits.insert((r + 1, c), v.clone());
        }
        self.edits = new_edits;
        let mark_keys: Vec<MarkKey> = self.marks.keys().cloned().collect();
        let mut new_marks = HashMap::new();
        for key in mark_keys {
            let color = self.marks.remove(&key).unwrap();
            let new_key = match key {
                MarkKey::Row(r) => MarkKey::Row(r + 1),
                MarkKey::Cell(r, c) => MarkKey::Cell(r + 1, c),
                other => other,
            };
            new_marks.insert(new_key, color);
        }
        self.marks = new_marks;
        self.structural_changes = true;
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Treat the first data row as column header names. The row is consumed
    /// from the table and column types are reset to Utf8.
    pub fn promote_first_row_to_headers(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.apply_edits();
        let first = self.rows.remove(0);
        for (i, col) in self.columns.iter_mut().enumerate() {
            let name = first.get(i).map(|v| v.to_string()).unwrap_or_default();
            col.name = if name.is_empty() {
                format!("column_{}", i + 1)
            } else {
                name
            };
            col.data_type = "Utf8".to_string();
        }
        if let Some(meta) = self.db_meta.as_mut()
            && !meta.row_tags.is_empty()
        {
            meta.row_tags.remove(0);
        }
        // Shift row keys (edits + marks) by -1, drop anything at row 0.
        let mut new_edits = HashMap::new();
        for (&(r, c), v) in &self.edits {
            if r > 0 {
                new_edits.insert((r - 1, c), v.clone());
            }
        }
        self.edits = new_edits;
        let mark_keys: Vec<MarkKey> = self.marks.keys().cloned().collect();
        let mut new_marks = HashMap::new();
        for key in mark_keys {
            let color = self.marks.remove(&key).unwrap();
            let new_key: Option<MarkKey> = match key {
                MarkKey::Row(0) => None,
                MarkKey::Row(r) => Some(MarkKey::Row(r - 1)),
                MarkKey::Cell(0, _) => None,
                MarkKey::Cell(r, c) => Some(MarkKey::Cell(r - 1, c)),
                other => Some(other),
            };
            if let Some(k) = new_key {
                new_marks.insert(k, color);
            }
        }
        self.marks = new_marks;
        self.structural_changes = true;
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Whether the table has been modified in any way since loading/saving.
    pub fn is_modified(&self) -> bool {
        !self.edits.is_empty() || self.structural_changes
    }

    /// Check if all values in a column can be converted to the target data type.
    /// Returns true if the conversion is safe (all non-null values are compatible).
    pub fn can_convert_column(&self, col_idx: usize, target_type: &str) -> bool {
        if col_idx >= self.columns.len() {
            return false;
        }
        for row_idx in 0..self.rows.len() {
            let val = self.get(row_idx, col_idx).unwrap_or(&CellValue::Null);
            if !can_convert_value(val, target_type) {
                return false;
            }
        }
        true
    }

    /// Convert all values in a column to a new data type.
    /// Returns true if conversion succeeded, false if validation failed.
    /// Pushes an undo action and converts both rows and pending edits.
    pub fn convert_column(&mut self, col_idx: usize, target_type: &str) -> bool {
        if col_idx >= self.columns.len() {
            return false;
        }
        let old_type = &self.columns[col_idx].data_type;
        if old_type == target_type {
            return true;
        }
        if !self.can_convert_column(col_idx, target_type) {
            return false;
        }
        // Save old values for undo
        let old_values: Vec<CellValue> = (0..self.rows.len())
            .map(|r| self.get(r, col_idx).cloned().unwrap_or(CellValue::Null))
            .collect();

        // Convert row values
        for row in &mut self.rows {
            if col_idx < row.len() {
                row[col_idx] = convert_value(&row[col_idx], target_type);
            }
        }
        // Convert pending edits for this column
        let edit_keys: Vec<(usize, usize)> = self
            .edits
            .keys()
            .filter(|(_, c)| *c == col_idx)
            .copied()
            .collect();
        for key in edit_keys {
            if let Some(val) = self.edits.get(&key) {
                let converted = convert_value(val, target_type);
                self.edits.insert(key, converted);
            }
        }

        let new_values: Vec<CellValue> = (0..self.rows.len())
            .map(|r| self.get(r, col_idx).cloned().unwrap_or(CellValue::Null))
            .collect();

        let old_type_str = self.columns[col_idx].data_type.clone();
        self.columns[col_idx].data_type = target_type.to_string();
        self.structural_changes = true;
        self.undo_stack.push(UndoAction::ConvertColumn {
            col_idx,
            old_type: old_type_str,
            new_type: target_type.to_string(),
            old_values,
            new_values,
        });
        self.redo_stack.clear();
        true
    }

    /// Evict the first `count` rows from the table, incrementing row_offset.
    /// Remaps edits: subtracts `count` from row indices, discards edits in evicted range.
    pub fn evict_front_rows(&mut self, count: usize) {
        let count = count.min(self.rows.len());
        if count == 0 {
            return;
        }
        self.rows.drain(..count);
        self.row_offset += count;
        let mut new_edits = HashMap::new();
        for (&(r, c), v) in &self.edits {
            if r >= count {
                new_edits.insert((r - count, c), v.clone());
            }
            // Edits in evicted range (r < count) are discarded
        }
        self.edits = new_edits;
    }

    /// Reset the modification tracking (call after saving).
    pub fn clear_modified(&mut self) {
        self.structural_changes = false;
        // edits are already cleared by apply_edits
    }

    /// Rename column `index` to `new`, recording an undoable action. No-op if
    /// the index is out of range or the name is unchanged. Marks the table as
    /// structurally changed (a schema change). Bulk callers wrap several of
    /// these with [`coalesce_undo_since`](Self::coalesce_undo_since).
    pub fn rename_column(&mut self, index: usize, new: String) {
        if index >= self.columns.len() || self.columns[index].name == new {
            return;
        }
        let old = self.columns[index].name.clone();
        self.undo_stack.push(UndoAction::RenameColumn {
            index,
            old,
            new: new.clone(),
        });
        self.redo_stack.clear();
        self.columns[index].name = new;
        self.structural_changes = true;
    }
}
