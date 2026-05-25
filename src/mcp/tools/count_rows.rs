//! MCP tool: `count_rows` — return the row count of a file.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::mcp::OctaMcpServer;

use super::read_with_registry;

// Tool description lives inline at the `#[tool]` site in `src/mcp/mod.rs`.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file.
    pub path: PathBuf,

    /// For multi-table sources, the specific table to count.
    #[serde(default)]
    pub table: Option<String>,

    /// Lift the streaming initial-load cap for this call so the count
    /// reflects every row in the file. Without this, the count is bounded
    /// by the cap and `initial_load_capped` flags `true`. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub async fn handle(_server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
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
    let row_count = dt.row_count();
    let initial_load_cap = if unlimited {
        usize::MAX
    } else {
        octa::formats::initial_load_rows()
    };
    let capped = !unlimited && row_count >= initial_load_cap;
    let mut out = Map::new();
    out.insert("row_count".to_string(), Value::from(row_count));
    out.insert("initial_load_capped".to_string(), Value::Bool(capped));
    out.insert(
        "initial_load_cap".to_string(),
        Value::from(initial_load_cap),
    );
    Ok(CallToolResult::success(vec![Content::text(
        Value::Object(out).to_string(),
    )]))
}
