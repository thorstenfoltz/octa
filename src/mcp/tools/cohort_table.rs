//! MCP tool: `cohort_table` - who first appeared in which period, and how
//! many came back (or a column's sum) in each later period. Read-only; the
//! same engine as the GUI's Analyse > Cohorts... and `--cohort`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::cohort::{CohortSpec, Period, Show, collect, wide};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Cohort table: each person (`who`) belongs to the period of their \
first row (`when`, a date or ISO text); each cell says how many of a cohort were active `k` \
periods later. `period`: day, week (Monday start), month (default), quarter, year. `show`: \
share (default, percent of the cohort, period 0 is 100), count, or sum (adds `sum_column` per \
cohort and period). Cells for periods the data does not reach yet are null. Rows without a \
`who` or a readable date are left out and counted in `skipped`. Returns `{skipped, table}`; \
columns `cohort`, `size`, `<period>_0`, `<period>_1`, ...";

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
    /// The column naming a person or customer.
    pub who: String,
    /// The date or timestamp column.
    pub when: String,
    /// day, week, month (default), quarter, year.
    #[serde(default)]
    pub period: Option<String>,
    /// share (default), count, sum.
    #[serde(default)]
    pub show: Option<String>,
    /// The number column for `show: sum`.
    #[serde(default)]
    pub sum_column: Option<String>,
    /// Rows in the response. Pass 0 for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let period = match p.period.as_deref() {
        None => Period::Month,
        Some(s) => Period::parse(s).ok_or_else(|| {
            anyhow::anyhow!("unknown period \"{s}\", expected day, week, month, quarter or year")
        })?,
    };
    let show = match p.show.as_deref() {
        None => Show::Share,
        Some(s) => Show::parse(s)
            .ok_or_else(|| anyhow::anyhow!("unknown show \"{s}\", expected share, count or sum"))?,
    };
    let table = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let spec = CohortSpec {
        who: table.column_index(&p.who)?,
        when: table.column_index(&p.when)?,
        period,
        show,
        sum_col: p
            .sum_column
            .as_deref()
            .map(|c| table.column_index(c))
            .transpose()?,
    };
    let cells =
        collect(&table, &spec).map_err(|e| anyhow::anyhow!("{}", octa::i18n::t(e.key())))?;
    Ok(json!({
        "skipped": cells.skipped,
        "table": table_to_json(&wide(&cells), ctx.resolve_row_cap(p.limit), ctx.cell_byte_cap),
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("cohort_table failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
