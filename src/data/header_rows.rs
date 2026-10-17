//! Fix the header row after opening: use a row (or a block of rows) further
//! down as the column names, or turn the names back into the first row.
//!
//! Exports from banks, portals and ERP systems put a title block above the
//! real header, split it over two rows (a merged `2025` above `Q1 Q2`), or
//! have none. Load-time problems are fixed after opening, never behind a
//! gate before it: [`suggest`] only proposes, nothing here runs on load.
//!
//! Both actions go through `DataTable`'s undoable primitives and fold into
//! one undo entry, and both detect the column types again the way opening a
//! file does: a column that was text only because of the title rows becomes
//! numbers again.

use std::collections::HashSet;

use crate::data::{
    CellValue, ColumnInfo, DataTable, UndoAction, column_type_from_cells, date_infer, num_parse,
};

/// Rows the load banner suggests as the header, 0-based and inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderSuggestion {
    pub first: usize,
    pub last: usize,
}

/// How far down [`suggest`] looks for the real header.
const SUGGEST_SCAN_ROWS: usize = 20;

/// Use rows `first..=last` (0-based) as the column names. They and every
/// row above them are removed. Text columns are then typed again, dates
/// included only with `detect_dates` (the GUI passes its "Detect dates on
/// load" setting: the check is automatic here, so it follows that switch).
pub fn use_header_rows(
    table: &mut DataTable,
    first: usize,
    last: usize,
    detect_dates: bool,
) -> anyhow::Result<()> {
    if first > last || last >= table.row_count() {
        anyhow::bail!(
            "the table has {} rows, so rows {}-{} cannot be the header",
            table.row_count(),
            first + 1,
            last + 1
        );
    }
    // Everything below reads and writes `rows`; pending edits must be in it.
    table.apply_edits();
    let header: Vec<Vec<String>> = (first..=last)
        .map(|r| {
            (0..table.col_count())
                .map(|c| name_text(table.get(r, c)))
                .collect()
        })
        .collect();
    let names = combined_names(&header, table.col_count());
    let start = table.undo_stack.len();
    for r in (0..=last).rev() {
        table.delete_row(r);
    }
    for (i, name) in names.into_iter().enumerate() {
        table.rename_column(i, name);
    }
    redetect_types(table, detect_dates);
    table.coalesce_undo_since(start);
    Ok(())
}

/// Turn the column names into the first row and name the columns
/// `column_1 .. column_N`: for a file that has no header, whose first data
/// row became the names. `detect_dates` as for [`use_header_rows`].
pub fn header_is_data(table: &mut DataTable, detect_dates: bool) -> anyhow::Result<()> {
    if table.col_count() == 0 {
        anyhow::bail!("the table has no columns");
    }
    table.apply_edits();
    let names: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();
    let start = table.undo_stack.len();
    table.insert_row(0);
    for (c, name) in names.iter().enumerate() {
        table.set(0, c, crate::formats::csv_reader::infer_cell_value(name));
    }
    // `set` writes the edit overlay; the type pass below writes `rows`.
    table.apply_edits();
    for c in 0..names.len() {
        table.rename_column(c, format!("column_{}", c + 1));
    }
    redetect_types(table, detect_dates);
    table.coalesce_undo_since(start);
    Ok(())
}

/// The rows the load banner should offer as the header, or `None`.
///
/// Only when more than half of the column names are placeholders, which is
/// what a title block above the real header leaves behind: the CSV reader's
/// `column_N`, Excel's `Column{n}`, empty names. Then the first of the top
/// [`SUGGEST_SCAN_ROWS`] rows that is filled across and mostly words. If the
/// row above it is a merged group row (two or more filled cells with gaps),
/// both are offered. A row with a single filled cell is a title, not a header.
pub fn suggest(table: &DataTable) -> Option<HeaderSuggestion> {
    let n = table.col_count();
    if n < 2 {
        return None;
    }
    let placeholders = table
        .columns
        .iter()
        .enumerate()
        .filter(|(i, c)| is_placeholder(&c.name, *i))
        .count();
    if placeholders * 2 <= n {
        return None;
    }
    let last =
        (0..table.row_count().min(SUGGEST_SCAN_ROWS)).find(|&r| row_is_header_like(table, r))?;
    let first = if last > 0 && row_is_group_header(table, last - 1) {
        last - 1
    } else {
        last
    };
    Some(HeaderSuggestion { first, last })
}

/// Whether the names are real rather than the `column_1 .. column_N` that
/// [`header_is_data`] leaves behind. Drives the Edit menu's "First row is
/// header" tick, so it follows an undo instead of going stale.
pub fn has_real_header(columns: &[ColumnInfo]) -> bool {
    columns.is_empty()
        || !columns
            .iter()
            .enumerate()
            .all(|(i, c)| c.name == format!("column_{}", i + 1))
}

/// The rows a right-click on `clicked` means: the block of consecutive
/// selected rows around it, or just `clicked` when it is not selected or the
/// selection has gaps.
pub fn header_block(selected: &HashSet<usize>, clicked: usize) -> (usize, usize) {
    if !selected.contains(&clicked) {
        return (clicked, clicked);
    }
    let first = selected.iter().copied().min().unwrap_or(clicked);
    let last = selected.iter().copied().max().unwrap_or(clicked);
    if last - first + 1 == selected.len() {
        (first, last)
    } else {
        (clicked, clicked)
    }
}

