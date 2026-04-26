pub mod json_util;
pub mod search;

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

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
    /// Rendered Jupyter Notebook view.
    Notebook,
    /// Collapsible JSON tree view (like Firefox JSON viewer).
    JsonTree,
}

/// Search/filter mode for the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchMode {
    /// Plain case-insensitive substring match.
    Plain,
    /// Wildcard: `*` = any chars, `?` = single char. Escape with `\*` and `\?`.
    Wildcard,
    /// Full regular expression (regex crate syntax).
    Regex,
}

impl SearchMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Plain => "Plain",
            Self::Wildcard => "Wildcard",
            Self::Regex => "Regex",
        }
    }
}

impl Default for SearchMode {
    fn default() -> Self {
        Self::Plain
    }
}

/// How to display binary (`Vec<u8>`) cell values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BinaryDisplayMode {
    /// Raw binary digits grouped per byte (e.g., `01000001 01000010`).
    #[default]
    Binary,
    /// Hexadecimal (e.g., `41 42`).
    Hex,
    /// Decode as UTF-8 text; fall back to hex for invalid sequences.
    Text,
}

impl BinaryDisplayMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Binary => "Binary",
            Self::Hex => "Hex",
            Self::Text => "Text (UTF-8)",
        }
    }
}

/// Convert a wildcard pattern to a regex string.
/// `*` matches any sequence of characters, `?` matches a single character.
/// Use `\*` for a literal `*` and `\?` for a literal `?`.
pub fn wildcard_to_regex(pattern: &str) -> String {
    let mut regex = String::from("(?i)");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() && (chars[i + 1] == '*' || chars[i + 1] == '?') {
            // Escaped wildcard → literal
            regex.push_str(&regex_syntax::escape(&chars[i + 1].to_string()));
            i += 2;
        } else if chars[i] == '*' {
            regex.push_str(".*");
            i += 1;
        } else if chars[i] == '?' {
            regex.push('.');
            i += 1;
        } else {
            regex.push_str(&regex_syntax::escape(&chars[i].to_string()));
            i += 1;
        }
    }
    regex
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
            CellValue::Binary(b) => {
                // Default Display uses hex; for mode-aware display use display_binary()
                for (i, byte) in b.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{:02x}", byte)?;
                }
                Ok(())
            }
            CellValue::Nested(s) => write!(f, "{}", s),
        }
    }
}

impl CellValue {
    /// Format this value for display, using `mode` when the value is Binary.
    /// Non-binary values use their normal `Display` representation.
    pub fn display_with_binary_mode(&self, mode: BinaryDisplayMode) -> String {
        match self {
            CellValue::Binary(b) => match mode {
                BinaryDisplayMode::Binary => b
                    .iter()
                    .map(|byte| format!("{:08b}", byte))
                    .collect::<Vec<_>>()
                    .join(" "),
                BinaryDisplayMode::Hex => b
                    .iter()
                    .map(|byte| format!("{:02x}", byte))
                    .collect::<Vec<_>>()
                    .join(" "),
                BinaryDisplayMode::Text => {
                    if let Ok(s) = std::str::from_utf8(b) {
                        if !s.is_empty()
                            && s.chars()
                                .all(|c| !c.is_control() || c == '\n' || c == '\r' || c == '\t')
                        {
                            return s.to_string();
                        }
                    }
                    // Fall back to hex for non-printable / invalid UTF-8
                    b.iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<Vec<_>>()
                        .join(" ")
                }
            },
            other => other.to_string(),
        }
    }

