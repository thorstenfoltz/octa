//! MCP tool: `convert` - read a file in one format, write in another.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::formats::FormatRegistry;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Convert a tabular source (file or open tab) to another format. The output extension picks \
the writer; read-only formats (SAS, RDS, HDF5, NetCDF) cannot be a target. Returns the \
row/column count and the output path.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the input file. Extension determines the read format. Omit
    /// when `open_tab` is set.
    #[serde(default)]
    pub input: PathBuf,

    /// Convert an open GUI tab instead of an input file. Pass the tab's name,
    /// or `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

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

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let table = ctx.resolve(&source_from(&p.open_tab, &p.input, &p.table))?;
    // Confine chat writes to the export dir (no-op for MCP / CLI).
    let output = ctx.resolve_write_path(&p.output)?;
    let registry = FormatRegistry::new();
    let out_reader = registry.reader_for_path(&output).ok_or_else(|| {
        anyhow::anyhow!(
            "no reader available for output extension on {}",
            output.display()
        )
    })?;
    if !out_reader.supports_write() {
        anyhow::bail!(
            "format {} does not support writing - pick a different output extension",
            out_reader.name()
        );
    }
    if ctx.backup_before_modify && output.exists() {
        octa::formats::backup_existing_file(&output)?;
    }
    out_reader.write_file_schema_aware(&output, &table, ctx.allow_schema_changes)?;

    let mut out = Map::new();
    out.insert("rows_written".to_string(), Value::from(table.row_count()));
    out.insert("cols_written".to_string(), Value::from(table.col_count()));
    out.insert(
        "output".to_string(),
        Value::String(output.display().to_string()),
    );
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("convert failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
