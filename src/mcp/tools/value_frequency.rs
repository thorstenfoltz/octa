//! MCP tool: `value_frequency` - per-column value counts (`value_counts`).

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::value_frequency::{BinningMode, compute_value_frequency};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Count how often each value appears in one column of a tabular file or open tab (a \
`value_counts()` equivalent). Returns `rows` (label + count, most frequent first) plus `nulls`, \
`total_non_null`, and `unique_count`. Set `bin: true` to group a numeric column into Sturges \
bins; use `top_n` to cap rows.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to scan.
    #[serde(default)]
    pub table: Option<String>,

    /// Name of the column to count values for.
    pub column: String,

    /// Return only the N most frequent values / bins. Omit for all.
    #[serde(default)]
    pub top_n: Option<usize>,

    /// Group numeric columns into Sturges bins instead of counting raw
    /// values. Ignored for non-numeric columns.
    #[serde(default)]
    pub bin: bool,

    /// Lift the streaming initial-load cap so the frequency counts include
    /// every row in the file. Without this, counts reflect at most the
    /// first `initial_load_rows` rows. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let binning = if p.bin {
        BinningMode::Sturges
    } else {
        BinningMode::None
    };
    let dt = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let col_idx = dt
        .columns
        .iter()
        .position(|c| c.name == p.column)
        .ok_or_else(|| anyhow::anyhow!("no such column: {}", p.column))?;
    let vf = compute_value_frequency(&dt, col_idx, p.top_n, binning)
        .ok_or_else(|| anyhow::anyhow!("could not compute value frequency for `{}`", p.column))?;

    let rows: Vec<Value> = vf
        .rows
        .iter()
        .map(|r| {
            let mut m = Map::new();
            m.insert("label".to_string(), Value::String(r.label.clone()));
            m.insert("count".to_string(), Value::from(r.count));
            Value::Object(m)
        })
        .collect();

    let mut out = Map::new();
    out.insert("column_name".to_string(), Value::String(vf.column_name));
    out.insert("binned".to_string(), Value::Bool(vf.binned));
    out.insert("nulls".to_string(), Value::from(vf.nulls));
    out.insert("total_non_null".to_string(), Value::from(vf.total_non_null));
    out.insert("unique_count".to_string(), Value::from(vf.unique_count));
    out.insert("rows".to_string(), Value::Array(rows));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("value_frequency failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
