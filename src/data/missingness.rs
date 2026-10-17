//! Missingness patterns: which columns go missing *together*.
//!
//! "Column X is 12% null" is on the quality report already, one row per column,
//! and it does not tell you the thing that matters. Four columns each 8% null
//! is a different problem depending on whether they are null in the same rows
//! (one upstream join that did not match) or in different ones (four unrelated
//! gaps). This finds the first case.
//!
//! The method is deliberately plain: reduce each row to the set of columns that
//! are null in it, count how many rows share each set, and report the sets that
//! cover enough of the table to mean something. No clustering, no distance
//! metric - rows either have the same holes or they do not.

use std::collections::HashMap;

use crate::data::{CellValue, ColumnInfo, DataTable};

/// A pattern must cover at least this share of the table's rows to be worth a
/// line in the report. Below it, the "pattern" is a handful of rows and the
/// per-column null percentages already say everything there is to say.
pub const MIN_SHARE: f64 = 0.01;

/// Patterns covering fewer rows than this are noise however large the share,
/// which matters for a small table: 1% of 40 rows is not a pattern.
pub const MIN_ROWS: usize = 5;

/// One set of columns that go missing together.
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    /// Indices of the columns that are null, ascending.
    pub columns: Vec<usize>,
    /// How many rows have exactly this set of nulls.
    pub rows: usize,
    /// `rows` as a share of the table, 0.0 to 1.0.
    pub share: f64,
}

/// Find the sets of columns that are null together, most rows first.
///
/// Rows with no nulls are not a pattern and never appear. Neither does the
/// single-column case: "this one column is null in 300 rows" is the null
/// percentage the main report already carries, and repeating it here would
/// bury the multi-column findings this exists for.
pub fn find_patterns(table: &DataTable) -> Vec<Pattern> {
    let row_count = table.row_count();
    let col_count = table.col_count();
    if row_count == 0 || col_count == 0 {
        return Vec::new();
    }

    let mut counts: HashMap<Vec<usize>, usize> = HashMap::new();
    for row in 0..row_count {
        let missing: Vec<usize> = (0..col_count)
            .filter(|&col| is_missing(table.get(row, col)))
            .collect();
        if missing.len() < 2 {
            continue;
        }
        *counts.entry(missing).or_insert(0) += 1;
    }

    let floor = MIN_ROWS.max((row_count as f64 * MIN_SHARE).ceil() as usize);
    let mut out: Vec<Pattern> = counts
        .into_iter()
        .filter(|(_, rows)| *rows >= floor)
        .map(|(columns, rows)| Pattern {
            columns,
            rows,
            share: rows as f64 / row_count as f64,
        })
        .collect();
    // Most rows first; ties broken by column order so the output is stable
    // across runs (a HashMap's iteration order is not).
    out.sort_by(|a, b| b.rows.cmp(&a.rows).then_with(|| a.columns.cmp(&b.columns)));
    out
}

/// A cell counts as missing when it is null or an empty string. An empty text
/// cell is a hole in the data whatever the reader chose to call it, and CSV
/// readers differ on which one they produce.
fn is_missing(cell: Option<&CellValue>) -> bool {
    match cell {
        None | Some(CellValue::Null) => true,
        Some(CellValue::String(s)) => s.is_empty(),
        _ => false,
    }
}

/// Render the patterns as the quality report's section table, or `None` when
/// there is nothing to say - a clean table must not grow an empty tab.
pub fn section_table(table: &DataTable) -> Option<DataTable> {
    let patterns = find_patterns(table);
    if patterns.is_empty() {
        return None;
    }
    let mut out = DataTable::empty();
    out.columns = ["columns", "column_count", "rows", "share_percent"]
        .iter()
        .map(|name| ColumnInfo {
            name: (*name).to_string(),
            data_type: if *name == "columns" { "Utf8" } else { "Int64" }.to_string(),
        })
        .collect();
    // `share_percent` is a rounded float, not an integer count.
    out.columns[3].data_type = "Float64".to_string();

    out.rows = patterns
        .iter()
        .map(|p| {
            let names: Vec<&str> = p
                .columns
                .iter()
                .filter_map(|&c| table.columns.get(c).map(|col| col.name.as_str()))
                .collect();
            vec![
                CellValue::String(names.join(", ")),
                CellValue::Int(p.columns.len() as i64),
                CellValue::Int(p.rows as i64),
                CellValue::Float((p.share * 1000.0).round() / 10.0),
            ]
        })
        .collect();
    Some(out)
}

#[cfg(test)]
#[path = "missingness_tests.rs"]
mod tests;
