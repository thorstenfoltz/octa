//! MCP tool: `seasonality` - split one column over time into trend,
//! repeating pattern and rest. Read-only; the same split as the chart's
//! Seasonality to table and `octa --seasonality`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::forecast::series_for;
use octa::data::seasonality::{decompose, decomposition_table};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Split a column over time into its trend, its repeating pattern \
and the rest (classical additive decomposition), to tell a real spike from one that comes every \
season. The season is read from the date spacing (24 hourly, 7 daily, 52 weekly, 12 monthly, 4 \
quarterly; `season` overrides it) and needs two full seasons of history. `x` is a date, date-time \
or number column with evenly spaced values (bucket uneven data first with `resample_timeseries`), \
`y` the numbers. Returns `{season, strength, decomposition}`: `strength` is the share of the \
movement around the trend the pattern explains (0 to 1), `decomposition` has `x, value, trend, \
seasonal, rest` per point, `trend` and `rest` null for the first and last half season.";

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
    /// The time column: dates, date-times or numbers, evenly spaced.
    pub x: String,
    /// The column of numbers to split.
    pub y: String,
    /// Season length in points; omit to read it from the spacing.
    #[serde(default)]
    pub season: Option<usize>,
    /// At most this many points in the answer; 0 for all.
    #[serde(default)]
    pub limit: Option<usize>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let table = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let (series, kind) = series_for(&table, table.column_index(&p.x)?, table.column_index(&p.y)?)
        .map_err(|e| anyhow::anyhow!(e))?;
    let d = decompose(&series.points, kind, p.season).map_err(|e| anyhow::anyhow!(e.message()))?;
    Ok(json!({
        "season": d.season,
        "strength": (d.strength * 1000.0).round() / 1000.0,
        "decomposition": table_to_json(&decomposition_table(&d, kind), ctx.resolve_row_cap(p.limit), ctx.cell_byte_cap),
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("seasonality failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
