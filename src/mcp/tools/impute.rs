//! MCP tool: `fill_missing` - fill missing/empty cells in one column via a
//! chosen imputation strategy.
//!
//! The source is resolved through the shared `ToolContext` (a file path or an
//! open GUI tab). Imputation is handled by `octa::data::impute::impute_column`.
//! The result table is the source with the target column's cells replaced by
//! the imputed values.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use octa::data::impute::{ImputeStrategy, impute_column};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Fill missing or empty cells in one column of a tabular file or open tab. \
     `column` is the column name to impute; `strategy` chooses the fill method: \
     `mean` or `median` (numeric columns only), `mode` (most frequent value), \
     `ffill` (forward-fill from the previous non-null row), `bfill` (backward-fill \
     from the next non-null row), or `const` (fill with the literal string in \
     `value`). Returns the table with the imputed column, in the same \
     `{schema, rows, row_count, truncated, ...}` shape as `read_table`.";

/// Parameters for `fill_missing`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to impute.
    #[serde(default)]
    pub table: Option<String>,

    /// Name of the column whose missing cells should be filled.
    pub column: String,

    /// Imputation strategy: `mean`, `median`, `mode`, `ffill`, `bfill`, or
    /// `const` (use the `value` field for the fill value).
    pub strategy: String,

    /// Fill value for the `const` strategy. Ignored for other strategies.
    #[serde(default)]
    pub value: Option<String>,

    /// Maximum rows to return in the response. Default is the server's
    /// configured limit. Pass `0` for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Lift the streaming initial-load cap so every row in the source file
    /// is read from disk. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

fn parse_strategy(strategy: &str, value: Option<&str>) -> anyhow::Result<ImputeStrategy> {
    match strategy.to_ascii_lowercase().as_str() {
        "mean" => Ok(ImputeStrategy::Mean),
        "median" => Ok(ImputeStrategy::Median),
        "mode" => Ok(ImputeStrategy::Mode),
        "ffill" => Ok(ImputeStrategy::ForwardFill),
        "bfill" => Ok(ImputeStrategy::BackwardFill),
        "const" => {
            let v = value.ok_or_else(|| {
                anyhow::anyhow!("strategy `const` requires the `value` field to be set")
            })?;
            Ok(ImputeStrategy::Constant(v.to_string()))
        }
        other => anyhow::bail!(
            "unknown strategy \"{other}\"; expected mean, median, mode, ffill, bfill, or const"
        ),
    }
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let mut dt = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;

    let col_idx = dt
        .columns
        .iter()
        .position(|c| c.name == p.column)
        .ok_or_else(|| anyhow::anyhow!("no such column: {}", p.column))?;

    let strategy = parse_strategy(&p.strategy, p.value.as_deref())?;
    let imputed = impute_column(&dt, col_idx, &strategy)?;

    // Replace the column's cells in-place on the cloned table.
    for (r, cell) in imputed.into_iter().enumerate() {
        if r < dt.rows.len() && col_idx < dt.rows[r].len() {
            dt.rows[r][col_idx] = cell;
        }
    }

    let row_cap = ctx.resolve_row_cap(p.limit);
    let cell_cap = ctx.cell_byte_cap;
    Ok(table_to_json(&dt, row_cap, cell_cap))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("fill_missing failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
