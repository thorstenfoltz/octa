//! MCP tool: `join_tables` - join N tabular sources left-to-right on shared
//! key columns.
//!
//! Sources are resolved through the shared `ToolContext` (a file path or an
//! open GUI tab). The join is executed via `octa::data::join::join_tables`
//! which builds a DuckDB SQL `USING (keys)` query over the named tables.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::Value;

use octa::data::join::{JoinType, join_tables};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Join N tabular sources left-to-right on shared key columns. Each entry in \
     `sources` has a `path` (file) or `open_tab` (GUI tab name / `@active`), plus an \
     optional `table` for multi-table sources. Sources are assigned names `t0`, `t1`, \
     ... and joined in order using a SQL `USING (on)` clause. `how` sets the join \
     type: `left` (default), `inner`, `right`, `full`, `semi` (left rows with a \
     partner, left columns only), `anti` (left rows without one) or `asof` (the last \
     key is matched to the nearest earlier value, e.g. a time). Duplicate key columns are \
     collapsed into one in the output. Requires at least two sources and at least one \
     key column in `on`.";

/// One entry in the `sources` list: a file path or an open GUI tab.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SourceParam {
    /// Path to a file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Address an open GUI tab by name or handle (e.g. `@active`). Omit when
    /// `path` is set.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources (SQLite, DuckDB), the inner table name.
    #[serde(default)]
    pub table: Option<String>,
}

/// Parameters for `join_tables`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Two or more sources to join. Each entry has a `path` (file) or
    /// `open_tab` (GUI tab name / `@active`), plus an optional inner `table`
    /// for multi-table sources. At least two sources are required.
    pub sources: Vec<SourceParam>,

    /// Key column name(s) to join on. The same column name must exist in all
    /// sources. At least one key is required.
    pub on: Vec<String>,

    /// Join type: `left` (default), `inner`, `right`, `full`, `semi` (left
    /// rows with a partner), `anti` (left rows without one) or `asof` (the
    /// last key is matched to the nearest earlier value).
    #[serde(default)]
    pub how: Option<String>,

    /// Maximum rows to return in the response. Default is the server's
    /// configured limit. Pass `0` for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Lift the streaming initial-load cap so every row in all source files
    /// is read from disk. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.sources.len() < 2 {
        anyhow::bail!("join_tables needs at least two sources");
    }
    if p.on.is_empty() {
        anyhow::bail!("join_tables needs at least one key column in `on`");
    }

    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let snaps: Vec<octa::data::DataTable> = p
        .sources
        .iter()
        .map(|s| ctx.resolve(&source_from(&s.open_tab, &s.path, &s.table)))
        .collect::<anyhow::Result<_>>()?;

    // Build owned name strings first so the refs below can borrow them.
    let names: Vec<String> = (0..snaps.len()).map(|i| format!("t{i}")).collect();
    let named: Vec<(&str, &octa::data::DataTable)> =
        names.iter().map(String::as_str).zip(snaps.iter()).collect();

    let how = JoinType::parse(p.how.as_deref().unwrap_or("left"))?;
    let out = join_tables(&named, &p.on, how)?;

    let row_cap = ctx.resolve_row_cap(p.limit);
    let cell_cap = ctx.cell_byte_cap;
    Ok(table_to_json(&out, row_cap, cell_cap))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("join_tables failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
