//! MCP tool: `diff_tables` - row-level diff of two files.
//!
//! Reads both files through the shared registry and delegates to the pure
//! `octa::data::diff::diff_rows`. The response carries the rows unique to each
//! side (each as a `table_to_json` payload, so `limit` / cell caps apply) plus
//! the shared-row count.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::DataTable;
use octa::data::diff::{RowDiff, diff_rows};

use crate::mcp::OctaMcpServer;

use super::{read_with_registry, table_to_json};

// Tool description lives inline at the `#[tool]` site in `src/mcp/mod.rs`.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the first file (side A).
    pub path_a: PathBuf,

    /// Path to the second file (side B).
    pub path_b: PathBuf,

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

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let row_cap = server.resolve_row_cap(p.limit);
    let cell_cap = server.cell_byte_cap;
    let path_a = p.path_a.clone();
    let path_b = p.path_b.clone();
    let table_a = p.table_a.clone();
    let table_b = p.table_b.clone();
    let unlimited = p.unlimited;

    let (a, b, diff) =
        tokio::task::spawn_blocking(move || -> anyhow::Result<(DataTable, DataTable, RowDiff)> {
            let _g = unlimited.then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
            let a = read_with_registry(&path_a, table_a.as_deref())?;
            let b = read_with_registry(&path_b, table_b.as_deref())?;
            let diff = diff_rows(&a, &b);
            Ok((a, b, diff))
        })
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("diff_tables failed: {e}"), None))?;

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

    Ok(CallToolResult::success(vec![Content::text(
        Value::Object(out).to_string(),
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
