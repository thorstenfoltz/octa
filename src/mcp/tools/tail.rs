//! MCP tool: `tail` - load a file and return its last N rows.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;

use crate::mcp::OctaMcpServer;

use super::{read_with_registry, table_to_json};

// Tool description lives inline at the `#[tool]` site in `src/mcp/mod.rs`.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Absolute or working-directory-relative path to the file.
    pub path: PathBuf,

    /// Number of trailing rows to return. Default is the server's configured
    /// limit. Pass 0 for unlimited (returns the whole loaded window).
    #[serde(default)]
    pub limit: Option<usize>,

    /// For multi-table sources, the specific table to load.
    #[serde(default)]
    pub table: Option<String>,

    /// Lift the streaming initial-load cap so the true end of a very large
    /// file is reachable. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let row_cap = server.resolve_row_cap(p.limit);
    let cell_cap = server.cell_byte_cap;
    let path = p.path.clone();
    let table_name = p.table.clone();
    let unlimited = p.unlimited;

    let mut dt = tokio::task::spawn_blocking(move || {
        let _g = unlimited.then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
        read_with_registry(&path, table_name.as_deref())
    })
    .await
    .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
    .map_err(|e| McpError::invalid_params(format!("read failed: {e}"), None))?;

    // Keep the last `row_cap` rows (None / 0 = keep all).
    if let Some(n) = row_cap {
        let len = dt.row_count();
        if n > 0 && len > n {
            dt.rows.drain(0..len - n);
            dt.total_rows = None;
        }
    }

    // Rows are already sliced to the tail; don't re-cap in table_to_json.
    let payload = table_to_json(&dt, None, cell_cap);
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
