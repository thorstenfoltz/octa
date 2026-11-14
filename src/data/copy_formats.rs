//! **Copy as**: the current selection as JSON or as SQL `INSERT` statements.
//! Pure: the app resolves the selection into a [`Block`] with the rule Copy
//! as Markdown table uses, then puts the text on the clipboard. Markdown and
//! the IN list keep their own builders (`app/clipboard.rs`,
//! `data::in_list`). No HTML: egui can put only text on the clipboard.

use std::collections::HashSet;

use super::times::{DurationStyle, format_duration, format_time};
use super::{CellValue, DataTable};

/// The Copy as submenu's entries, shared by the Edit menu, both cell menus
/// and the shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyAs {
    Markdown,
    InList,
    Json,
    Insert,
}

impl CopyAs {
    pub const ALL: [CopyAs; 4] = [
        CopyAs::Markdown,
        CopyAs::InList,
        CopyAs::Json,
        CopyAs::Insert,
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            CopyAs::Markdown => "copy_as.markdown",
            CopyAs::InList => "copy_as.in_list",
            CopyAs::Json => "copy_as.json",
            CopyAs::Insert => "copy_as.insert",
        }
    }

    pub fn hint_key(self) -> &'static str {
        match self {
            CopyAs::Markdown => "copy_as.markdown_hint",
            CopyAs::InList => "copy_as.in_list_hint",
            CopyAs::Json => "copy_as.json_hint",
            CopyAs::Insert => "copy_as.insert_hint",
        }
    }
}

/// These columns of these rows. `only`, when set, is a disjoint cell
/// selection: a cell outside it counts as empty.
pub struct Block<'a> {
    pub table: &'a DataTable,
    pub cols: Vec<usize>,
    pub rows: Vec<usize>,
    pub only: Option<&'a HashSet<(usize, usize)>>,
}

static NULL: CellValue = CellValue::Null;

impl Block<'_> {
    fn cell(&self, r: usize, c: usize) -> &CellValue {
        if self.only.is_some_and(|s| !s.contains(&(r, c))) {
            return &NULL;
        }
        self.table.get(r, c).unwrap_or(&NULL)
    }

    /// The column names, a repeat numbered `name_2` so every key is unique.
    /// Only the output is renamed, never the table.
    fn names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::with_capacity(self.cols.len());
        for &c in &self.cols {
            let base = self.table.columns.get(c).map_or("", |ci| ci.name.as_str());
            let name = super::unique_name(base, |n| out.iter().any(|o| o == n));
            out.push(name);
        }
        out
    }
}

fn json_value(v: &CellValue) -> serde_json::Value {
    use serde_json::Value;
    match v {
        CellValue::Null => Value::Null,
        CellValue::Bool(b) => Value::Bool(*b),
        CellValue::Int(i) => Value::from(*i),
        CellValue::Float(f) => serde_json::Number::from_f64(*f).map_or(Value::Null, Value::Number),
        CellValue::Time(ns) => Value::String(format_time(*ns)),
        CellValue::Duration(ns) => Value::String(format_duration(*ns, DurationStyle::Iso)),
        // Already JSON (a struct or list column): kept as structure, not as
        // a string holding JSON.
        CellValue::Nested(s) => {
            serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone()))
        }
        other => Value::String(other.to_string()),
    }
}

/// An array of objects keyed by column name, two-space indent.
pub fn to_json(b: &Block) -> String {
    let names = b.names();
    let rows: Vec<serde_json::Value> = b
        .rows
        .iter()
        .map(|&r| {
            let mut obj = serde_json::Map::new();
            for (name, &c) in names.iter().zip(&b.cols) {
                obj.insert(name.clone(), json_value(b.cell(r, c)));
            }
            serde_json::Value::Object(obj)
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::Value::Array(rows)).unwrap_or_default()
}

fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn sql_value(v: &CellValue, data_type: &str) -> String {
    match v {
        // A decimal column's digits go in bare, every digit.
        CellValue::String(s)
            if crate::data::decimal::decimal_parts(data_type).is_some()
                && crate::data::decimal::Exact::parse(s).is_some() =>
        {
            s.clone()
        }
        CellValue::Null => "NULL".into(),
        CellValue::Int(i) => i.to_string(),
        CellValue::Float(f) if f.is_finite() => f.to_string(),
        CellValue::Float(_) => "NULL".into(),
        CellValue::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        CellValue::Duration(ns) => quote(&format_duration(*ns, DurationStyle::Iso)),
        other => quote(&other.to_string()),
    }
}

/// One `INSERT` per row, ANSI quoting (`"` doubled in names, `'` in text).
pub fn to_insert(b: &Block, table: &str) -> String {
    let cols = b
        .names()
        .iter()
        .map(|n| ident(n))
        .collect::<Vec<_>>()
        .join(", ");
    let target = ident(table);
    b.rows
        .iter()
        .map(|&r| {
            let vals = b
                .cols
                .iter()
                .map(|&c| sql_value(b.cell(r, c), &b.table.columns[c].data_type))
                .collect::<Vec<_>>()
                .join(", ");
            format!("INSERT INTO {target} ({cols}) VALUES ({vals});\n")
        })
        .collect()
}

/// The table an INSERT names: the database table, else the file stem made
/// into an identifier (letters, digits and `_`; a leading digit gets `t_`),
/// else `data`. No dialog asks.
pub fn insert_table_name(db_table: Option<&str>, path: Option<&str>) -> String {
    if let Some(t) = db_table.filter(|t| !t.is_empty()) {
        return t.to_string();
    }
    let stem = path
        .and_then(|p| std::path::Path::new(p).file_stem())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut id: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    while id.contains("__") {
        id = id.replace("__", "_");
    }
    let id = id.trim_matches('_').to_string();
    if id.is_empty() {
        return "data".into();
    }
    if id.starts_with(|c: char| c.is_ascii_digit()) {
        return format!("t_{id}");
    }
    id
}

#[cfg(test)]
#[path = "copy_formats_tests.rs"]
mod tests;
