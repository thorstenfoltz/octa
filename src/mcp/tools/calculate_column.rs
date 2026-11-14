//! MCP tool: `calculate_column` - add a rank, row number, running total,
//! percent of total or change-to-previous column and write the result. A
//! write tool (dropped under `--mcp-read-only`). Same engine as the GUI
//! dialog and `octa --calculate`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::calculated::{CalcByName, CalcExtras, CalcKind, add_calculated_column};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, read_with_registry};

pub const DESCRIPTION: &str = "Add one calculated column and write the result. `kind`: `rank` \
and `dense_rank` (place by `value`, ties share a place; rank leaves a gap, dense rank does not; \
empty values last), `row_number` (1..n in `order`, no `value`), `running_total`, \
`percent_of_total` (a percentage, 12.5 means 12.5 %), `diff_prev` and `pct_change_prev` (to the \
previous row in `order`; empty at the start, beside an empty value, and for a percent change \
from 0), `previous` and `next` (the value `offset` rows before or after in `order`, default 1; \
empty at the edge of a group), `session` (visits numbered 1, 2, 3 per group: `value` holds the \
timestamps, put the user column in `groups`; a pause longer than `gap_minutes`, default 30, \
starts a new visit), `abc` (A, B or C by the running share of the group total, largest `value` \
first; A up to `abc_a` percent, default 80, B up to `abc_b`, default 95; negative values are \
refused). `groups` restart the calculation for each combination of their values. `descending` \
reverses the order; default: largest first for the ranks, ascending otherwise. Numbers stored \
as text are read as numbers; other text refuses the arithmetic kinds. `new_column` names the \
result (default `<value>_<kind>`; a taken name you pass is refused); it is inserted right of `value` \
(row_number at the left). Writes to `output_path` (default: overwrite `path`); the format \
follows its extension. Database files are not valid sources or targets.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the source file.
    pub path: PathBuf,
    /// rank, dense_rank, row_number, running_total, percent_of_total, diff_prev, pct_change_prev,
    /// previous, next, session, abc.
    pub kind: CalcKind,
    /// The column the calculation works on (not for row_number).
    #[serde(default)]
    pub value: Option<String>,
    /// Row order for row_number, running_total, diff_prev, pct_change_prev, previous, next.
    /// Default: file order.
    #[serde(default)]
    pub order: Option<String>,
    /// Largest first. Default: true for the ranks, false otherwise.
    #[serde(default)]
    pub descending: Option<bool>,
    /// Columns whose values restart the calculation.
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(flatten)]
    pub extras: CalcExtras,
    /// Name of the new column.
    #[serde(default)]
    pub new_column: Option<String>,
    /// Where to write the result. Defaults to `path` (overwrite in place).
    #[serde(default)]
    pub output_path: Option<PathBuf>,
    /// Lift the streaming initial-load cap so every row is read.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    ctx.ensure_readable(&p.path)?;
    let mut table = read_with_registry(&p.path, None)?;
    if table.db_meta.is_some() {
        anyhow::bail!("database files (SQLite/DuckDB/GeoPackage) are not valid sources");
    }
    if table.is_partial() {
        anyhow::bail!(
            "only part of {} was read; pass unlimited: true to calculate over every row",
            p.path.display()
        );
    }
    let spec = CalcByName {
        kind: p.kind,
        value: p.value.clone(),
        order: p.order.clone(),
        descending: p.descending,
        groups: p.groups.clone(),
        extras: p.extras,
    }
    .resolve(&table)?;
    let (name, _) = add_calculated_column(&mut table, &spec, p.new_column.as_deref())?;
    let requested = p.output_path.clone().unwrap_or_else(|| p.path.clone());
    let target = ctx.write_transformed(&requested, &table)?;
    let mut out = Map::new();
    out.insert("rows_written".to_string(), Value::from(table.row_count()));
    out.insert("new_column".to_string(), Value::String(name));
    out.insert("output".to_string(), Value::String(target));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("calculate_column failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
