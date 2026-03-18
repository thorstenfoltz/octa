use std::collections::HashMap;
use std::fmt;

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

    /// Set a cell value (tracked as an edit).
    pub fn set(&mut self, row: usize, col: usize, value: CellValue) {
        // Ensure the row exists
        if row < self.rows.len() && col < self.columns.len() {
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
    }

    /// Delete a row by index.
    pub fn delete_row(&mut self, index: usize) {
        self.structural_changes = true;
        if index < self.rows.len() {
            self.rows.remove(index);
            // Clean up edits referencing this row or higher
            let mut new_edits = HashMap::new();
            for (&(r, c), v) in &self.edits {
                if r < index {
                    new_edits.insert((r, c), v.clone());
                } else if r > index {
                    new_edits.insert((r - 1, c), v.clone());
                }
                // r == index: dropped
            }
            self.edits = new_edits;
        }
    }

    /// Insert a new column at the given index with a given name and data type.
    /// If index >= col_count, appends at the end.
    pub fn insert_column(&mut self, index: usize, name: String, data_type: String) {
        self.structural_changes = true;
        let idx = index.min(self.columns.len());
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
    }

    /// Delete a column by index.
    pub fn delete_column(&mut self, col_idx: usize) {
        self.structural_changes = true;
        if col_idx < self.columns.len() {
            self.columns.remove(col_idx);
            for row in &mut self.rows {
                if col_idx < row.len() {
                    row.remove(col_idx);
                }
            }
            // Clean up edits: remove edits for the deleted column, shift higher columns down
            let mut new_edits = HashMap::new();
            for (&(r, c), v) in &self.edits {
                if c < col_idx {
                    new_edits.insert((r, c), v.clone());
                } else if c > col_idx {
                    new_edits.insert((r, c - 1), v.clone());
                }
                // c == col_idx: dropped
            }
            self.edits = new_edits;
        }
    }

    /// Move a row from `from` to `to`. Both must be valid indices.
    pub fn move_row(&mut self, from: usize, to: usize) {
        if from == to || from >= self.rows.len() || to >= self.rows.len() {
            return;
        }
        self.structural_changes = true;
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

    /// Reset the modification tracking (call after saving).
    pub fn clear_modified(&mut self) {
        self.structural_changes = false;
        // edits are already cleared by apply_edits
    }
}

/// Compare two CellValues for sorting.
/// Ordering: Null < Bool < Int/Float (numeric) < String/Date/DateTime < Binary < Nested
fn cmp_cell_values(a: &CellValue, b: &CellValue) -> std::cmp::Ordering {
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
