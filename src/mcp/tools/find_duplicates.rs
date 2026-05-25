//! MCP tool: `find_duplicates` — rows sharing identical key-column values.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::duplicates::find_duplicate_rows;
use octa::data::{CellValue, DataTable};

use crate::mcp::OctaMcpServer;

use super::{read_with_registry, table_to_json};

// Tool description lives inline at the `#[tool]` site in `src/mcp/mod.rs`.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file.
    pub path: PathBuf,

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

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let row_cap = server.resolve_row_cap(p.limit);
    let cell_cap = server.cell_byte_cap;
    let path = p.path.clone();
    let table_name = p.table.clone();
    let key_columns = p.key_columns.clone();
    let unlimited = p.unlimited;

    let (sub, dup_count) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let _g = unlimited.then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
        if key_columns.is_empty() {
            anyhow::bail!("key_columns must not be empty");
        }
        let dt = read_with_registry(&path, table_name.as_deref())?;
        let mut key_idx = Vec::with_capacity(key_columns.len());
        for name in &key_columns {
            let idx = dt
                .columns
                .iter()
                .position(|c| &c.name == name)
                .ok_or_else(|| anyhow::anyhow!("no such column: {name}"))?;
            key_idx.push(idx);
        }
        let dup_rows = find_duplicate_rows(&dt, &key_idx);
        // Materialise the duplicate rows into a standalone table so the
        // shared `table_to_json` can serialise + cap them.
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
        Ok((sub, dup_rows.len()))
    })
    .await
    .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
    .map_err(|e| McpError::invalid_params(format!("find_duplicates failed: {e}"), None))?;

    let result = table_to_json(&sub, row_cap, cell_cap);
    let mut out = Map::new();
    out.insert(
        "key_columns".to_string(),
        Value::Array(p.key_columns.into_iter().map(Value::String).collect()),
    );
    out.insert("duplicate_row_count".to_string(), Value::from(dup_count));
    out.insert("result".to_string(), result);
    Ok(CallToolResult::success(vec![Content::text(
        Value::Object(out).to_string(),
    )]))
}
