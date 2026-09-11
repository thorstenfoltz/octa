//! Chat-only tool: edit the live, open GUI tab (add a computed column, insert
//! rows, set cells, delete rows). Resolves the target tab snapshot, computes /
//! validates every op against it, and queues one batched `PendingTabEdit` that
//! `OctaApp` applies on the UI thread (undoable). No MCP `handle` - this needs
//! the GUI write-back channel.

use serde_json::{Map, Value};

use octa::data::CellValue;

use super::{PendingTabEdit, ResolvedOp, ToolContext};

pub const DESCRIPTION: &str = "Edit the LIVE open tab so the user sees it immediately (undoable). \
Prefer this over `edit_table` for data that is open. Ops: add_column (a DuckDB SQL expression, \
scalar or window), insert_rows, set_cells, delete_rows, drop_columns (remove columns by index or \
name), sort_rows (reorder the whole tab by one or more columns, each ascending or descending - \
this is how you sort or 'order by' what the user has open). drop_columns is applied last, so the \
other ops still reference the original columns. The user still saves to persist. Only works when \
Write protection is off in Settings.";

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Which open tab to edit: a handle like `#2`, `@active`, or the tab name.
    pub open_tab: String,
    /// Ordered edit operations. Applied add_column -> insert_rows -> set_cells
    /// -> delete_rows regardless of array order.
    pub ops: Vec<OpSpec>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum OpSpec {
    AddColumn {
        name: String,
        expression: String,
    },
    InsertRows {
        #[serde(default)]
        at: Option<usize>,
        rows: Vec<Vec<Value>>,
    },
    SetCells {
        cells: Vec<CellSpec>,
    },
    DeleteRows {
        rows: Vec<usize>,
    },
    DropColumns {
        cols: Vec<ColRef>,
    },
    /// Reorder every row of the tab. Keys are most-significant first.
    SortRows {
        by: Vec<SortSpec>,
    },
}

