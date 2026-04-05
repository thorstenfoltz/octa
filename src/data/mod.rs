use std::collections::HashMap;
use std::fmt;

/// How to display the loaded file content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// Structured tabular view (default).
    Table,
    /// Raw text view of the file content (like a text editor).
    Raw,
    /// Rendered PDF page view (like Adobe Reader).
    Pdf,
    /// Rendered Markdown view.
    Markdown,
}

/// Represents a single cell value in the data table.
/// Supports structured (typed columns) and semi-structured (mixed types) data.
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Date(String),
    DateTime(String),
    Binary(Vec<u8>),
    /// For semi-structured nested data (JSON objects, arrays, etc.)
    Nested(String),
}

impl fmt::Display for CellValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CellValue::Null => write!(f, ""),
            CellValue::Bool(b) => write!(f, "{}", b),
            CellValue::Int(i) => write!(f, "{}", i),
            CellValue::Float(v) => {
                if v.fract() == 0.0 && v.abs() < 1e15 {
                    write!(f, "{:.1}", v)
                } else {
                    write!(f, "{}", v)
                }
            }
            CellValue::String(s) => write!(f, "{}", s),
            CellValue::Date(s) => write!(f, "{}", s),
            CellValue::DateTime(s) => write!(f, "{}", s),
            CellValue::Binary(b) => write!(f, "<{} bytes>", b.len()),
            CellValue::Nested(s) => write!(f, "{}", s),
        }
    }
}

impl CellValue {
    /// Try to parse a display string back into a CellValue, keeping the same variant
    /// as the `hint` when possible.
    pub fn parse_like(hint: &CellValue, text: &str) -> CellValue {
        if text.is_empty() {
            return CellValue::Null;
        }
        match hint {
            CellValue::Bool(_) => match text.to_lowercase().as_str() {
                "true" | "1" | "yes" => CellValue::Bool(true),
                "false" | "0" | "no" => CellValue::Bool(false),
                _ => CellValue::String(text.to_string()),
            },
            CellValue::Int(_) => text
                .parse::<i64>()
                .map(CellValue::Int)
                .unwrap_or_else(|_| CellValue::String(text.to_string())),
            CellValue::Float(_) => text
                .parse::<f64>()
                .map(CellValue::Float)
                .unwrap_or_else(|_| CellValue::String(text.to_string())),
            _ => CellValue::String(text.to_string()),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            CellValue::Null => "null",
            CellValue::Bool(_) => "bool",
            CellValue::Int(_) => "int",
            CellValue::Float(_) => "float",
            CellValue::String(_) => "string",
            CellValue::Date(_) => "date",
            CellValue::DateTime(_) => "datetime",
            CellValue::Binary(_) => "binary",
            CellValue::Nested(_) => "nested",
        }
    }
}

/// Column metadata
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
}

/// Available highlight colors for marking cells, rows, and columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MarkColor {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl MarkColor {
    pub const ALL: &'static [MarkColor] = &[
        MarkColor::Red,
        MarkColor::Orange,
        MarkColor::Yellow,
        MarkColor::Green,
        MarkColor::Blue,
        MarkColor::Purple,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            MarkColor::Red => "Red",
            MarkColor::Orange => "Orange",
            MarkColor::Yellow => "Yellow",
            MarkColor::Green => "Green",
            MarkColor::Blue => "Blue",
            MarkColor::Purple => "Purple",
        }
    }
}

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
    SetMark {
        key: MarkKey,
        old_color: Option<MarkColor>,
        new_color: Option<MarkColor>,
    },
}

