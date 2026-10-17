//! Split one column into several.

use anyhow::Result;
use regex::Regex;

use super::cell_text;
use crate::data::{CellValue, DataTable};

/// How to split each cell's text.
#[derive(Debug, Clone)]
pub enum SplitSpec {
    /// Split on a literal delimiter string (e.g. `","`, `" - "`).
    Delimiter(String),
    /// Split on a regular expression.
    Regex(String),
    /// Slice into fixed-width chunks of `n` characters.
    FixedWidth(usize),
}

/// Split `col` into one or more new columns according to `spec`.
///
/// Returns `(name, values)` pairs, one per output column. The number of output
/// columns is the maximum part count across all rows; rows with fewer parts get
/// `Null` in the trailing columns. New column names are `"<source>_1"`,
/// `"<source>_2"`, ... All output cells are `String`.
pub fn split_column(
    table: &DataTable,
    col: usize,
    spec: &SplitSpec,
) -> Result<Vec<(String, Vec<CellValue>)>> {
    if col >= table.col_count() {
        anyhow::bail!("column index {col} out of range");
    }
    let re = match spec {
        SplitSpec::Regex(pattern) => Some(Regex::new(pattern)?),
        _ => None,
    };

    let n = table.row_count();
    let mut parts_per_row: Vec<Vec<String>> = Vec::with_capacity(n);
    let mut max_parts = 0usize;
    for r in 0..n {
        let text = cell_text(table, r, col);
        let parts: Vec<String> = match spec {
            SplitSpec::Delimiter(d) => {
                if d.is_empty() {
                    vec![text]
                } else {
                    text.split(d.as_str()).map(|s| s.to_string()).collect()
                }
            }
            SplitSpec::Regex(_) => re
                .as_ref()
                .expect("regex compiled above")
                .split(&text)
                .map(|s| s.to_string())
                .collect(),
            SplitSpec::FixedWidth(w) => split_fixed(&text, *w),
        };
        max_parts = max_parts.max(parts.len());
        parts_per_row.push(parts);
    }
    if max_parts == 0 {
        max_parts = 1;
    }

    let base = &table.columns[col].name;
    let mut out: Vec<(String, Vec<CellValue>)> = (0..max_parts)
        .map(|i| (format!("{base}_{}", i + 1), Vec::with_capacity(n)))
        .collect();
    for parts in &parts_per_row {
        for (i, slot) in out.iter_mut().enumerate() {
            let v = parts
                .get(i)
                .map(|s| CellValue::String(s.clone()))
                .unwrap_or(CellValue::Null);
            slot.1.push(v);
        }
    }
    Ok(out)
}

/// Chunk `text` into fixed-width slices of `width` characters (UTF-8 safe).
fn split_fixed(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    chars.chunks(width).map(|c| c.iter().collect()).collect()
}

#[cfg(test)]
#[path = "split_tests.rs"]
mod tests;
