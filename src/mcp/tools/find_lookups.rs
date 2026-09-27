//! MCP tool: `find_lookups` - hidden lookup tables, columns that always
//! follow another column. Read-only; the same scan as the GUI's
//! **Analyse -> Find lookup tables...** and `octa --lookups`.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::lookups::{DEFAULT_MIN_CONSISTENCY, find_lookups, findings_table};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Find hidden lookup tables: columns that always have the same value \
for the same key (a customer's name and city following customer_id), which means a flat export \
really holds two tables. Only keys that repeat are considered. `min_consistency` (0 to 1, default \
0.95) is the share of rows that must agree with their key's most common value. Returns \
`{findings}`, one row per key and following column with `consistency_percent`, \
`conflicting_keys` and `breaking_rows`. Breaking rows are often typos.";

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
    /// Share of rows (0 to 1) that must agree with their key's most common
    /// value. Default 0.95.
    #[serde(default)]
    pub min_consistency: Option<f64>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let min = p.min_consistency.unwrap_or(DEFAULT_MIN_CONSISTENCY);
    anyhow::ensure!(
        (0.0..=1.0).contains(&min),
        "min_consistency must be between 0 and 1"
    );
    let table = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let findings = find_lookups(&table, min, &AtomicBool::new(false));
    Ok(json!({
        "findings": table_to_json(
            &findings_table(&table, &findings),
            ctx.resolve_row_cap(None),
            ctx.cell_byte_cap,
        ),
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("find_lookups failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
