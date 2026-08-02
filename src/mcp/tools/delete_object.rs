//! MCP tool: `delete_object` - delete one cloud object, or every object under
//! a prefix. Write tool, and the only irreversible one in this group.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "Delete a cloud object, or every object under a prefix. `url` is a \
cloud URL (`s3://bucket/key`, `az://container/key`, `gs://bucket/key`); ending it in `/` deletes \
the whole folder recursively, which is why that form additionally requires `recursive: true`. \
**This cannot be undone** unless the bucket has versioning on. Deleting a key that does not exist \
is not an error. Refuses more than 10,000 objects in one call. Prefer `move_object` to an archive \
prefix when the data might still be wanted.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Cloud URL to delete. Ending in `/` means the whole folder.
    pub url: String,
    /// Required confirmation when `url` names a folder. Guards against a
    /// trailing slash turning a one-object delete into a recursive one.
    #[serde(default)]
    pub recursive: Option<bool>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.url.trim().is_empty() {
        anyhow::bail!("`url` is required (e.g. s3://bucket/old.csv)");
    }
    let (provider, loc) = ctx.cloud_provider_for_write(&p.url)?;
    if octa::cloud::is_prefix(&loc.key) && p.recursive != Some(true) {
        anyhow::bail!(
            "{} names a folder; pass recursive: true to delete everything under it",
            p.url
        );
    }
    let report = octa::cloud::ops::delete(provider.as_ref(), &loc.key)?;
    Ok(json!({
        "url": p.url,
        "deleted": report.objects,
        "bytes": report.bytes,
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("delete_object failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