    /// Parse a display string back into a Binary CellValue, respecting the display mode
    /// that was used to show it.
    pub fn parse_binary(text: &str, mode: BinaryDisplayMode) -> CellValue {
        if text.is_empty() {
            return CellValue::Null;
        }
        match mode {
            BinaryDisplayMode::Binary => {
                let bytes: Result<Vec<u8>, _> = text
                    .split_whitespace()
                    .map(|chunk| u8::from_str_radix(chunk, 2))
                    .collect();
                match bytes {
                    Ok(b) => CellValue::Binary(b),
                    Err(_) => CellValue::Binary(text.as_bytes().to_vec()),
                }
            }
            BinaryDisplayMode::Hex => {
                let bytes: Result<Vec<u8>, _> = text
                    .split_whitespace()
                    .map(|chunk| u8::from_str_radix(chunk, 16))
                    .collect();
                match bytes {
                    Ok(b) => CellValue::Binary(b),
                    Err(_) => CellValue::Binary(text.as_bytes().to_vec()),
                }
            }
            BinaryDisplayMode::Text => CellValue::Binary(text.as_bytes().to_vec()),
        }
    }

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
            CellValue::Date(_) => CellValue::Date(text.to_string()),
            CellValue::DateTime(_) => CellValue::DateTime(text.to_string()),
            CellValue::Binary(_) => Self::parse_binary(text, BinaryDisplayMode::Hex),
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

/// Evaluate a simple Excel-like formula.
/// Supports cell references (e.g. A1, B2), numeric literals, and operators +, -, *, /.
/// Column letters: A=0, B=1, ..., Z=25, AA=26, etc. Row numbers are 1-based.
/// Returns the computed f64 result, or None if the formula is invalid.
pub fn evaluate_formula(formula: &str, table: &DataTable) -> Option<f64> {
    let expr = formula.trim();
    if expr.is_empty() {
        return None;
    }
    let tokens = tokenize_formula(expr)?;
    eval_expression(&tokens, 0, table).map(|(val, _)| val)
}

/// Resolve a cell reference like "A1", "BC42" to (row, col) zero-indexed.
fn parse_cell_ref(s: &str) -> Option<(usize, usize)> {
    let s = s.trim();
    let col_end = s.bytes().position(|b| b.is_ascii_digit())?;
    if col_end == 0 {
        return None;
    }
    let col_str = &s[..col_end];
    let row_str = &s[col_end..];
    if row_str.is_empty() {
        return None;
    }
    // Column: A=0, B=1, ..., Z=25, AA=26
    let mut col: usize = 0;
    for ch in col_str.chars() {
        if !ch.is_ascii_alphabetic() {
            return None;
        }
        col = col * 26 + (ch.to_ascii_uppercase() as usize - b'A' as usize + 1);
    }
    col -= 1; // zero-indexed
    let row: usize = row_str.parse::<usize>().ok()?;
    if row == 0 {
        return None;
    }
    Some((row - 1, col))
}

/// Get numeric value from a cell, parsing strings as f64 if needed.
fn cell_as_f64(table: &DataTable, row: usize, col: usize) -> Option<f64> {
    match table.get(row, col)? {
        CellValue::Int(n) => Some(*n as f64),
        CellValue::Float(f) => Some(*f),
        CellValue::String(s) => s.trim().parse::<f64>().ok(),
        CellValue::Null => Some(0.0),
        _ => None,
    }
}

#[derive(Debug, Clone)]
enum FormulaToken {
    Number(f64),
    CellRef(usize, usize), // (row, col)
    Op(char),              // +, -, *, /
    LParen,
    RParen,
}

fn tokenize_formula(expr: &str) -> Option<Vec<FormulaToken>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            ' ' => {
                i += 1;
            }
            '+' | '-' | '*' | '/' => {
                // Unary minus: treat as part of number if at start or after operator/lparen
                if chars[i] == '-' {
                    let is_unary = tokens.is_empty()
                        || matches!(
                            tokens.last(),
                            Some(FormulaToken::Op(_)) | Some(FormulaToken::LParen)
                        );
                    if is_unary {
                        // Collect the number after the minus
                        i += 1;
                        let start = i;
                        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                            i += 1;
                        }
                        if i == start {
                            return None;
                        }
                        let num_str: String = chars[start..i].iter().collect();
                        let val: f64 = num_str.parse().ok()?;
                        tokens.push(FormulaToken::Number(-val));
                        continue;
                    }
                }
                tokens.push(FormulaToken::Op(chars[i]));
                i += 1;
            }
            '(' => {
                tokens.push(FormulaToken::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(FormulaToken::RParen);
                i += 1;
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                let val: f64 = num_str.parse().ok()?;
                tokens.push(FormulaToken::Number(val));
            }
            c if c.is_ascii_alphabetic() => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric()) {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let (row, col) = parse_cell_ref(&word)?;
                tokens.push(FormulaToken::CellRef(row, col));
            }
            _ => return None,
        }
    }
    Some(tokens)
}

