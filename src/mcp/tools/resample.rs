//! MCP tool: `resample_timeseries` - group rows into time buckets. Mirrors the
//! GUI Time series dialog and CLI `--resample`; all three build the SQL with
//! `octa::data::timeseries` and run it through `octa::sql::run_query`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::Value;

use octa::data::timeseries::{Interval, ResampleSpec, TimeAgg, build_resample_sql};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Group rows into time buckets: one output row per interval of a \
timestamp column. Set `time_col` to the timestamp column, `value_cols` to the columns to \
aggregate, `interval` to minute/hour/day/week/month/quarter/year (default day) and `agg` to \
sum/mean/min/max/count/first/last (default sum). `group_by` produces one series per \
combination of those columns. The bucket lands in a column named `bucket`. Returns the \
resampled table as `{schema, rows, row_count, ...}`. Operates on a file `path` or an \
`open_tab`.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to read.
    #[serde(default)]
    pub table: Option<String>,

    /// The timestamp column to bucket.
    pub time_col: String,

    /// Columns aggregated within each bucket.
    pub value_cols: Vec<String>,

    /// Bucket size: minute/hour/day/week/month/quarter/year. Default `day`.
    #[serde(default)]
    pub interval: Option<String>,

    /// Aggregate: sum/mean/min/max/count/first/last. Default `sum`.
    #[serde(default)]
    pub agg: Option<String>,

    /// Extra grouping columns: one series per combination.
    #[serde(default)]
    pub group_by: Vec<String>,

    /// Cap how many rows the response carries (0 = unlimited).
    #[serde(default)]
    pub limit: Option<usize>,

    /// Lift the streaming initial-load cap so the resample sees every row.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let interval = match p.interval.as_deref() {
        None => Interval::Day,
        Some(s) => Interval::parse(s).ok_or_else(|| {
            anyhow::anyhow!("unknown interval `{s}` (minute/hour/day/week/month/quarter/year)")
        })?,
    };
    let agg = match p.agg.as_deref() {
        None => TimeAgg::Sum,
        Some(s) => TimeAgg::parse(s).ok_or_else(|| {
            anyhow::anyhow!("unknown aggregate `{s}` (sum/mean/min/max/count/first/last)")
        })?,
    };

    let dt = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let cols: Vec<String> = dt.columns.iter().map(|c| c.name.clone()).collect();
    let spec = ResampleSpec {
        time_col: p.time_col.clone(),
        value_cols: p.value_cols.clone(),
        interval,
        agg,
        group_by: p.group_by.clone(),
    };
    let sql = build_resample_sql(&spec, &cols)?;
    let outcome = octa::sql::run_query(&dt, &sql)
        .map_err(|e| anyhow::anyhow!("resample query failed: {e}"))?;

    let row_cap = ctx.resolve_row_cap(p.limit);
    Ok(table_to_json(&outcome.table, row_cap, ctx.cell_byte_cap))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("resample failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
