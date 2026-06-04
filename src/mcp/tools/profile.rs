//! MCP tool: `profile` - per-column statistics via DuckDB `SUMMARIZE`.
//!
//! The table is registered as the DuckDB temp table `data` (types are
//! preserved by `octa::sql::register_table`, so numeric columns get real
//! numeric stats) and `SUMMARIZE data` is run. The result - one row per
//! source column - is reshaped into an object keyed by SUMMARIZE's own
//! column names (`min`, `max`, `avg`, `std`, `q25`/`q50`/`q75`,
//! `approx_unique`, `count`, `null_percentage`, ...).

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::CellValue;
use octa::sql::run_query;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Profile a tabular file or open tab: per-column statistics via DuckDB SUMMARIZE - type, min, \
max, approximate distinct count, mean, standard deviation, quartiles, row count, and null \
percentage. The fastest way to understand an unfamiliar dataset.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources (SQLite, DuckDB, GeoPackage), the table to
    /// profile. Omit for single-table formats.
    #[serde(default)]
    pub table: Option<String>,

    /// Lift the streaming initial-load cap so SUMMARIZE sees every row.
    /// Without this, the per-column stats reflect at most the first
    /// `initial_load_rows` rows. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let dt = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    if dt.col_count() == 0 {
        anyhow::bail!("file has no columns to profile");
    }
    let summary = run_query(&dt, "SUMMARIZE data")?.table;

    // SUMMARIZE yields one row per source column; reshape each row into
    // an object keyed by SUMMARIZE's own column names.
    let keys: Vec<String> = summary.columns.iter().map(|c| c.name.clone()).collect();
    let mut columns: Vec<Value> = Vec::with_capacity(summary.row_count());
    for r in 0..summary.row_count() {
        let mut obj = Map::new();
        for (c, key) in keys.iter().enumerate() {
            let cell = summary.get(r, c).unwrap_or(&CellValue::Null);
            // cell cap 0 = no truncation; profile values are small.
            obj.insert(key.clone(), super::cell_to_json(cell, 0).0);
        }
        columns.push(Value::Object(obj));
    }

    let mut out = Map::new();
    out.insert("column_count".to_string(), Value::from(columns.len()));
    out.insert("columns".to_string(), Value::Array(columns));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("profile failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