/// Recursive descent parser: expression = term (('+' | '-') term)*
fn eval_expression(tokens: &[FormulaToken], pos: usize, table: &DataTable) -> Option<(f64, usize)> {
    let (mut left, mut p) = eval_term(tokens, pos, table)?;
    while p < tokens.len() {
        match &tokens[p] {
            FormulaToken::Op('+') => {
                let (right, np) = eval_term(tokens, p + 1, table)?;
                left += right;
                p = np;
            }
            FormulaToken::Op('-') => {
                let (right, np) = eval_term(tokens, p + 1, table)?;
                left -= right;
                p = np;
            }
            _ => break,
        }
    }
    Some((left, p))
}

/// term = factor (('*' | '/') factor)*
fn eval_term(tokens: &[FormulaToken], pos: usize, table: &DataTable) -> Option<(f64, usize)> {
    let (mut left, mut p) = eval_factor(tokens, pos, table)?;
    while p < tokens.len() {
        match &tokens[p] {
            FormulaToken::Op('*') => {
                let (right, np) = eval_factor(tokens, p + 1, table)?;
                left *= right;
                p = np;
            }
            FormulaToken::Op('/') => {
                let (right, np) = eval_factor(tokens, p + 1, table)?;
                if right == 0.0 {
                    return None; // division by zero
                }
                left /= right;
                p = np;
            }
            _ => break,
        }
    }
    Some((left, p))
}

/// factor = Number | CellRef | '(' expression ')'
fn eval_factor(tokens: &[FormulaToken], pos: usize, table: &DataTable) -> Option<(f64, usize)> {
    if pos >= tokens.len() {
        return None;
    }
    match &tokens[pos] {
        FormulaToken::Number(n) => Some((*n, pos + 1)),
        FormulaToken::CellRef(row, col) => {
            let val = cell_as_f64(table, *row, *col).unwrap_or(0.0);
            Some((val, pos + 1))
        }
        FormulaToken::LParen => {
            let (val, p) = eval_expression(tokens, pos + 1, table)?;
            if p < tokens.len() && matches!(tokens[p], FormulaToken::RParen) {
                Some((val, p + 1))
            } else {
                None
            }
        }
        _ => None,
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
}

/// Key identifying what is marked (cell, row, or column).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MarkKey {
    Cell(usize, usize),
    Row(usize),
    Column(usize),
}

