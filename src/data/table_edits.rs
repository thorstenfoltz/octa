//! A small, surface-agnostic table-edit engine shared by the MCP/chat
//! `edit_table` (file on disk) and the chat `edit_open_tab` (live tab) tools.
//! Ops apply in a fixed, index-stable order: add columns, insert rows, set
//! cells, delete rows (deletes last, highest-index-first), so earlier ops never
//! invalidate the indices later ops use.

use crate::data::{CellValue, DataTable};

/// A column reference inside an edit op: a 0-based index or a column name.
#[derive(Debug, Clone)]
pub enum EditColRef {
    Index(usize),
    Name(String),
}

/// One edit operation against a `DataTable`.
#[derive(Debug, Clone)]
pub enum EditOp {
    /// Append a computed column. `expression` is a DuckDB SQL expression
    /// evaluated per row against the table (scalar or window).
    AddColumn { name: String, expression: String },
    /// Insert literal rows. `at` defaults to append; each row lines up
    /// positionally with the current columns.
    InsertRows {
        at: Option<usize>,
        rows: Vec<Vec<CellValue>>,
    },
    /// Set individual cells.
    SetCells(Vec<(usize, EditColRef, CellValue)>),
    /// Delete rows by index.
    DeleteRows(Vec<usize>),
    /// Drop columns (by index or name). Applied last so every other op's
    /// column references still line up with the input table.
    DropColumns(Vec<EditColRef>),
}

/// What `apply_edit_ops` did, for a tool's response summary.
#[derive(Debug, Default, Clone)]
pub struct EditSummary {
    pub columns_added: usize,
    pub rows_inserted: usize,
    pub cells_set: usize,
    pub rows_deleted: usize,
    pub columns_dropped: usize,
}

fn resolve_col(table: &DataTable, col: &EditColRef) -> anyhow::Result<usize> {
    match col {
        EditColRef::Index(i) => {
            if *i >= table.col_count() {
                anyhow::bail!(
                    "column {i} is out of range (table has {} columns)",
                    table.col_count()
                );
            }
            Ok(*i)
        }
        EditColRef::Name(name) => table
            .columns
            .iter()
            .position(|c| &c.name == name)
            .ok_or_else(|| anyhow::anyhow!("no column named \"{name}\"")),
    }
}

/// Compute the values for an `AddColumn` expression, aligned to the table's
/// rows. A hidden ordinal `__octa_rownum` (0..n-1) is injected and selected
/// alongside the expression, then the result is mapped back by ordinal, so the
/// engine never relies on DuckDB preserving input order. Returns the values
/// (length == `table.row_count()`) and the inferred Arrow type string.
pub fn compute_column_values(
    table: &DataTable,
    expression: &str,
) -> anyhow::Result<(Vec<CellValue>, String)> {
    let n = table.row_count();
    let mut work = table.clone();
    work.apply_edits();
    let rn_idx = work.col_count();
    work.insert_column(rn_idx, "__octa_rownum".to_string(), "Int64".to_string());
    for r in 0..n {
        work.set(r, rn_idx, CellValue::Int(r as i64));
    }
    work.apply_edits();

    let query = format!("SELECT __octa_rownum AS rn, ({expression}) AS v FROM data");
    let outcome = crate::sql::run_query(&work, &query)
        .map_err(|e| anyhow::anyhow!("evaluating expression: {e}"))?;
    let res = outcome.table; // workspace result columns are all Utf8 text

    let mut values = vec![CellValue::Null; n];
    for r in 0..res.row_count() {
        let rn = match res.get(r, 0) {
            Some(c) => c.to_string().trim().parse::<usize>().ok(),
            None => None,
        };
        if let Some(rn) = rn.filter(|i| *i < n) {
            values[rn] = parse_cell(res.get(r, 1));
        }
    }
    let ty = infer_arrow_type(&values);
    Ok((values, ty))
}

