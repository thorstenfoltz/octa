//! MCP tool: `edit_table` - edit an existing file in place. Set individual
//! cells, insert rows, and delete rows, then save back through the same
//! reader the file was loaded with. Database sources (SQLite / DuckDB) keep
//! their diff-based save semantics: only the rows that actually changed are
//! UPDATE/INSERT/DELETE-d, because the reader snapshots row identity into
//! `db_meta` on load and `apply_edits` leaves that snapshot intact.
//!
//! Column changes (rename / add / drop) are not supported here - this tool
//! only mutates cell values and row counts.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::formats::FormatRegistry;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, cell_from_json, read_with_registry};

pub const DESCRIPTION: &str = "Edit an existing tabular file in place via its native writer. `set` updates cells, \
`insert_rows` adds rows, `delete_rows` removes rows by index. SQLite / DuckDB keep diff-based \
save (only changed rows are written). Column changes are not supported. Use `table` for \
multi-table sources.";

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
    let col_count = table.col_count();

    // Deletes first, highest index first so the lower indices stay valid.
    let mut delete_sorted = p.delete_rows.clone();
    delete_sorted.sort_unstable();
    delete_sorted.dedup();
    for &idx in delete_sorted.iter().rev() {
        if idx >= table.row_count() {
            anyhow::bail!(
                "delete_rows: row {idx} is out of range (table has {} row(s))",
                table.row_count()
            );
        }
        table.delete_row(idx);
    }
    let rows_deleted = delete_sorted.len();

    // Inserts next.
    let mut rows_inserted = 0usize;
    for ins in &p.insert_rows {
        if ins.values.len() != col_count {
            anyhow::bail!(
                "insert_rows: row has {} cell(s) but the table has {col_count} column(s)",
                ins.values.len()
            );
        }
        let at = ins.at.unwrap_or_else(|| table.row_count());
        if at > table.row_count() {
            anyhow::bail!(
                "insert_rows: index {at} is out of range (table has {} row(s))",
                table.row_count()
            );
        }
        table.insert_row(at);
        for (c, v) in ins.values.iter().enumerate() {
            let cell = cell_from_json(v, &table.columns[c].data_type);
            table.set(at, c, cell);
        }
        rows_inserted += 1;
    }

    // Cell edits last, against the post-insert/delete row layout.
    let mut cells_set = 0usize;
    for edit in &p.set {
        let col = match &edit.col {
            ColRef::Index(i) => *i,
            ColRef::Name(name) => table
                .columns
                .iter()
                .position(|c| &c.name == name)
                .ok_or_else(|| anyhow::anyhow!("set: no column named \"{name}\""))?,
        };
        if col >= col_count {
            anyhow::bail!("set: column {col} is out of range (table has {col_count} column(s))");
        }
        if edit.row >= table.row_count() {
            anyhow::bail!(
                "set: row {} is out of range (table has {} row(s))",
                edit.row,
                table.row_count()
            );
        }
        let cell = cell_from_json(&edit.value, &table.columns[col].data_type);
        table.set(edit.row, col, cell);
        cells_set += 1;
    }

    // Fold edits into rows, then write through the native writer (the GUI save
    // sequence). DB diff-save keys off db_meta.original, which apply_edits
    // leaves untouched.
    table.apply_edits();
    reader.write_file(path, &table)?;

    let mut out = Map::new();
    out.insert("cells_set".to_string(), Value::from(cells_set));
    out.insert("rows_inserted".to_string(), Value::from(rows_inserted));
    out.insert("rows_deleted".to_string(), Value::from(rows_deleted));
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
