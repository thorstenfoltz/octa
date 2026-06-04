//! MCP tool: `unique_columns` - find columns (or small combinations)
//! whose values are unique across a tabular file. Useful for
//! primary-key reconnaissance on undocumented sources.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::unique_columns::{UniqueAnalysis, find_unique_columns};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Find columns (or small combinations) whose values are unique across a tabular file or open \
tab - primary-key reconnaissance. Returns `total_rows`, `single` (per-column results), and \
`combos` (when `max_combo_size > 1`, clamped to [1,3]).";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to inspect.
    #[serde(default)]
    pub table: Option<String>,

    /// Maximum combo size to test (1 = single columns only, 2 = +
    /// pairs, 3 = + triples). Clamped to `[1, 3]`. Default 1.
    #[serde(default)]
    pub max_combo_size: Option<usize>,

    /// Lift the streaming initial-load cap so every row in the file
    /// is considered. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let dt = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let analysis = find_unique_columns(&dt, p.max_combo_size.unwrap_or(1));
    Ok(analysis_to_json(&analysis))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("unique_columns failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}

fn analysis_to_json(a: &UniqueAnalysis) -> Value {
    let single: Vec<Value> = a
        .single
        .iter()
        .map(|r| {
            let mut m = Map::new();
            m.insert("column".to_string(), Value::String(r.column.clone()));
            m.insert("distinct_count".to_string(), Value::from(r.distinct_count));
            m.insert("null_count".to_string(), Value::from(r.null_count));
            m.insert("is_unique".to_string(), Value::Bool(r.is_unique));
            Value::Object(m)
        })
        .collect();
    let combos: Vec<Value> = a
        .combos
        .iter()
        .map(|c| {
            let mut m = Map::new();
            let names: Vec<Value> = c.columns.iter().map(|n| Value::String(n.clone())).collect();
            m.insert("columns".to_string(), Value::Array(names));
            m.insert("distinct_count".to_string(), Value::from(c.distinct_count));
            m.insert("is_unique".to_string(), Value::Bool(c.is_unique));
            Value::Object(m)
        })
        .collect();
    let mut out = Map::new();
    out.insert("total_rows".to_string(), Value::from(a.total_rows));
    out.insert("single".to_string(), Value::Array(single));
    out.insert("combos".to_string(), Value::Array(combos));
    Value::Object(out)
}
