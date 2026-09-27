//! MCP tool: `forecast` - Holt-Winters forecast of one column over time.
//! Read-only; the same model as the chart's Forecast and `octa --forecast`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::forecast::{forecast, forecast_table, series_for};
use octa::data::timeline::column_named;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Forecast a column over time with Holt-Winters (trend plus a \
season read from the date spacing: 24 hourly, 7 daily, 52 weekly, 12 monthly, 4 quarterly; \
`season` overrides it). `x` is a date, date-time or number column with evenly spaced values \
(bucket uneven data first with `resample_timeseries`), `y` the numbers to forecast, `periods` how \
far ahead (default 12). Returns `{season, forecast}`; `forecast` has `x, forecast, lo80, hi80, \
lo95, hi95` per period.";

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
    /// The column of numbers to forecast.
    pub y: String,
    /// How many periods ahead. Default 12.
    #[serde(default)]
    pub periods: Option<usize>,
    /// Season length in points; omit to read it from the spacing.
    #[serde(default)]
    pub season: Option<usize>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let table = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let (series, kind) = series_for(
        &table,
        column_named(&table, &p.x)?,
        column_named(&table, &p.y)?,
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    let f = forecast(&series.points, kind, p.periods.unwrap_or(12), p.season)
        .map_err(|e| anyhow::anyhow!(octa::i18n::t(e.i18n_key())))?;
    Ok(json!({
        "season": f.season,
        "forecast": table_to_json(&forecast_table(&f, kind), ctx.resolve_row_cap(None), ctx.cell_byte_cap),
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("forecast failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