/// Metadata associating rows of a `DataTable` with rows in a source database
/// table. Set by the SQLite / DuckDB readers and consumed by their writers to
/// produce INSERT / UPDATE / DELETE statements rather than overwriting.
#[derive(Debug, Clone)]
pub struct DbRowMeta {
    /// Name of the source table.
    pub table_name: String,
    /// Per-row source identity, parallel to `DataTable.rows`.
    /// `None` = inserted by the user since load (becomes an INSERT on save).
    /// `Some(tag)` = original row from the DB (rowid for SQLite, sequential for DuckDB).
    pub row_tags: Vec<Option<i64>>,
    /// Snapshot of original row values keyed by tag, used to detect cell-level
    /// changes for UPDATE statements.
    pub original: HashMap<i64, Vec<CellValue>>,
    /// Original column names at load time. Save fails if columns no longer
    /// match — schema-altering edits aren't supported on DB-backed tables.
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
            if let Some(meta) = self.db_meta.as_mut() {
                if index < meta.row_tags.len() {
                    meta.row_tags.remove(index);
                }
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
        if let Some(meta) = self.db_meta.as_mut() {
            if from < meta.row_tags.len() {
                let tag = meta.row_tags.remove(from);
                meta.row_tags.insert(to.min(meta.row_tags.len()), tag);
            }
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
            if ascending { cmp } else { cmp.reverse() }
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
        if let Some(meta) = self.db_meta.as_mut() {
            if !meta.row_tags.is_empty() {
                meta.row_tags.remove(0);
            }
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
                        if let Some(meta) = self.db_meta.as_mut() {
                            if index < meta.row_tags.len() {
                                meta.row_tags.remove(index);
                            }
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
                            if col_idx < row.len() {
                                if let Some(val) = old_values.get(row_idx) {
                                    row[col_idx] = val.clone();
                                }
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
                        if let Some(meta) = self.db_meta.as_mut() {
                            if index < meta.row_tags.len() {
                                meta.row_tags.remove(index);
                            }
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
                            if col_idx < row.len() {
                                if let Some(val) = new_values.get(row_idx) {
                                    row[col_idx] = val.clone();
                                }
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

/// Convert a CellValue to a target data type.
/// Assumes `can_convert_value` has already validated the conversion.
pub fn convert_value(val: &CellValue, target_type: &str) -> CellValue {
    match val {
        CellValue::Null => CellValue::Null,
        CellValue::Bool(b) => match target_type {
            "Boolean" => val.clone(),
            "Int64" => CellValue::Int(if *b { 1 } else { 0 }),
            "Float64" => CellValue::Float(if *b { 1.0 } else { 0.0 }),
            "String" | "Utf8" => CellValue::String(b.to_string()),
            _ => val.clone(),
        },
        CellValue::Int(n) => match target_type {
            "Int64" => val.clone(),
            "Float64" => CellValue::Float(*n as f64),
            "Boolean" => CellValue::Bool(*n != 0),
            "String" | "Utf8" => CellValue::String(n.to_string()),
            _ => val.clone(),
        },
        CellValue::Float(f) => match target_type {
            "Float64" => val.clone(),
            "Int64" => CellValue::Int(*f as i64),
            "String" | "Utf8" => CellValue::String(f.to_string()),
            _ => val.clone(),
        },
        CellValue::String(s) => {
            if s.is_empty() {
                return CellValue::Null;
            }
            match target_type {
                "String" | "Utf8" => val.clone(),
                "Int64" => CellValue::Int(s.parse::<i64>().unwrap_or(0)),
                "Float64" => CellValue::Float(s.parse::<f64>().unwrap_or(0.0)),
                "Boolean" => {
                    let lower = s.to_lowercase();
                    CellValue::Bool(matches!(lower.as_str(), "true" | "1" | "yes"))
                }
                "Date32" => CellValue::Date(s.clone()),
                "Timestamp(Microsecond, None)" => CellValue::DateTime(s.clone()),
                _ => val.clone(),
            }
        }
        CellValue::Date(s) => match target_type {
            "Date32" => val.clone(),
            "String" | "Utf8" => CellValue::String(s.clone()),
            "Timestamp(Microsecond, None)" => CellValue::DateTime(format!("{s} 00:00:00")),
            _ => val.clone(),
        },
        CellValue::DateTime(s) => match target_type {
            "Timestamp(Microsecond, None)" => val.clone(),
            "String" | "Utf8" => CellValue::String(s.clone()),
            "Date32" => CellValue::Date(s.chars().take(10).collect()),
            _ => val.clone(),
        },
        CellValue::Binary(b) => match target_type {
            "String" | "Utf8" => CellValue::String(format!("{b:?}")),
            _ => val.clone(),
        },
        CellValue::Nested(s) => match target_type {
            "String" | "Utf8" => CellValue::String(s.clone()),
            _ => val.clone(),
        },
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
