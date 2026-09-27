//! MCP tool: `value_shapes` - what a column's values look like, with the
//! specifics taken out. Read-only; the same shapes the header funnel's
//! Shapes switch and the Quality Report show.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::shapes::shape_frequency;
use octa::data::timeline::column_named;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "What a column's values look like, with the specifics taken out: \
digits become 9, capital letters A, other letters a, punctuation stays (`D-80331` is `A-99999`). \
Returns `{column, empty, shape_count, shapes: [{shape, count, example}]}`, most common shape \
first. A column where one shape dominates and a few values differ usually holds typos or a second \
format. Values longer than 24 characters shorten runs as `a(12)`.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,
    /// An open GUI tab (name, or `@active`) instead of a file.
    #[serde(default)]
    pub open_tab: Option<String>,
    /// For multi-table sources, the table to read.
    #[serde(default)]
    pub table: Option<String>,
    /// The column to shape.
    pub column: String,
    /// Maximum shapes to return. Pass 0 for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let table = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let col = column_named(&table, &p.column)?;
    let shapes = shape_frequency(&table, col);
    let cap = ctx.resolve_row_cap(p.limit).unwrap_or(usize::MAX);
    Ok(json!({
        "column": p.column,
        "empty": shapes.empty,
        "shape_count": shapes.shapes.len(),
        "shapes": shapes.shapes.iter().take(cap).map(|s| json!({
            "shape": s.shape, "count": s.count, "example": s.example
        })).collect::<Vec<_>>(),
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("value_shapes failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
