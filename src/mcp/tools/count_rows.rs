//! MCP tool: `count_rows` - return the row count of a file.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Count the rows in a tabular file or open tab. For streaming formats the count is bounded by \
Octa's 5,000,000-row initial-load cap; `initial_load_capped` flags when it may be short. Pass \
`unlimited: true` for the true total. A large Parquet, CSV or JSON file is counted exactly \
straight from the file without reading it, and the response then carries `streamed: true`.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to count.
    #[serde(default)]
    pub table: Option<String>,

    /// Lift the streaming initial-load cap for this call so the count
    /// reflects every row in the file. Without this, the count is bounded
    /// by the cap and `initial_load_capped` flags `true`. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let source = source_from(&p.open_tab, &p.path, &p.table);
    // A big file that DuckDB can scan answers this exactly, from its metadata,
    // without reading a row. That is the difference between a count that is
    // silently short and one that is right, and it costs nothing.
    if let Some(scan) = ctx.scan_for(&source) {
        let mut out = Map::new();
        out.insert("row_count".to_string(), Value::from(scan.row_count()));
        out.insert("initial_load_capped".to_string(), Value::Bool(false));
        out.insert("streamed".to_string(), Value::Bool(true));
        return Ok(Value::Object(out));
    }
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let dt = ctx.resolve(&source)?;
    let row_count = dt.row_count();
    let initial_load_cap = if p.unlimited {
        usize::MAX
    } else {
        octa::formats::initial_load_rows()
    };
    let capped = !p.unlimited && row_count >= initial_load_cap;
    let mut out = Map::new();
    out.insert("row_count".to_string(), Value::from(row_count));
    out.insert("initial_load_capped".to_string(), Value::Bool(capped));
    out.insert(
        "initial_load_cap".to_string(),
        Value::from(initial_load_cap),
    );
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("count_rows failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
