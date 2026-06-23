//! MCP tool: `edit_table` - edit an existing file in place. Set individual
//! cells, insert rows, delete rows, and append computed columns, then save
//! back through the same reader the file was loaded with. Database sources
//! (SQLite / DuckDB) keep their diff-based save semantics: only the rows that
//! actually changed are UPDATE/INSERT/DELETE-d, because the reader snapshots
//! row identity into `db_meta` on load and `apply_edits` leaves that snapshot
//! intact.
//!
//! Adding a column to a DuckDB / SQLite / GeoPackage file is a schema change.
//! It requires Write protection to be turned off in Settings (the
//! `allow_schema_changes` flag on `ToolContext`).

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::formats::FormatRegistry;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, cell_from_json, read_with_registry};

pub const DESCRIPTION: &str = "Edit an existing tabular file in place via its native writer. \
`set` updates cells, `insert_rows` adds rows, `delete_rows` removes rows by index, \
`add_column` appends a new column whose values are computed from a DuckDB SQL expression \
(e.g. `price * 0.9` or a window function), and `drop_column` removes columns by index or name. \
SQLite / DuckDB keep diff-based save (only changed rows are written). Adding or dropping a \
column on a DuckDB, SQLite, or GeoPackage file is a schema change and requires Write protection \
to be turned off in Settings. Use `table` for multi-table sources.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddColumnOp {
    /// Name of the new column.
    pub name: String,
    /// DuckDB SQL expression evaluated per row (scalar or window),
    /// e.g. `price * 0.9` or `AVG(v) OVER (ORDER BY id ROWS BETWEEN 6 PRECEDING AND CURRENT ROW)`.
    pub expression: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file to edit. Edited in place via its native writer.
    pub path: PathBuf,

    /// Reserved: editing an open GUI tab in place is not supported in v1.
    /// Setting this returns an error - edit a file path instead.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources (SQLite, DuckDB, GeoPackage, Excel), the table
    /// to edit. Defaults to the reader's `read_file` behaviour.
    #[serde(default)]
    pub table: Option<String>,

    /// Cell edits to apply. `col` is either a 0-based column index (number) or
    /// a column name (string). `row` is a 0-based row index into the loaded
    /// rows.
    #[serde(default)]
    pub set: Vec<CellEdit>,

    /// Rows to insert. `at` is the 0-based insertion index; omit (or pass null)
    /// to append at the end. `values` line up positionally with the columns.
    #[serde(default)]
    pub insert_rows: Vec<RowInsert>,

    /// 0-based row indices to delete (applied highest-first so indices stay
    /// valid).
    #[serde(default)]
    pub delete_rows: Vec<usize>,

    /// Columns to append. `expression` is a DuckDB SQL expression evaluated per
    /// row (scalar or window), e.g. `v * 2` or
    /// `AVG(v) OVER (ORDER BY id ROWS BETWEEN 6 PRECEDING AND CURRENT ROW)`.
    #[serde(default)]
    pub add_column: Vec<AddColumnOp>,

    /// Columns to drop, by 0-based index or name. Applied after the other ops,
    /// so their column references still match the file's original layout.
    /// Dropping a column from a DuckDB / SQLite / GeoPackage file is a schema
    /// change and needs Write protection off.
    #[serde(default)]
    pub drop_column: Vec<ColRef>,

    /// Lift the streaming initial-load cap so the whole file is loaded before
    /// editing (and rewritten in full for non-DB formats). Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CellEdit {
    /// 0-based row index.
    pub row: usize,
    /// Column index (number) or column name (string).
    pub col: ColRef,
    /// New cell value, coerced to the column's type.
    pub value: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RowInsert {
    /// 0-based insertion index. Omit to append at the end.
    #[serde(default)]
    pub at: Option<usize>,
    /// Cell values, positional with the columns.
    pub values: Vec<Value>,
}