/// Column names from one or more header rows (`rows[k][col]`, blank = "").
/// In every row but the last, a blank takes the nearest non-blank value to
/// its left: a merged Excel cell keeps its text only in the first cell of the
/// span. Per column the non-blank parts are joined with one space; an empty
/// result is `column_N`, and a repeat gets `_2`, `_3`.
pub fn combined_names(rows: &[Vec<String>], width: usize) -> Vec<String> {
    let mut filled: Vec<Vec<String>> = rows.to_vec();
    let last = filled.len().saturating_sub(1);
    for row in filled.iter_mut().take(last) {
        let mut carry = String::new();
        for cell in row.iter_mut() {
            if cell.trim().is_empty() {
                cell.clone_from(&carry);
            } else {
                carry.clone_from(cell);
            }
        }
    }
    let mut names: Vec<String> = Vec::with_capacity(width);
    for col in 0..width {
        let parts: Vec<&str> = filled
            .iter()
            .filter_map(|r| r.get(col))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        let base = if parts.is_empty() {
            format!("column_{}", col + 1)
        } else {
            parts.join(" ")
        };
        let name = crate::data::recipe::unique_name(&names, &base);
        names.push(name);
    }
    names
}

/// A header cell as a name: blank for empty cells, a whole float without
/// its `.0` (`2025`, as Excel shows it), everything else as displayed.
fn name_text(cell: Option<&CellValue>) -> String {
    match cell {
        None | Some(CellValue::Null) => String::new(),
        Some(CellValue::Float(v)) if v.fract() == 0.0 && v.abs() < 1e15 => {
            format!("{}", *v as i64)
        }
        Some(v) => v.to_string().trim().to_string(),
    }
}

fn is_text_type(data_type: &str) -> bool {
    matches!(data_type, "Utf8" | "LargeUtf8" | "String")
}

/// Give every column the type opening the file would have given it, now
/// that rows at the top are gone (or back). A text column takes the type its
/// cells now support; a typed column (`Int32`, `Decimal`) keeps its precise
/// type unless it now holds text. Certain date and number layouts are
/// applied to text columns; an ambiguous one stays text, as nobody can be
/// asked in the middle of an edit. Each changed column pushes one
/// `ConvertColumn`, so undo and redo restore types and values together.
fn redetect_types(table: &mut DataTable, detect_dates: bool) {
    for col in 0..table.col_count() {
        let old_type = table.columns[col].data_type.clone();
        let old_values = column_values(table, col);
        let inferred = column_type_from_cells(old_values.iter());
        if is_text_type(&old_type) || inferred == "Utf8" {
            table.columns[col].data_type = inferred.to_string();
        }
        if is_text_type(&table.columns[col].data_type) {
            promote_text_column(table, col, detect_dates);
        }
        let new_type = table.columns[col].data_type.clone();
        let new_values = column_values(table, col);
        if new_type != old_type || new_values != old_values {
            table.undo_stack.push(UndoAction::ConvertColumn {
                col_idx: col,
                old_type,
                new_type,
                old_values,
                new_values,
            });
            table.redo_stack.clear();
            table.structural_changes = true;
        }
    }
}

fn column_values(table: &DataTable, col: usize) -> Vec<CellValue> {
    (0..table.row_count())
        .map(|r| table.get(r, col).cloned().unwrap_or(CellValue::Null))
        .collect()
}

/// The loader's date detection (when `detect_dates`), then its number
/// detection, on one text column, keeping only certain outcomes.
fn promote_text_column(table: &mut DataTable, col: usize, detect_dates: bool) {
    if detect_dates && date_infer::column_is_candidate(table, col) {
        let outcome = {
            let strings = date_infer::collect_column_strings(table, col);
            date_infer::infer_column(&strings)
        };
        match outcome {
            date_infer::InferOutcome::PromotedDate(layout) => {
                date_infer::apply_date(table, col, layout);
                return;
            }
            date_infer::InferOutcome::PromotedDateTime(layout) => {
                date_infer::apply_datetime(table, col, layout);
                return;
            }
            _ => {}
        }
    }
    if num_parse::column_is_candidate(table, col) {
        let outcome = {
            let strings = date_infer::collect_column_strings(table, col);
            num_parse::infer_column(&strings)
        };
        if let num_parse::NumberOutcome::Promote(style) = outcome {
            num_parse::apply_style(table, col, style);
        }
    }
}

/// An empty name, or the one a reader or Octa invents for a column without
/// one: `column_3`, `Column3`, `column3` at (1-based) position 3.
fn is_placeholder(name: &str, idx: usize) -> bool {
    let squashed: String = name
        .trim()
        .to_lowercase()
        .chars()
        .filter(|&c| c != '_')
        .collect();
    squashed.is_empty() || squashed == format!("column{}", idx + 1)
}

fn is_blank(cell: Option<&CellValue>) -> bool {
    match cell {
        None | Some(CellValue::Null) => true,
        Some(CellValue::String(s)) => s.trim().is_empty(),
        _ => false,
    }
}

/// Every cell filled, and at least half of them words rather than numbers.
fn row_is_header_like(table: &DataTable, r: usize) -> bool {
    let n = table.col_count();
    let mut words = 0;
    for c in 0..n {
        let cell = table.get(r, c);
        if is_blank(cell) {
            return false;
        }
        if let Some(CellValue::String(s)) = cell
            && num_parse::parse_number_relaxed(s).is_none()
        {
            words += 1;
        }
    }
    words * 2 >= n
}

/// Two or more filled cells with gaps between them: a merged group row.
fn row_is_group_header(table: &DataTable, r: usize) -> bool {
    let n = table.col_count();
    let filled = (0..n).filter(|&c| !is_blank(table.get(r, c))).count();
    filled >= 2 && filled < n
}

#[cfg(test)]
#[path = "header_rows_tests.rs"]
mod tests;