/// One sort key: which column, and which way round.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SortSpec {
    pub col: ColRef,
    /// Descending when true; ascending (the default) otherwise.
    #[serde(default)]
    pub descending: bool,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CellSpec {
    pub row: usize,
    pub col: ColRef,
    pub value: Value,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum ColRef {
    Index(usize),
    Name(String),
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let queue = ctx.pending_tab_edits.as_ref().ok_or_else(|| {
        anyhow::anyhow!("editing the live tab is only available in the in-GUI assistant")
    })?;
    if !ctx.allow_existing_writes {
        anyhow::bail!(
            "Modifying open data is turned off (Write protection). Turn it off in Settings to let me \
             edit the open tab, or I can save the result to a new file in the export directory instead."
        );
    }

    let snap = ctx
        .snapshot_for_open_tab(&p.open_tab)
        .ok_or_else(|| anyhow::anyhow!("no open tab matches \"{}\"", p.open_tab))?;
    // A large-file tab holds one page read from disk and is read-only in the
    // GUI. Editing it would compute ops against a window and apply them to a
    // table that is about to be replaced by the next page.
    if snap.window.is_some() {
        anyhow::bail!(
            "\"{}\" is open in large-file mode, which is read-only - it streams from disk as the \
             user scrolls, so there is nothing in memory to edit. Write the result to a new file \
             instead.",
            p.open_tab
        );
    }
    let table = &snap.table;
    let n = table.row_count();

    let mut resolved: Vec<ResolvedOp> = Vec::new();
    // Sorting permutes rows, so it is queued after every op that addresses a
    // row by index. Mixing it with drop_columns is refused outright below:
    // dropping shifts column indices, and the sort keys are column indices.
    let mut sorts: Vec<ResolvedOp> = Vec::new();
    // cols_added, rows_ins, cells, rows_del, cols_dropped, sort_keys
    let mut summary = (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);

    for op in &p.ops {
        match op {
            OpSpec::AddColumn { name, expression } => {
                let (values, type_name) = octa::data::compute_column_values(table, expression)?;
                resolved.push(ResolvedOp::AddColumn {
                    name: name.clone(),
                    type_name,
                    values,
                });
                summary.0 += 1;
            }
            OpSpec::InsertRows { at, rows } => {
                let mut out_rows = Vec::with_capacity(rows.len());
                for row in rows {
                    if row.len() != table.col_count() {
                        anyhow::bail!(
                            "insert row has {} cells but the tab has {} columns",
                            row.len(),
                            table.col_count()
                        );
                    }
                    out_rows.push(
                        row.iter()
                            .enumerate()
                            .map(|(c, v)| super::cell_from_json(v, &table.columns[c].data_type))
                            .collect::<Vec<CellValue>>(),
                    );
                }
                summary.1 += out_rows.len();
                resolved.push(ResolvedOp::InsertRows {
                    at: *at,
                    rows: out_rows,
                });
            }
            OpSpec::SetCells { cells } => {
                let mut out = Vec::with_capacity(cells.len());
                for c in cells {
                    let col = match &c.col {
                        ColRef::Index(i) => *i,
                        ColRef::Name(name) => table
                            .columns
                            .iter()
                            .position(|ci| &ci.name == name)
                            .ok_or_else(|| anyhow::anyhow!("no column named \"{name}\""))?,
                    };
                    if col >= table.col_count() {
                        anyhow::bail!("column {col} is out of range");
                    }
                    if c.row >= n {
                        anyhow::bail!("row {} is out of range (tab has {n} rows)", c.row);
                    }
                    let ty = &table.columns[col].data_type;
                    out.push((c.row, col, super::cell_from_json(&c.value, ty)));
                }
                summary.2 += out.len();
                resolved.push(ResolvedOp::SetCells(out));
            }
            OpSpec::DeleteRows { rows } => {
                for &r in rows {
                    if r >= n {
                        anyhow::bail!("delete: row {r} is out of range (tab has {n} rows)");
                    }
                }
                summary.3 += rows.len();
                resolved.push(ResolvedOp::DeleteRows(rows.clone()));
            }
            OpSpec::DropColumns { cols } => {
                let mut idxs = Vec::with_capacity(cols.len());
                for c in cols {
                    let col = match c {
                        ColRef::Index(i) => {
                            if *i >= table.col_count() {
                                anyhow::bail!("drop: column {i} is out of range");
                            }
                            *i
                        }
                        ColRef::Name(name) => table
                            .columns
                            .iter()
                            .position(|ci| &ci.name == name)
                            .ok_or_else(|| anyhow::anyhow!("no column named \"{name}\""))?,
                    };
                    idxs.push(col);
                }
                idxs.sort_unstable();
                idxs.dedup();
                if idxs.len() == table.col_count() {
                    anyhow::bail!(
                        "cannot drop every column - a table must keep at least one column"
                    );
                }
                summary.4 += idxs.len();
                resolved.push(ResolvedOp::DropColumns(idxs));
            }
            OpSpec::SortRows { by } => {
                if by.is_empty() {
                    anyhow::bail!("sort_rows needs at least one column to sort by");
                }
                let mut keys = Vec::with_capacity(by.len());
                for k in by {
                    let col = col_index(table, &k.col)?;
                    keys.push((col, !k.descending));
                }
                summary.5 += keys.len();
                sorts.push(ResolvedOp::SortRows(keys));
            }
        }
    }

    if !sorts.is_empty() && summary.4 > 0 {
        anyhow::bail!(
            "sort_rows and drop_columns cannot be combined in one call: dropping a column renumbers \
             the columns the sort keys name. Drop the columns first, then sort in a second call."
        );
    }
    resolved.extend(sorts);

    queue.lock().unwrap().push(PendingTabEdit {
        tab_handle: snap.handle.clone(),
        snapshot_rows: n,
        ops: resolved,
    });

    let mut out = Map::new();
    out.insert("applied_to".to_string(), Value::String(snap.handle.clone()));
    out.insert("columns_added".to_string(), Value::from(summary.0));
    out.insert("rows_inserted".to_string(), Value::from(summary.1));
    out.insert("cells_set".to_string(), Value::from(summary.2));
    out.insert("rows_deleted".to_string(), Value::from(summary.3));
    out.insert("columns_dropped".to_string(), Value::from(summary.4));
    out.insert("sort_keys".to_string(), Value::from(summary.5));
    out.insert(
        "note".to_string(),
        Value::String("Applied to the live tab. The user must save to persist.".to_string()),
    );
    Ok(Value::Object(out))
}

/// Resolve a [`ColRef`] to a column index, bounds-checked.
fn col_index(table: &octa::data::DataTable, c: &ColRef) -> anyhow::Result<usize> {
    match c {
        ColRef::Index(i) => {
            if *i >= table.col_count() {
                anyhow::bail!(
                    "column {i} is out of range (the tab has {})",
                    table.col_count()
                );
            }
            Ok(*i)
        }
        ColRef::Name(name) => table
            .columns
            .iter()
            .position(|ci| &ci.name == name)
            .ok_or_else(|| anyhow::anyhow!("no column named \"{name}\"")),
    }
}

#[cfg(test)]
#[path = "edit_open_tab_tests.rs"]
mod tests;
