//! MCP tool: `move_object` - copy a cloud object (or prefix) to a new
//! location, then delete the source. Write tool.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::Value;

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "Move a cloud object, or every object under a prefix, to another \
location: a copy followed by deleting the source. `from` and `to` are cloud URLs; a `from` ending \
in `/` moves the whole folder recursively. Works across buckets, accounts and providers. Object \
stores have no rename, so this is genuinely copy-then-delete: the delete only runs after every \
copy succeeded, so an interrupted move leaves the source intact. Refuses more than 10,000 objects \
in one call. Use `copy_object` when the source should survive.";

/// Same shape as `copy_object`, so a caller can switch between them by name.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Source cloud URL. Ending in `/` means "this folder, recursively".
    pub from: String,
    /// Destination cloud URL. For a folder source this is the target folder.
    pub to: String,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    super::copy_object::transfer(ctx, &p.from, &p.to, octa::cloud::ops::move_)
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("move_object failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