/// A column reference: either a 0-based index or a column name.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum ColRef {
    Index(usize),
    Name(String),
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.open_tab.is_some() {
        anyhow::bail!(
            "editing an open tab in place is not supported yet; edit a file path instead"
        );
    }

    // Chat in-place gate: write protection must be off to modify existing files.
    if ctx.restrict_filesystem && !ctx.allow_existing_writes {
        anyhow::bail!(
            "Modifying existing files is turned off (Write protection). Turn it off in Settings to \
             let the assistant edit files in place, or write the result to a new file in the export \
             directory instead."
        );
    }

    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let path = &p.path;
    // In-place edits are only allowed on files the user has open (no-op for
    // MCP / CLI).
    ctx.ensure_readable(path)?;
    let registry = FormatRegistry::new();
    let reader = registry
        .reader_for_path(path)
        .ok_or_else(|| anyhow::anyhow!("no reader available for {}", path.display()))?;
    if !reader.supports_write() {
        anyhow::bail!(
            "format {} does not support writing - cannot edit {}",
            reader.name(),
            path.display()
        );
    }

    let mut table = read_with_registry(path, p.table.as_deref())?;
    let col_count_before = table.col_count();

    use octa::data::{EditColRef, EditOp};
    let mut ops: Vec<EditOp> = Vec::new();

    for a in &p.add_column {
        ops.push(EditOp::AddColumn {
            name: a.name.clone(),
            expression: a.expression.clone(),
        });
    }

    for ins in &p.insert_rows {
        let rows = vec![
            ins.values
                .iter()
                .enumerate()
                .map(|(c, v)| {
                    let ty = table
                        .columns
                        .get(c)
                        .map(|ci| ci.data_type.as_str())
                        .unwrap_or("Utf8");
                    cell_from_json(v, ty)
                })
                .collect::<Vec<_>>(),
        ];
        ops.push(EditOp::InsertRows { at: ins.at, rows });
    }

    if !p.set.is_empty() {
        let cells = p
            .set
            .iter()
            .map(|e| {
                let colref = match &e.col {
                    ColRef::Index(i) => EditColRef::Index(*i),
                    ColRef::Name(n) => EditColRef::Name(n.clone()),
                };
                let ty = match &e.col {
                    ColRef::Index(i) => table.columns.get(*i).map(|c| c.data_type.clone()),
                    ColRef::Name(n) => table
                        .columns
                        .iter()
                        .find(|c| &c.name == n)
                        .map(|c| c.data_type.clone()),
                }
                .unwrap_or_else(|| "Utf8".to_string());
                (e.row, colref, cell_from_json(&e.value, &ty))
            })
            .collect::<Vec<_>>();
        ops.push(EditOp::SetCells(cells));
    }

    if !p.delete_rows.is_empty() {
        ops.push(EditOp::DeleteRows(p.delete_rows.clone()));
    }

    if !p.drop_column.is_empty() {
        let cols = p
            .drop_column
            .iter()
            .map(|c| match c {
                ColRef::Index(i) => EditColRef::Index(*i),
                ColRef::Name(n) => EditColRef::Name(n.clone()),
            })
            .collect::<Vec<_>>();
        ops.push(EditOp::DropColumns(cols));
    }

    let summary = octa::data::apply_edit_ops(&mut table, &ops)?;

    // A column add/remove against a DB file is a schema change: give a friendly
    // refusal before the writer would fail.
    let schema_changed = table.col_count() != col_count_before;
    if schema_changed && !ctx.allow_schema_changes && is_db_file(path) {
        anyhow::bail!(
            "Adding or removing columns on a database file is a schema change, which is turned \
             off. Turn off Write protection in Settings to allow it."
        );
    }

    // Fold edits into rows, then write through the native writer (the GUI save
    // sequence). DB diff-save keys off db_meta.original, which apply_edits
    // leaves untouched.
    table.apply_edits();
    if ctx.backup_before_modify && path.exists() {
        octa::formats::backup_existing_file(path)?;
    }
    reader.write_file_schema_aware(path, &table, ctx.allow_schema_changes)?;

    let mut out = Map::new();
    out.insert(
        "columns_added".to_string(),
        Value::from(summary.columns_added),
    );
    out.insert("cells_set".to_string(), Value::from(summary.cells_set));
    out.insert(
        "rows_inserted".to_string(),
        Value::from(summary.rows_inserted),
    );
    out.insert(
        "rows_deleted".to_string(),
        Value::from(summary.rows_deleted),
    );
    out.insert(
        "columns_dropped".to_string(),
        Value::from(summary.columns_dropped),
    );
    out.insert(
        "path".to_string(),
        Value::String(path.display().to_string()),
    );
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("edit_table failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}

fn is_db_file(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("duckdb" | "ddb" | "sqlite" | "sqlite3" | "db" | "gpkg")
    )
}
