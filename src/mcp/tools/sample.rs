//! MCP tool: `sample` - load a file and return a random N-row sample.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use octa::data::sample::sample_table;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Return a random N-row sample (without replacement, original order preserved) of a tabular \
file or open tab, same response shape as `read_table`. `limit` sets the sample size (0 = every \
row). `seed` makes it reproducible.";

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

    /// Sample size. Default is the server's configured limit. Pass 0 for
    /// "every row" (no sampling).
    #[serde(default)]
    pub limit: Option<usize>,

    /// For multi-table sources, the specific table to load.
    #[serde(default)]
    pub table: Option<String>,

    /// Seed for reproducible sampling. Default 0.
    #[serde(default)]
    pub seed: u64,

    /// Lift the streaming initial-load cap so sampling sees every row on disk.
    /// Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let dt = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    // None (unlimited) means "every row"; otherwise sample `n`.
    let n = ctx
        .resolve_row_cap(p.limit)
        .unwrap_or_else(|| dt.row_count());
    let sampled = sample_table(&dt, n, p.seed);
    // Rows are already the sample; serialise all of them.
    Ok(table_to_json(&sampled, None, ctx.cell_byte_cap))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("sample failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
