//! MCP tool: `resample_timeseries` - group rows into time buckets. Mirrors the
//! GUI Time series dialog and CLI `--resample`; all three build the SQL with
//! `octa::data::timeseries` and run it through `octa::sql::run_query`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::Value;

use octa::data::timeseries::{Interval, ResampleSpec, TimeAgg, build_resample_sql};
use octa::db::pushdown::dialect::is_time_type;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, server_table_to_json, source_from, table_to_json};

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

    let spec = ResampleSpec {
        time_col: p.time_col.clone(),
        value_cols: p.value_cols.clone(),
        interval,
        agg,
        group_by: p.group_by.clone(),
    };
    let source = source_from(&p.open_tab, &p.path, &p.table);
    if let Some((snap, src)) = ctx.db_tab(&source) {
        let columns = &snap.table.columns;
        // The same check the local path makes, before anything goes to the server.
        build_resample_sql(&spec, columns)?;
        let type_of = |name: &str| {
            columns
                .iter()
                .find(|c| c.name == name)
                .map(|c| c.data_type.as_str())
        };
        let values: Vec<&str> = spec.value_cols.iter().filter_map(|c| type_of(c)).collect();
        let sums = matches!(agg, TimeAgg::Sum | TimeAgg::Mean);
        // As in the GUI: a date in a text column and a sum over times stay local.
        if type_of(&spec.time_col).is_some_and(is_time_type)
            && !octa::db::pushdown::sums_time_locally(sums, &values)
        {
            let (table, more) = octa::db::pushdown::timeseries::resample(
                &mut *ctx.db_tab_connect(src)?,
                src,
                columns,
                &spec,
                ctx.db_tab_row_cap(p.limit, snap),
                &std::sync::atomic::AtomicBool::new(false),
            )?;
            return Ok(server_table_to_json(&table, more, ctx.cell_byte_cap));
        }
    }
    let dt = ctx.resolve(&source)?;
    let sql = build_resample_sql(&spec, &dt.columns)?;
    let outcome = octa::sql::run_query(&dt, &sql)
        .map_err(|e| anyhow::anyhow!("resample query failed: {e:#}"))?;

    let row_cap = ctx.resolve_row_cap(p.limit);
    Ok(table_to_json(&outcome.table, row_cap, ctx.cell_byte_cap))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("resample failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
