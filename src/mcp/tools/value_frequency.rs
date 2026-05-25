//! MCP tool: `value_frequency` — per-column value counts (`value_counts`).

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::value_frequency::{BinningMode, compute_value_frequency};

use crate::mcp::OctaMcpServer;

use super::read_with_registry;

// Tool description lives inline at the `#[tool]` site in `src/mcp/mod.rs`.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file.
    pub path: PathBuf,

    /// For multi-table sources, the specific table to scan.
    #[serde(default)]
    pub table: Option<String>,

    /// Name of the column to count values for.
    pub column: String,

    /// Return only the N most frequent values / bins. Omit for all.
    #[serde(default)]
    pub top_n: Option<usize>,

    /// Group numeric columns into Sturges bins instead of counting raw
    /// values. Ignored for non-numeric columns.
    #[serde(default)]
    pub bin: bool,

    /// Lift the streaming initial-load cap so the frequency counts include
    /// every row in the file. Without this, counts reflect at most the
    /// first `initial_load_rows` rows. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub async fn handle(_server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let path = p.path.clone();
    let table_name = p.table.clone();
    let column = p.column.clone();
    let top_n = p.top_n;
    let unlimited = p.unlimited;
    let binning = if p.bin {
        BinningMode::Sturges
    } else {
        BinningMode::None
    };

    let vf = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let _g = unlimited.then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
        let dt = read_with_registry(&path, table_name.as_deref())?;
        let col_idx = dt
            .columns
            .iter()
            .position(|c| c.name == column)
            .ok_or_else(|| anyhow::anyhow!("no such column: {column}"))?;
        compute_value_frequency(&dt, col_idx, top_n, binning)
            .ok_or_else(|| anyhow::anyhow!("could not compute value frequency for `{column}`"))
    })
    .await
    .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
    .map_err(|e| McpError::invalid_params(format!("value_frequency failed: {e}"), None))?;

    let rows: Vec<Value> = vf
        .rows
        .iter()
        .map(|r| {
            let mut m = Map::new();
            m.insert("label".to_string(), Value::String(r.label.clone()));
            m.insert("count".to_string(), Value::from(r.count));
            Value::Object(m)
        })
        .collect();

    let mut out = Map::new();
    out.insert("column_name".to_string(), Value::String(vf.column_name));
    out.insert("binned".to_string(), Value::Bool(vf.binned));
    out.insert("nulls".to_string(), Value::from(vf.nulls));
    out.insert("total_non_null".to_string(), Value::from(vf.total_non_null));
    out.insert("unique_count".to_string(), Value::from(vf.unique_count));
    out.insert("rows".to_string(), Value::Array(rows));
    Ok(CallToolResult::success(vec![Content::text(
        Value::Object(out).to_string(),
    )]))
}
