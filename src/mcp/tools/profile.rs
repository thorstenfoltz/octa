//! MCP tool: `profile` — per-column statistics via DuckDB `SUMMARIZE`.
//!
//! The file is registered as the DuckDB temp table `data` (types are
//! preserved by `octa::sql::register_table`, so numeric columns get real
//! numeric stats) and `SUMMARIZE data` is run. The result — one row per
//! source column — is reshaped into an object keyed by SUMMARIZE's own
//! column names (`min`, `max`, `avg`, `std`, `q25`/`q50`/`q75`,
//! `approx_unique`, `count`, `null_percentage`, …).

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::CellValue;
use octa::sql::run_query;

use crate::mcp::OctaMcpServer;

use super::read_with_registry;

// Tool description lives inline at the `#[tool]` site in `src/mcp/mod.rs`.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file.
    pub path: PathBuf,

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

pub async fn handle(_server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let path = p.path.clone();
    let table_name = p.table.clone();
    let unlimited = p.unlimited;

    let summary = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let _g = unlimited.then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
        let dt = read_with_registry(&path, table_name.as_deref())?;
        if dt.col_count() == 0 {
            anyhow::bail!("file has no columns to profile");
        }
        let outcome = run_query(&dt, "SUMMARIZE data")?;
        Ok(outcome.table)
    })
    .await
    .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
    .map_err(|e| McpError::invalid_params(format!("profile failed: {e}"), None))?;

    // SUMMARIZE yields one row per source column; reshape each row into
    // an object keyed by SUMMARIZE's own column names.
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

    let mut out = Map::new();
    out.insert("column_count".to_string(), Value::from(columns.len()));
    out.insert("columns".to_string(), Value::Array(columns));
    Ok(CallToolResult::success(vec![Content::text(
        Value::Object(out).to_string(),
    )]))
}
