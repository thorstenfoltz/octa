//! Find/replace within a single column's cells.

use crate::data::CellValue;
use crate::data::DataTable;
use crate::data::search::RowMatcher;

/// Return `col`'s values with every match of `matcher` replaced by
/// `replacement`. `Null` cells are left untouched; all other cells become
/// `String` after replacement. Reuses the search engine's [`RowMatcher`] so
/// Plain / Wildcard / Regex semantics match the search bar.
pub fn replace_in_column(
    table: &DataTable,
    col: usize,
    matcher: &RowMatcher,
    replacement: &str,
) -> Vec<CellValue> {
    let n = table.row_count();
    (0..n)
        .map(|r| {
            let cur = table.get(r, col).cloned().unwrap_or(CellValue::Null);
            if matches!(cur, CellValue::Null) {
                return CellValue::Null;
            }
            let text = matcher.replace_all(&cur.to_string(), replacement);
            match cur {
                // A clock time or duration stays one when the result reads
                // as one; text in a time column would save as nothing.
                CellValue::Time(_) | CellValue::Duration(_) => CellValue::parse_like(&cur, &text),
                _ => CellValue::String(text),
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "replace_in_column_tests.rs"]
mod tests;
