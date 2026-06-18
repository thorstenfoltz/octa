//! MCP tool: `union_tables` - concatenate multiple tabular sources into one,
//! reconciling differing schemas.
//!
//! Each source is resolved through the shared `ToolContext` (a file path or an
//! open GUI tab). The plan is computed by `octa::data::union::plan_union` and
//! can be adjusted via `drop` / `cast` before the actual stack operation.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use octa::data::union::{plan_union, union_tables};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Concatenate (union) multiple tables into one, reconciling differing schemas. \
     By default takes the union of all columns (missing cells become null) and widens \
     conflicting numeric types (int+float -> float, any other disagreement -> text). \
     Use `drop` to omit columns and `cast` to override a column's target type. \
     Sources may be file paths or open GUI tabs (`open_tab` per entry).";

/// One entry in the `sources` list: a file path or an open GUI tab.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SourceParam {
    /// Path to a file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Address an open GUI tab by name or handle (e.g. `@active`). Omit when
    /// `path` is set.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources (SQLite, DuckDB), the inner table name.
    #[serde(default)]
    pub table: Option<String>,
}

/// A single cast override: change the resolved target Arrow type for one column.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CastOverride {
    /// Column name to retype.
    pub column: String,
    /// Arrow type name to use (e.g. `"Utf8"`, `"Float64"`, `"Int64"`).
    #[serde(rename = "type")]
    pub target_type: String,
}

/// Parameters for `union_tables`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Two or more sources to stack vertically. Each entry has a `path` (file)
    /// or `open_tab` (GUI tab name / `@active`), plus an optional inner `table`
    /// for multi-table sources. At least two sources are required.
    pub sources: Vec<SourceParam>,

    /// Column names to exclude from the output. Unrecognised names are silently
    /// ignored.
    #[serde(default)]
    pub drop: Vec<String>,

    /// Override the resolved target Arrow type for specific columns.
    #[serde(default)]
    pub cast: Vec<CastOverride>,

    /// Maximum rows to return in the response. Default is the server's
    /// configured limit. Pass `0` for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Lift the streaming initial-load cap so every row in all source files is
    /// read from disk. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.sources.len() < 2 {
        anyhow::bail!("union_tables needs at least two sources");
    }

    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let snaps: Vec<octa::data::DataTable> = p
        .sources
        .iter()
        .map(|s| ctx.resolve(&source_from(&s.open_tab, &s.path, &s.table)))
        .collect::<anyhow::Result<_>>()?;

    let tables: Vec<&octa::data::DataTable> = snaps.iter().collect();
    let schemas: Vec<&[octa::data::ColumnInfo]> =
        tables.iter().map(|t| t.columns.as_slice()).collect();
    let mut plan = plan_union(&schemas);

    for col_name in &p.drop {
        if let Some(c) = plan.columns.iter_mut().find(|c| &c.name == col_name) {
            c.include = false;
        }
    }

    for override_ in &p.cast {
        if let Some(c) = plan.columns.iter_mut().find(|c| c.name == override_.column) {
            c.target_type = override_.target_type.clone();
        }
    }

    let out = union_tables(&tables, &plan)?;

    let row_cap = ctx.resolve_row_cap(p.limit);
    let cell_cap = ctx.cell_byte_cap;
    Ok(table_to_json(&out, row_cap, cell_cap))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("union_tables failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
