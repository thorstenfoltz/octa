//! MCP tool: `read_table` — load a file and return schema + rows.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;

use crate::mcp::OctaMcpServer;

use super::{read_with_registry, table_to_json};

// Tool description is declared inline at the `#[tool(description = ...)]`
// site in `src/mcp/mod.rs` because rmcp's macro only accepts a string
// literal there. Keep the two in sync when editing.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Absolute or working-directory-relative path to the file.
    pub path: PathBuf,

    /// Maximum rows to return. Default is the server's configured limit (1000
    /// unless changed via Octa's Settings → MCP). Pass 0 for unlimited.
    /// Note: this only slices the *response*. The file is still read with
    /// the streaming initial-load cap (5 M rows by default). Set `unlimited`
    /// to lift the file-loader cap as well.
    #[serde(default)]
    pub limit: Option<usize>,

    /// For multi-table sources (SQLite, DuckDB, GeoPackage), the specific
    /// table to load. Omit for single-table formats.
    #[serde(default)]
    pub table: Option<String>,

    /// Lift the streaming initial-load cap for this call so every row in the
    /// file is read from disk. Combine with `limit: 0` to actually return
    /// every row in the response. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let row_cap = server.resolve_row_cap(p.limit);
    let cell_cap = server.cell_byte_cap;
    let path = p.path.clone();
    let table_name = p.table.clone();
    let unlimited = p.unlimited;
    let dt = tokio::task::spawn_blocking(move || {
        let _g = unlimited.then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
        read_with_registry(&path, table_name.as_deref())
    })
    .await
    .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
    .map_err(|e| McpError::invalid_params(format!("read failed: {e}"), None))?;
    let payload = table_to_json(&dt, row_cap, cell_cap);
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
