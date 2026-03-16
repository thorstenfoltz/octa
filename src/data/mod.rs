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
}

impl DataTable {
    pub fn empty() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            edits: HashMap::new(),
            source_path: None,
            format_name: None,
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

    /// Check if the table has any unsaved edits.
    pub fn has_edits(&self) -> bool {
        !self.edits.is_empty()
    }

    /// Discard all edits.
    pub fn discard_edits(&mut self) {
        self.edits.clear();
    }

    /// Insert a new empty row at the given index.
    /// If index >= row_count, appends at the end.
    pub fn insert_row(&mut self, index: usize) {
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

    /// Add a new column at the end with a given name and data type.
    pub fn add_column(&mut self, name: String, data_type: String) {
        self.columns.push(ColumnInfo { name, data_type });
        // Add a Null cell to every existing row
        for row in &mut self.rows {
            row.push(CellValue::Null);
        }
    }

    /// Delete a column by index.
    pub fn delete_column(&mut self, col_idx: usize) {
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
}
