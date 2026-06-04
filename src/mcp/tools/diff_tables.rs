//! MCP tool: `diff_tables` - row-level diff of two files.
//!
//! Reads both sources through the shared registry (or open tabs) and
//! delegates to the pure `octa::data::diff::diff_rows`. The response carries
//! the rows unique to each side (each as a `table_to_json` payload, so
//! `limit` / cell caps apply) plus the shared-row count.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::DataTable;
use octa::data::diff::diff_rows;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Row-level diff of two tabular sources (files or open tabs), comparing whole-row content \
positionally. Returns `only_in_a` / `only_in_b` (table payloads of the rows unique to each \
side), their counts, and `shared_keys`. `limit` caps rows per side (0 = unlimited). Run \
`compare_schemas` first if the column layouts might differ.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the first file (side A). Omit when `open_tab_a` is set.
    #[serde(default)]
    pub path_a: PathBuf,

    /// Path to the second file (side B). Omit when `open_tab_b` is set.
    #[serde(default)]
    pub path_b: PathBuf,

    /// Operate on an open GUI tab for side A (name, or `@active`).
    #[serde(default)]
    pub open_tab_a: Option<String>,

    /// Operate on an open GUI tab for side B (name, or `@active`).
    #[serde(default)]
    pub open_tab_b: Option<String>,

    /// For multi-table sources, the table name to read from file A.
    #[serde(default)]
    pub table_a: Option<String>,

    /// For multi-table sources, the table name to read from file B.
    #[serde(default)]
    pub table_b: Option<String>,

    /// Maximum rows to return *per side*. Default is the server's configured
    /// limit. Pass 0 for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Lift the streaming initial-load cap for this call so every row in both
    /// files is read from disk. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let a = ctx.resolve(&source_from(&p.open_tab_a, &p.path_a, &p.table_a))?;
    let b = ctx.resolve(&source_from(&p.open_tab_b, &p.path_b, &p.table_b))?;
    let diff = diff_rows(&a, &b);

    let row_cap = ctx.resolve_row_cap(p.limit);
    let cell_cap = ctx.cell_byte_cap;
    let a_sub = subset(&a, &diff.only_in_a);
    let b_sub = subset(&b, &diff.only_in_b);

    let mut out = Map::new();
    out.insert(
        "only_in_a".to_string(),
        table_to_json(&a_sub, row_cap, cell_cap),
    );
    out.insert(
        "only_in_b".to_string(),
        table_to_json(&b_sub, row_cap, cell_cap),
    );
    out.insert(
        "only_in_a_count".to_string(),
        Value::from(diff.only_in_a.len()),
    );
    out.insert(
        "only_in_b_count".to_string(),
        Value::from(diff.only_in_b.len()),
    );
    out.insert("shared_keys".to_string(), Value::from(diff.shared_keys));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("diff_tables failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}

/// Materialise `indices` from `table` into a new `DataTable` (columns cloned),
/// so `table_to_json` can serialise just the differing rows.
fn subset(table: &DataTable, indices: &[usize]) -> DataTable {
    let mut out = DataTable::empty();
    out.columns = table.columns.clone();
    out.rows = indices
        .iter()
        .map(|&r| {
            (0..table.col_count())
                .map(|c| {
                    table
                        .get(r, c)
                        .cloned()
                        .unwrap_or(octa::data::CellValue::Null)
                })
                .collect()
        })
        .collect();
    out
}