/// Parse a workspace result cell (always text) into the tightest CellValue.
fn parse_cell(c: Option<&CellValue>) -> CellValue {
    let s = match c {
        None | Some(CellValue::Null) => return CellValue::Null,
        Some(v) => v.to_string(),
    };
    let t = s.trim();
    if t.is_empty() {
        return CellValue::Null;
    }
    if let Ok(i) = t.parse::<i64>() {
        return CellValue::Int(i);
    }
    if let Ok(f) = t.parse::<f64>() {
        return CellValue::Float(f);
    }
    CellValue::String(s)
}

fn infer_arrow_type(values: &[CellValue]) -> String {
    let mut seen_float = false;
    let mut seen_int = false;
    let mut seen_other = false;
    for v in values {
        match v {
            CellValue::Null => {}
            CellValue::Int(_) => seen_int = true,
            CellValue::Float(_) => seen_float = true,
            _ => seen_other = true,
        }
    }
    if seen_other || (!seen_int && !seen_float) {
        "Utf8".to_string()
    } else if seen_float {
        "Float64".to_string()
    } else {
        "Int64".to_string()
    }
}

/// Apply `ops` to `table` in the canonical order. Mutates `table` in place
/// (cell edits are folded with `apply_edits` at the end so the writer sees
/// committed values). Returns a summary of counts.
pub fn apply_edit_ops(table: &mut DataTable, ops: &[EditOp]) -> anyhow::Result<EditSummary> {
    let mut summary = EditSummary::default();

    // 1. Add columns (each computed against the running table).
    for op in ops {
        if let EditOp::AddColumn { name, expression } = op {
            let (values, ty) = compute_column_values(table, expression)?;
            let idx = table.col_count();
            table.insert_column(idx, name.clone(), ty);
            for (r, v) in values.into_iter().enumerate() {
                table.set(r, idx, v);
            }
            summary.columns_added += 1;
        }
    }
    // 2. Insert rows.
    for op in ops {
        if let EditOp::InsertRows { at, rows } = op {
            for row in rows {
                if row.len() != table.col_count() {
                    anyhow::bail!(
                        "insert row has {} cells but the table has {} columns",
                        row.len(),
                        table.col_count()
                    );
                }
                let at_i = at
                    .unwrap_or_else(|| table.row_count())
                    .min(table.row_count());
                table.insert_row(at_i);
                for (c, v) in row.iter().enumerate() {
                    table.set(at_i, c, v.clone());
                }
                summary.rows_inserted += 1;
            }
        }
    }
    // 3. Set cells.
    for op in ops {
        if let EditOp::SetCells(cells) = op {
            for (row, col, value) in cells {
                let c = resolve_col(table, col)?;
                if *row >= table.row_count() {
                    anyhow::bail!(
                        "set: row {row} is out of range (table has {} rows)",
                        table.row_count()
                    );
                }
                table.set(*row, c, value.clone());
                summary.cells_set += 1;
            }
        }
    }
    // 4. Delete rows (highest index first).
    let mut to_delete: Vec<usize> = ops
        .iter()
        .filter_map(|op| match op {
            EditOp::DeleteRows(idxs) => Some(idxs.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    to_delete.sort_unstable();
    to_delete.dedup();
    for &idx in to_delete.iter().rev() {
        if idx >= table.row_count() {
            anyhow::bail!(
                "delete: row {idx} is out of range (table has {} rows)",
                table.row_count()
            );
        }
        table.delete_row(idx);
        summary.rows_deleted += 1;
    }

    // 5. Drop columns. Resolve every reference against the current layout
    // first, then remove highest-index-first so earlier removals don't shift
    // the indices still to be removed.
    let mut cols_to_drop: Vec<usize> = Vec::new();
    for op in ops {
        if let EditOp::DropColumns(cols) = op {
            for col in cols {
                cols_to_drop.push(resolve_col(table, col)?);
            }
        }
    }
    cols_to_drop.sort_unstable();
    cols_to_drop.dedup();
    if cols_to_drop.len() == table.col_count() && !cols_to_drop.is_empty() {
        anyhow::bail!("cannot drop every column - a table must keep at least one column");
    }
    for &idx in cols_to_drop.iter().rev() {
        table.delete_column(idx);
        summary.columns_dropped += 1;
    }

    table.apply_edits();
    Ok(summary)
}

#[cfg(test)]
#[path = "table_edits_tests.rs"]
mod tests;
