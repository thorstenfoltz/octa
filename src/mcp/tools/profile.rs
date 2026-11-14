//! MCP tool: `profile` - per-column statistics via DuckDB `SUMMARIZE`.
//!
//! The table is registered as the DuckDB temp table `data` (types are
//! preserved by `octa::sql::register_table`, so numeric columns get real
//! numeric stats) and `SUMMARIZE data` is run. The result - one row per
//! source column - is reshaped into an object keyed by SUMMARIZE's own
//! column names (`min`, `max`, `avg`, `std`, `q25`/`q50`/`q75`,
//! `approx_unique`, `count`, `null_percentage`, ...).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::summary::SummaryStat;
use octa::data::{CellValue, DataTable};
use octa::db::pushdown::ServerSource;
use octa::sql::run_query;

use crate::mcp::OctaMcpServer;

use super::{TableSnapshot, ToolContext, source_from};

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

/// One row per source column, each reshaped into an object keyed by the summary's
/// own column names.
fn columns_of(summary: &DataTable) -> Vec<Value> {
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
    columns
}

/// The same Summary the GUI runs on a database tab holding part of its table, in
/// one scan: the median, quartiles and mode take a statement per column, which on
/// a warehouse is real time and money for something the model may not need.
fn on_server(ctx: &ToolContext, snap: &TableSnapshot, src: &ServerSource) -> anyhow::Result<Value> {
    use SummaryStat as S;
    let per_column = [S::Median, S::Iqr, S::Q25, S::Q75, S::Mode, S::ModeCount];
    let enabled: Vec<S> = S::all()
        .into_iter()
        .filter(|s| !per_column.contains(s))
        .collect();
    let mut c = ctx.db_tab_connect(src)?;
    let cancel = AtomicBool::new(false);
    let rows = octa::db::pushdown::count_rows(&mut *c, src, &cancel)?;
    let (summary, local) =
        octa::db::pushdown::summary::run(&mut *c, src, &snap.table, rows, &enabled, &cancel)?;
    let columns = columns_of(&summary);
    let mut out = Map::new();
    out.insert("on_server".to_string(), Value::Bool(true));
    out.insert("row_count".to_string(), Value::from(rows));
    out.insert("column_count".to_string(), Value::from(columns.len()));
    out.insert("columns".to_string(), Value::Array(columns));
    out.insert(
        "note".to_string(),
        Value::String(
            "Computed on the server over every row. The median, quartiles and mode are left out, \
             as each takes a statement per column: use run_sql for the columns that matter."
                .to_string(),
        ),
    );
    if !local.is_empty() {
        let parts = [local.engine, local.by_design, local.failed].concat();
        out.insert("from_loaded_rows".to_string(), Value::from(parts));
    }
    Ok(Value::Object(out))
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let source = source_from(&p.open_tab, &p.path, &p.table);
    if let Some((snap, src)) = ctx.db_tab(&source) {
        return on_server(ctx, snap, src);
    }
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let dt = ctx.resolve(&source)?;
    if dt.col_count() == 0 {
        anyhow::bail!("file has no columns to profile");
    }
    let summary = run_query(&dt, "SUMMARIZE data")?.table;
    let columns = columns_of(&summary);

    let mut out = Map::new();
    out.insert("column_count".to_string(), Value::from(columns.len()));
    out.insert("columns".to_string(), Value::Array(columns));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("profile failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
