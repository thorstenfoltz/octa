//! MCP tool: `cell_history` - the Git history of one cell. Read-only; the
//! same history as the GUI's cell menu **Cell history...** and
//! `octa --cell-history`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::cell_history::{cell_history, history_table, resolve_row};
use octa::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, table_to_json};

pub const DESCRIPTION: &str = "The Git history of one cell in a file inside a Git repository: \
the commits that changed it, newest first, with date, author, subject, the value, and `change` \
(changed, row_added, row_removed, column_added, column_removed, earliest). Name the row with \
`key` + `key_value` (column names and their values, followed through re-sorts) or `row` (1-based \
position). Renames are followed. Uncommitted changes on disk come first. `depth` commits are read \
(default 50). Returns `{history, positional_commits, unreadable, more}`.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to a file inside a Git repository.
    pub path: PathBuf,
    /// The column of the cell.
    pub column: String,
    /// Key column(s) that identify the row. Pair with `key_value`.
    #[serde(default)]
    pub key: Vec<String>,
    /// The row's value in each `key` column, in the same order.
    #[serde(default)]
    pub key_value: Vec<String>,
    /// 1-based row number, when there is no key.
    #[serde(default)]
    pub row: Option<usize>,
    /// How many commits to read, newest first. Default 50.
    #[serde(default)]
    pub depth: Option<usize>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let loaded = octa::git::history::versions_with_working_copy(
        &p.path,
        p.depth.unwrap_or(50),
        DEFAULT_MAX_DECOMPRESSED_BYTES,
    )?;
    let row = resolve_row(&loaded.versions, p.key.clone(), p.key_value.clone(), p.row)?;
    let h = cell_history(&loaded.versions, &row, &p.column);
    Ok(json!({
        "history": table_to_json(&history_table(&h), ctx.resolve_row_cap(None), ctx.cell_byte_cap),
        "positional_commits": h.positional.iter().map(|c| c.sha.clone()).collect::<Vec<_>>(),
        "unreadable": loaded.unreadable.iter().map(|(c, e)| json!({ "commit": c.sha, "error": e })).collect::<Vec<_>>(),
        "more": loaded.more,
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("cell_history failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