/// Key identifying what is marked (cell, row, or column).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MarkKey {
    Cell(usize, usize),
    Row(usize),
    Column(usize),
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
            self.undo_stack.push(UndoAction::DeleteRow {
                index,
                data: row_data,
            });
            self.redo_stack.clear();
            self.rows.remove(index);
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
        // Remap edits
        let mut new_edits = HashMap::new();
        for (&(r, c), v) in &self.edits {
            let new_r = if r == from {
                to
            } else if from < to {
                // Row moved down: rows in (from, to] shift up by 1
                if r > from && r <= to {
                    r - 1
                } else {
                    r
                }
            } else {
                // Row moved up: rows in [to, from) shift down by 1
                if r >= to && r < from {
                    r + 1
                } else {
                    r
                }
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
                if c > from && c <= to {
                    c - 1
                } else {
                    c
                }
            } else {
                if c >= to && c < from {
                    c + 1
                } else {
                    c
                }
            };
            new_edits.insert((r, new_c), v.clone());
        }
        self.edits = new_edits;
    }

    /// Reorder all columns according to a permutation.
    /// `order[new_pos] = old_pos` — i.e. the column that was at `old_pos` moves to `new_pos`.
    /// The `order` slice must be a valid permutation of `0..col_count`.
    pub fn reorder_columns(&mut self, order: &[usize]) {
        let n = self.columns.len();
        if order.len() != n {
            return;
        }
        self.structural_changes = true;

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
            if ascending {
                cmp
            } else {
                cmp.reverse()
            }
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

    /// Set a color mark on a cell, row, or column.
    pub fn set_mark(&mut self, key: MarkKey, color: MarkColor) {
        let old_color = self.marks.get(&key).copied();
        self.undo_stack.push(UndoAction::SetMark {
            key: key.clone(),
            old_color,
            new_color: Some(color),
        });
        self.redo_stack.clear();
        self.marks.insert(key, color);
    }

    /// Remove a color mark.
    pub fn clear_mark(&mut self, key: MarkKey) {
        let old_color = self.marks.get(&key).copied();
        if old_color.is_some() {
            self.undo_stack.push(UndoAction::SetMark {
                key: key.clone(),
                old_color,
                new_color: None,
            });
            self.redo_stack.clear();
            self.marks.remove(&key);
        }
    }

    /// Get the effective mark color for a cell (cell mark > row mark > column mark).
    pub fn get_mark_color(&self, row: usize, col: usize) -> Option<MarkColor> {
        if let Some(&c) = self.marks.get(&MarkKey::Cell(row, col)) {
            return Some(c);
        }
        if let Some(&c) = self.marks.get(&MarkKey::Row(row)) {
            return Some(c);
        }
        if let Some(&c) = self.marks.get(&MarkKey::Column(col)) {
            return Some(c);
        }
        None
    }

    /// Undo the last action. Returns true if something was undone.
    pub fn undo(&mut self) -> bool {
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
                UndoAction::DeleteRow { index, data } => {
                    self.rows.insert(index, data);
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
                UndoAction::SetMark { key, old_color, .. } => match old_color {
                    Some(c) => {
                        self.marks.insert(key, c);
                    }
                    None => {
                        self.marks.remove(&key);
                    }
                },
            }
            self.redo_stack.push(action);
            true
        } else {
            false
        }
    }

    /// Redo the last undone action. Returns true if something was redone.
    pub fn redo(&mut self) -> bool {
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
                UndoAction::SetMark { key, new_color, .. } => match new_color {
                    Some(c) => {
                        self.marks.insert(key, c);
                    }
                    None => {
                        self.marks.remove(&key);
                    }
                },
            }
            self.undo_stack.push(action);
            true
        } else {
            false
        }
    }
}

/// Check if a single CellValue can be converted to the target data type.
#[allow(dead_code)]
pub fn can_convert_value(val: &CellValue, target_type: &str) -> bool {
    match val {
        CellValue::Null => true, // Null converts to anything
        CellValue::Bool(_) => matches!(
            target_type,
            "String" | "Utf8" | "Boolean" | "Int64" | "Float64"
        ),
        CellValue::Int(_) => matches!(
            target_type,
            "String" | "Utf8" | "Int64" | "Float64" | "Boolean"
        ),
        CellValue::Float(f) => match target_type {
            "String" | "Utf8" | "Float64" => true,
            "Int64" => f.fract() == 0.0 && f.abs() < i64::MAX as f64,
            _ => false,
        },
        CellValue::String(s) => {
            if s.is_empty() {
                return true;
            }
            match target_type {
                "String" | "Utf8" => true,
                "Int64" => s.parse::<i64>().is_ok(),
                "Float64" => s.parse::<f64>().is_ok(),
                "Boolean" => matches!(
                    s.to_lowercase().as_str(),
                    "true" | "false" | "1" | "0" | "yes" | "no"
                ),
                "Date32" => chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok(),
                "Timestamp(Microsecond, None)" => {
                    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").is_ok()
                        || chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").is_ok()
                        || chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.3f").is_ok()
                }
                _ => false,
            }
        }
        CellValue::Date(_) => matches!(
            target_type,
            "String" | "Utf8" | "Date32" | "Timestamp(Microsecond, None)"
        ),
        CellValue::DateTime(_) => match target_type {
            "String" | "Utf8" | "Timestamp(Microsecond, None)" => true,
            "Date32" => true, // truncate time portion
            _ => false,
        },
        CellValue::Binary(_) => matches!(target_type, "String" | "Utf8"),
        CellValue::Nested(_) => matches!(target_type, "String" | "Utf8"),
    }
}

/// Compare two CellValues for sorting.
/// Ordering: Null < Bool < Int/Float (numeric) < String/Date/DateTime < Binary < Nested
pub fn cmp_cell_values(a: &CellValue, b: &CellValue) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    match (a, b) {
        (CellValue::Null, CellValue::Null) => Ordering::Equal,
        (CellValue::Null, _) => Ordering::Less,
        (_, CellValue::Null) => Ordering::Greater,

        (CellValue::Bool(a), CellValue::Bool(b)) => a.cmp(b),

        (CellValue::Int(a), CellValue::Int(b)) => a.cmp(b),
        (CellValue::Float(a), CellValue::Float(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (CellValue::Int(a), CellValue::Float(b)) => {
            (*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal)
        }
        (CellValue::Float(a), CellValue::Int(b)) => {
            a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal)
        }

        (CellValue::String(a), CellValue::String(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
        (CellValue::Date(a), CellValue::Date(b)) => a.cmp(b),
        (CellValue::DateTime(a), CellValue::DateTime(b)) => a.cmp(b),

        // Fallback: compare display strings
        _ => a
            .to_string()
            .to_lowercase()
            .cmp(&b.to_string().to_lowercase()),
    }
}
