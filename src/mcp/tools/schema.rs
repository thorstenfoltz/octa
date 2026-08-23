//! MCP tool: `schema` - return only the column schema for a file.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::Value;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, schema_to_json, source_from};

pub const DESCRIPTION: &str = "Return the column schema (name + data type) of a tabular file or open tab. No rows are \
serialised, so this is the cheap discovery step before `read_table` or `run_sql`. For \
multi-table sources pass `table`.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to inspect.
    #[serde(default)]
    pub table: Option<String>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let source = source_from(&p.open_tab, &p.path, &p.table);
    // The columns of a big file come from a DESCRIBE over the scan, so asking
    // what is in a forty gigabyte file reads none of it.
    if let Some(scan) = ctx.scan_for(&source) {
        let mut table = octa::data::DataTable::empty();
        table.columns = scan.columns().to_vec();
        let mut out = schema_to_json(&table);
        if let Some(m) = out.as_object_mut() {
            m.insert("streamed".to_string(), Value::Bool(true));
        }
        return Ok(out);
    }
    let dt = ctx.resolve(&source)?;
    Ok(schema_to_json(&dt))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("schema failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
