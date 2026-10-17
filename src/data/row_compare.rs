//! Compare N rows of one table, field by field.
//!
//! The output is the transpose of just those rows - one output row per
//! original column - with a `differs` flag in front, so the eye runs down a
//! single narrow column to find where the records disagree:
//!
//! ```text
//! column   differs  row 12   row 4711  row 900001
//! city     no       Aachen   Aachen    Aachen
//! amount   yes      12.50    13.10     12.50
//! ```
//!
//! Built for arbitrary N, never for two: three marked rows are three columns.
//! That is why this does not go through [`crate::data::compare`], whose A-vs-B
//! `status` / `changed_columns` shape only ever describes a pair.
//!
//! Pure, and composed of two engines that already exist:
//! [`crate::data::compare::subset`] pulls the rows out and
//! [`crate::data::transpose::transpose_table`] turns them on their side.

use std::collections::HashSet;

use crate::data::compare::subset;
use crate::data::transpose::transpose_table;
use crate::data::{CellValue, ColumnInfo, DataTable};

/// Column holding the `yes` flag, in front of the compared rows.
const DIFFERS_COL: usize = 1;

/// The rows to compare: the selection when it holds at least two rows, else
/// the marked rows.
///
/// Selecting is the lighter gesture and therefore wins, which is what makes
/// the entry usable without marking anything first. It only wins when it holds
/// two or more rows, so a single click that lands on a row does not silently
/// replace a set of marks the user built up on purpose. One place, because the
/// menu asks how many rows there are and the tab opener asks which ones.
pub fn rows_to_compare(table: &DataTable, selected: &HashSet<usize>) -> Vec<usize> {
    if selected.len() >= 2 {
        let mut rows: Vec<usize> = selected.iter().copied().collect();
        rows.sort_unstable();
        return rows;
    }
    marked_rows(table)
}

/// The rows the user has marked, ascending.
///
/// Only whole-row marks count: a marked *cell* says something about that cell,
/// and reading it as "compare this row" would put rows in the comparison the
/// user never asked for. One place, because the menu asks whether there are
/// enough of them and the tab opener asks which ones.
pub fn marked_rows(table: &DataTable) -> Vec<usize> {
    let mut rows: Vec<usize> = table
        .marks
        .keys()
        .filter_map(|k| match k {
            crate::data::MarkKey::Row(r) => Some(*r),
            _ => None,
        })
        .collect();
    rows.sort_unstable();
    rows
}

/// Compare `rows` of `table` field by field.
///
/// Row labels carry `row_offset`, so a page of a large file names the rows the
/// user sees. Values are compared as the text the transpose produced, which is
/// exactly what the resulting tab displays: a cell cannot look identical and
/// still be flagged, or the flag would be untrustworthy.
pub fn compare_rows(table: &DataTable, rows: &[usize]) -> DataTable {
    let mut out = transpose_table(&subset(table, rows));

    for (i, &r) in rows.iter().enumerate() {
        if let Some(col) = out.columns.get_mut(i + 1) {
            col.name = format!("row {}", r + 1 + table.row_offset);
        }
    }

    out.columns.insert(
        DIFFERS_COL,
        ColumnInfo {
            name: "differs".to_string(),
            data_type: "Utf8".to_string(),
        },
    );
    for row in &mut out.rows {
        let differs = row[DIFFERS_COL..]
            .iter()
            .any(|v| v.to_string() != row[DIFFERS_COL].to_string());
        // Spelled out both ways: an empty cell reads as "not checked" and
        // leaves the reader guessing which it is.
        let flag = if differs { "yes" } else { "no" };
        row.insert(DIFFERS_COL, CellValue::String(flag.to_string()));
    }

    out
}

#[cfg(test)]
#[path = "row_compare_tests.rs"]
mod tests;
