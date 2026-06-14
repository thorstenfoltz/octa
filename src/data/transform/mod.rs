//! OpenRefine-style column-shaping transforms.
//!
//! Each operation is a **pure** function over a [`DataTable`] (no IO, no GUI
//! state) so the same logic backs the GUI dialog, future CLI/MCP surfaces, and
//! unit tests. One file per op (the project's drop-in module convention):
//!
//! - [`split_column`] - one column -> N columns (by delimiter / regex / width).
//! - [`merge_columns`] - N columns -> one joined column.
//! - [`fill_down`] / [`fill_up`] - propagate the previous/next non-empty value.
//! - [`extract_pattern`] - pull the first regex capture into a new column.
//! - [`replace_in_column`] - find/replace within one column's cells.
//!
//! Ops that produce **new** columns return the column data; ops that rewrite an
//! existing column return the replacement cell vector. The caller materialises
//! the result through [`DataTable::insert_column`] + [`DataTable::set`] so the
//! existing undo/redo machinery records the change.

pub mod conditional_value;
pub mod extract;
pub mod fill;
pub mod merge;
pub mod replace_in_column;
pub mod split;

pub use conditional_value::{CaseRule, CaseSpec, build_case_column, infer_case_column_type};
pub use extract::extract_pattern;
pub use fill::{fill_down, fill_up};
pub use merge::merge_columns;
pub use replace_in_column::replace_in_column;
pub use split::{SplitSpec, split_column};

use super::{CellValue, DataTable};

/// Display text of a cell, empty string for `Null` / out-of-range.
fn cell_text(table: &DataTable, row: usize, col: usize) -> String {
    table
        .get(row, col)
        .map(|v| v.to_string())
        .unwrap_or_default()
}

/// A cell counts as "empty" for fill purposes when it is `Null` or renders to
/// an empty string.
fn is_empty(value: &CellValue) -> bool {
    matches!(value, CellValue::Null) || value.to_string().is_empty()
}
