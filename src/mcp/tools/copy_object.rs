//! MCP tool: `copy_object` - copy one cloud object, or a whole prefix, to
//! another location. Write tool.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "Copy a cloud object, or every object under a prefix, to another \
location. `from` and `to` are cloud URLs (`s3://bucket/key`, `az://container/key`, \
`gs://bucket/key`). A `from` ending in `/` copies the whole folder recursively and recreates its \
shape under `to`. Source and destination may be different buckets, different accounts, or \
different providers (S3 -> GCS works); a copy inside one bucket is done server-side, a copy \
across buckets is streamed in blocks so object size does not drive memory. The source is never \
modified. Refuses to copy a folder into itself, and refuses more than 10,000 objects in one call.";

/// Shared shape for the copy/move tools.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Source cloud URL. Ending in `/` means "this folder, recursively".
    pub from: String,
    /// Destination cloud URL. For a folder source this is the target folder.
    pub to: String,
}

/// Resolve both ends and run `op`. Shared by `copy_object` and `move_object`.
pub(super) fn transfer(
    ctx: &ToolContext,
    from: &str,
    to: &str,
    op: fn(
        &dyn octa::cloud::CloudProvider,
        &str,
        &dyn octa::cloud::CloudProvider,
        &str,
        bool,
    ) -> anyhow::Result<octa::cloud::TransferReport>,
) -> anyhow::Result<Value> {
    if from.trim().is_empty() || to.trim().is_empty() {
        anyhow::bail!("`from` and `to` are both required (cloud URLs)");
    }
    // The source only needs read access; the destination is what gets written.
    let (src, src_loc) = ctx.cloud_provider_for(from)?;
    let (dst, dst_loc) = ctx.cloud_provider_for_write(to)?;
    // "Same store" decides between a server-side copy and a streamed transfer.
    // Same scheme + same bucket is the honest test we can make from here.
    let same_store = src_loc.kind == dst_loc.kind && src_loc.bucket == dst_loc.bucket;

    let report = op(
        src.as_ref(),
        &src_loc.key,
        dst.as_ref(),
        &dst_loc.key,
        same_store,
    )?;
    Ok(json!({
        "from": from,
        "to": to,
        "objects": report.objects,
        "bytes": report.bytes,
        "server_side": report.server_side,
    }))
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    transfer(ctx, &p.from, &p.to, octa::cloud::ops::copy)
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("copy_object failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
