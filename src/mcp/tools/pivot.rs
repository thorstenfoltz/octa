//! MCP tool: `pivot` - reshape a table between long and wide form via DuckDB's
//! `PIVOT` / `UNPIVOT`. Mirrors the GUI Pivot dialog; both build the SQL with
//! the shared `octa::data::pivot` builders and run it through
//! `octa::sql::run_query`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use octa::data::pivot::{PivotAgg, pivot_sql, unpivot_sql};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Reshape a table between long and wide form using DuckDB PIVOT / \
UNPIVOT. With `mode: \"pivot\"` (default), spread the distinct values of column `on` into new \
columns, aggregating `value` with `agg` (sum/count/avg/min/max), optionally grouped by `group`. \
With `mode: \"unpivot\"`, melt the columns in `columns` into two columns named `name_col` / \
`value_col` (long form). Returns the reshaped table as `{schema, rows, row_count, ...}`. Operates \
on a file `path` or an `open_tab`.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to reshape.
    #[serde(default)]
    pub table: Option<String>,

    /// `pivot` (long -> wide, default) or `unpivot` (wide -> long).
    #[serde(default)]
    pub mode: Option<String>,

    // --- Pivot parameters ---
    /// Pivot: the column whose distinct values become new columns.
    #[serde(default)]
    pub on: Option<String>,
    /// Pivot: the column aggregated under each new column.
    #[serde(default)]
    pub value: Option<String>,
    /// Pivot: aggregate function (`sum`/`count`/`avg`/`min`/`max`). Default `sum`.
    #[serde(default)]
    pub agg: Option<String>,
    /// Pivot: identity columns kept as rows (empty = DuckDB infers them).
    #[serde(default)]
    pub group: Vec<String>,

    // --- Unpivot parameters ---
    /// Unpivot: the columns to melt (at least two).
    #[serde(default)]
    pub columns: Vec<String>,
    /// Unpivot: name of the generated key column. Default `name`.
    #[serde(default)]
    pub name_col: Option<String>,
    /// Unpivot: name of the generated value column. Default `value`.
    #[serde(default)]
    pub value_col: Option<String>,

    /// Cap how many rows the response carries (0 = unlimited).
    #[serde(default)]
    pub limit: Option<usize>,

    /// Lift the streaming initial-load cap so the reshape sees every row.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let mode = p.mode.as_deref().unwrap_or("pivot").to_ascii_lowercase();
    let sql = match mode.as_str() {
        "pivot" => {
            let on =
                p.on.as_deref()
                    .ok_or_else(|| anyhow::anyhow!("pivot requires `on`"))?;
            let value = p
                .value
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("pivot requires `value`"))?;
            let agg = match p.agg.as_deref() {
                None => PivotAgg::Sum,
                Some(s) => PivotAgg::parse(s).ok_or_else(|| {
                    anyhow::anyhow!("unknown aggregate `{s}` (use sum/count/avg/min/max)")
                })?,
            };
            pivot_sql(on, agg, value, &p.group)
        }
        "unpivot" => {
            let name_col = p.name_col.as_deref().unwrap_or("name");
            let value_col = p.value_col.as_deref().unwrap_or("value");
            unpivot_sql(&p.columns, name_col, value_col)
                .ok_or_else(|| anyhow::anyhow!("unpivot needs at least two `columns`"))?
        }
        other => anyhow::bail!("unknown mode `{other}` (use `pivot` or `unpivot`)"),
    };

    let dt = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let outcome =
        octa::sql::run_query(&dt, &sql).map_err(|e| anyhow::anyhow!("pivot query failed: {e}"))?;

    let row_cap = ctx.resolve_row_cap(p.limit);
    Ok(table_to_json(&outcome.table, row_cap, ctx.cell_byte_cap))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("pivot failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
