//! MCP tool: `find_duplicates` - rows sharing identical key-column values.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::duplicates::find_duplicate_rows;
use octa::data::{CellValue, DataTable};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Find duplicate rows in a tabular file or open tab. `key_columns` lists the columns whose \
combined value forms the duplicate key; every row sharing its key with another is returned. The \
response carries `duplicate_row_count` and `result` (schema + duplicate rows). `limit` caps rows \
(0 = unlimited).";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to scan.
    #[serde(default)]
    pub table: Option<String>,

    /// Column names whose combined value forms the duplicate key. Must be
    /// non-empty; every name must exist in the file.
    pub key_columns: Vec<String>,

    /// Maximum duplicate rows to return. Default is the server's
    /// configured limit. Pass 0 for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Lift the streaming initial-load cap so duplicate detection scans
    /// every row in the file. Without this, only the first
    /// `initial_load_rows` rows are considered. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    if p.key_columns.is_empty() {
        anyhow::bail!("key_columns must not be empty");
    }
    let dt = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let mut key_idx = Vec::with_capacity(p.key_columns.len());
    for name in &p.key_columns {
        let idx = dt
            .columns
            .iter()
            .position(|c| &c.name == name)
            .ok_or_else(|| anyhow::anyhow!("no such column: {name}"))?;
        key_idx.push(idx);
    }
    let dup_rows = find_duplicate_rows(&dt, &key_idx);
    // Materialise the duplicate rows into a standalone table so the shared
    // `table_to_json` can serialise + cap them.
    let mut sub = DataTable::empty();
    sub.columns = dt.columns.clone();
    sub.rows = dup_rows
        .iter()
        .map(|&r| {
            (0..dt.col_count())
                .map(|c| dt.get(r, c).cloned().unwrap_or(CellValue::Null))
                .collect()
        })
        .collect();

    let result = table_to_json(&sub, ctx.resolve_row_cap(p.limit), ctx.cell_byte_cap);
    let mut out = Map::new();
    out.insert(
        "key_columns".to_string(),
        Value::Array(p.key_columns.iter().cloned().map(Value::String).collect()),
    );
    out.insert(
        "duplicate_row_count".to_string(),
        Value::from(dup_rows.len()),
    );
    out.insert("result".to_string(), result);
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("find_duplicates failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
