//! MCP tool: `convert` — read a file in one format, write in another.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::formats::FormatRegistry;

use crate::mcp::OctaMcpServer;

use super::read_with_registry;

// Tool description lives inline at the `#[tool]` site in `src/mcp/mod.rs`.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the input file. Extension determines the read format.
    pub input: PathBuf,

    /// Path to the output file. Extension determines the write format.
    pub output: PathBuf,

    /// For multi-table input sources, load this specific table.
    #[serde(default)]
    pub table: Option<String>,

    /// Lift the streaming initial-load cap on the input read so the entire
    /// source file is converted. Without this, conversion is bounded by
    /// the default cap. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub async fn handle(_server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let input = p.input.clone();
    let output = p.output.clone();
    let table_name = p.table.clone();
    let unlimited = p.unlimited;
    let (rows, cols, out_path) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let _g = unlimited.then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
        let table = read_with_registry(&input, table_name.as_deref())?;
        let registry = FormatRegistry::new();
        let out_reader = registry.reader_for_path(&output).ok_or_else(|| {
            anyhow::anyhow!(
                "no reader available for output extension on {}",
                output.display()
            )
        })?;
        if !out_reader.supports_write() {
            anyhow::bail!(
                "format {} does not support writing — pick a different output extension",
                out_reader.name()
            );
        }
        out_reader.write_file(&output, &table)?;
        Ok((
            table.row_count(),
            table.col_count(),
            output.display().to_string(),
        ))
    })
    .await
    .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
    .map_err(|e| McpError::invalid_params(format!("convert failed: {e}"), None))?;

    let mut out = Map::new();
    out.insert("rows_written".to_string(), Value::from(rows));
    out.insert("cols_written".to_string(), Value::from(cols));
    out.insert("output".to_string(), Value::String(out_path));
    Ok(CallToolResult::success(vec![Content::text(
        Value::Object(out).to_string(),
    )]))
}
