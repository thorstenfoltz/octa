//! MCP tool: `tail` - load a file and return its last N rows.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Return the LAST N rows (the tail) of a tabular file or open tab, same response shape as \
`read_table`. `limit` sets N (0 = the whole loaded window).";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Absolute or working-directory-relative path to the file. Omit when
    /// `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

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

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let mut dt = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;

    // Keep the last `row_cap` rows (None / 0 = keep all).
    if let Some(n) = ctx.resolve_row_cap(p.limit) {
        let len = dt.row_count();
        if n > 0 && len > n {
            dt.rows.drain(0..len - n);
            dt.total_rows = None;
        }
    }

    // Rows are already sliced to the tail; don't re-cap in table_to_json.
    Ok(table_to_json(&dt, None, ctx.cell_byte_cap))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("tail failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
