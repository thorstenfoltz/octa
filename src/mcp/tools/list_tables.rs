//! MCP tool: `list_tables` — enumerate the tables in a multi-table source.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::formats::FormatRegistry;

use crate::mcp::OctaMcpServer;

// Tool description lives inline at the `#[tool]` site in `src/mcp/mod.rs`.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file.
    pub path: PathBuf,
}

pub async fn handle(_server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let path = p.path.clone();
    let list = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let registry = FormatRegistry::new();
        let reader = registry
            .reader_for_path(&path)
            .ok_or_else(|| anyhow::anyhow!("no reader available for {}", path.display()))?;
        reader.list_tables(&path)
    })
    .await
    .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
    .map_err(|e| McpError::invalid_params(format!("list_tables failed: {e}"), None))?;

    let tables: Vec<Value> = list
        .unwrap_or_default()
        .into_iter()
        .map(|t| {
            let cols: Vec<Value> = t
                .columns
                .iter()
                .map(|c| {
                    let mut m = Map::new();
                    m.insert("name".to_string(), Value::String(c.name.clone()));
                    m.insert("type".to_string(), Value::String(c.data_type.clone()));
                    Value::Object(m)
                })
                .collect();
            let mut m = Map::new();
            m.insert("name".to_string(), Value::String(t.name));
            m.insert("columns".to_string(), Value::Array(cols));
            m.insert(
                "row_count".to_string(),
                t.row_count.map_or(Value::Null, Value::from),
            );
            Value::Object(m)
        })
        .collect();

    let mut out = Map::new();
    out.insert("tables".to_string(), Value::Array(tables));
    Ok(CallToolResult::success(vec![Content::text(
        Value::Object(out).to_string(),
    )]))
}
