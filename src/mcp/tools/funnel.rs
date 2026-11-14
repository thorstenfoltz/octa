//! MCP tool: `funnel` - how many people get through a series of steps in
//! order, and where they drop out. Read-only; the same engine as the GUI's
//! Analyse > Funnel... and `--funnel`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::funnel::{FunnelSpec, collect, steps_from, table};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Funnel: how many people (`who`) get through `steps` (values of \
the `event` column, two or more, each once) in that order. A person reaches a step when they \
have that event at or after the moment they reached the step before; `when` (a date or \
timestamp, or ISO text) orders the rows, without it the table's row order does. \
`within_days` counts a later step only that many days or fewer after the first step (needs \
`when`). Rows without a `who` or a readable time are left out and counted in `skipped`. \
Returns `{skipped, table}`; columns `step`, `users`, `share_of_first`, `share_of_previous` \
(percent) and `dropped` (people lost since the step before).";

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
    /// The column saying what happened in each row.
    pub event: String,
    /// The date or timestamp column. Omit to use the row order.
    #[serde(default)]
    pub when: Option<String>,
    /// Event values in funnel order, two or more.
    pub steps: Vec<String>,
    /// Count a later step only within this many days of the first step.
    #[serde(default)]
    pub within_days: Option<u32>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let data = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let spec = FunnelSpec {
        who: data.column_index(&p.who)?,
        event: data.column_index(&p.event)?,
        when: p
            .when
            .as_deref()
            .map(|c| data.column_index(c))
            .transpose()?,
        steps: steps_from(&p.steps),
        within_days: p.within_days,
    };
    let counts =
        collect(&data, &spec).map_err(|e| anyhow::anyhow!("{}", octa::i18n::t(e.key())))?;
    Ok(json!({
        "skipped": counts.skipped,
        "table": table_to_json(&table(&counts), None, ctx.cell_byte_cap),
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("funnel failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
