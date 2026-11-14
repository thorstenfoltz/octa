//! MCP tool: `rolling_window` - add a rolling aggregate over the previous N
//! rows. Mirrors the GUI Time series dialog and CLI `--rolling`; all three
//! build the SQL with `octa::data::timeseries` and run it through
//! `octa::sql::run_query`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::Value;

use octa::data::timeseries::{RollingSpec, TimeAgg, build_rolling_sql};
use octa::db::pushdown::timeseries::{rolling, rolling_supported};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, server_table_to_json, source_from, table_to_json};

pub const DESCRIPTION: &str = "Add a rolling aggregate column over the previous N rows. Set \
`order_col` to the column that orders the frame (required: a rolling aggregate over \
unordered rows is meaningless), `value_col` to the column aggregated, `window` to the number \
of rows in the frame including the current one, and `agg` to \
sum/mean/min/max/count/first/last (default mean). `partition_by` restarts the frame for each \
combination of those columns. The result is every source column plus \
`<value_col>_rolling_<window>`. Operates on a file `path` or an `open_tab`.";

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

    /// The column that orders the frame. Required: a rolling aggregate over
    /// unordered rows is meaningless.
    pub order_col: String,

    /// The column aggregated over the frame.
    pub value_col: String,

    /// Rows in the frame, including the current one. At least 1.
    pub window: usize,

    /// Aggregate: sum/mean/min/max/count/first/last. Default `mean`.
    #[serde(default)]
    pub agg: Option<String>,

    /// Columns that restart the frame: the window never spans two groups.
    #[serde(default)]
    pub partition_by: Vec<String>,

    /// Cap how many rows the response carries (0 = unlimited).
    #[serde(default)]
    pub limit: Option<usize>,

    /// Lift the streaming initial-load cap so the window sees every row.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let agg = match p.agg.as_deref() {
        None => TimeAgg::Mean,
        Some(s) => TimeAgg::parse(s).ok_or_else(|| {
            anyhow::anyhow!("unknown aggregate `{s}` (sum/mean/min/max/count/first/last)")
        })?,
    };

    let spec = RollingSpec {
        order_col: p.order_col.clone(),
        value_col: p.value_col.clone(),
        window: p.window,
        agg,
        partition_by: p.partition_by.clone(),
    };
    let source = source_from(&p.open_tab, &p.path, &p.table);
    if let Some((snap, src)) = ctx.db_tab(&source) {
        let columns = &snap.table.columns;
        // The same check the local path makes, before anything goes to the server.
        build_rolling_sql(&spec, columns)?;
        let value = columns.iter().find(|c| c.name == spec.value_col);
        let value = value.map_or("", |c| c.data_type.as_str());
        let sums = matches!(agg, TimeAgg::Sum | TimeAgg::Mean);
        if rolling_supported(src.engine(), agg)
            && !octa::db::pushdown::sums_time_locally(sums, &[value])
        {
            // ponytail: no key to break a tie in time, unlike the GUI, which has the
            // tab's row key. Rows of equal time may swap between runs.
            let (table, more) = rolling(
                &mut *ctx.db_tab_connect(src)?,
                src,
                columns,
                &spec,
                &[],
                ctx.db_tab_row_cap(p.limit, snap),
                &std::sync::atomic::AtomicBool::new(false),
            )?;
            return Ok(server_table_to_json(&table, more, ctx.cell_byte_cap));
        }
    }
    let dt = ctx.resolve(&source)?;
    let sql = build_rolling_sql(&spec, &dt.columns)?;
    let outcome = octa::sql::run_query(&dt, &sql)
        .map_err(|e| anyhow::anyhow!("rolling window query failed: {e:#}"))?;

    let row_cap = ctx.resolve_row_cap(p.limit);
    Ok(table_to_json(&outcome.table, row_cap, ctx.cell_byte_cap))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("rolling window failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
