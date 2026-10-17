//! Extract a regex match from each cell into a new column.

use regex::Regex;

use super::cell_text;
use crate::data::{CellValue, DataTable};

/// For each cell in `col`, extract the first capture group of `re` (or the
/// whole match when the pattern has no capture group). Cells that don't match
/// become `Null`. The caller compiles `re` so it can surface a friendly error
/// for a bad pattern.
pub fn extract_pattern(table: &DataTable, col: usize, re: &Regex) -> Vec<CellValue> {
    let n = table.row_count();
    (0..n)
        .map(|r| {
            let text = cell_text(table, r, col);
            match re.captures(&text) {
                Some(caps) => caps
                    .get(1)
                    .or_else(|| caps.get(0))
                    .map(|m| CellValue::String(m.as_str().to_string()))
                    .unwrap_or(CellValue::Null),
                None => CellValue::Null,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;
