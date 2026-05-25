//! MCP tool: `schema` — return only the column schema for a file.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;

use crate::mcp::OctaMcpServer;

use super::{read_with_registry, schema_to_json};

// Tool description lives inline at the `#[tool]` site in `src/mcp/mod.rs`.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file.
    pub path: PathBuf,

    /// For multi-table sources, the specific table to inspect.
    #[serde(default)]
    pub table: Option<String>,
}

pub async fn handle(_server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let path = p.path.clone();
    let table_name = p.table.clone();
    let dt = tokio::task::spawn_blocking(move || read_with_registry(&path, table_name.as_deref()))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("read failed: {e}"), None))?;
    let payload = schema_to_json(&dt);
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
