//! MCP tool: `read_table` - load a file and return schema + rows.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::Value;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

/// LLM-facing description used by the in-GUI chat tool registry. The MCP
/// server keeps its own string literal at the `#[tool(description = ...)]`
/// site (rmcp's macro only accepts a literal there); keep the two roughly in
/// sync when editing.
pub const DESCRIPTION: &str = "Read a tabular data file (or an open tab) and return its column schema and rows. \
Supports Parquet, CSV, TSV, JSON, JSONL, Excel, SQLite, DuckDB, GeoPackage, ORC, Avro, \
Arrow, and text formats. Returns `schema`, `rows`, `row_count`, and `truncated`. Use \
`limit` to cap response rows (0 = unlimited).";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Absolute or working-directory-relative path to the file. Omit when
    /// `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab. Has no effect under `--mcp`
    /// (there are no open tabs there).
    #[serde(default)]
    pub open_tab: Option<String>,

    /// Maximum rows to return. Omit to use the configured default; pass 0 for
    /// unlimited.
    /// Note: this only slices the *response*. The file is still read with
    /// the streaming initial-load cap (5 M rows by default). Set `unlimited`
    /// to lift the file-loader cap as well.
    #[serde(default)]
    pub limit: Option<usize>,

    /// For multi-table sources (SQLite, DuckDB, GeoPackage), the specific
    /// table to load. Omit for single-table formats.
    #[serde(default)]
    pub table: Option<String>,

    /// Lift the streaming initial-load cap for this call so every row in the
    /// file is read from disk. Combine with `limit: 0` to actually return
    /// every row in the response. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

/// Shared sync implementation called by both the MCP `handle` wrapper and the
/// in-GUI chat agent.
pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let source = source_from(&p.open_tab, &p.path, &p.table);
    let cap = ctx.resolve_row_cap(p.limit);
    // A big file is paged rather than loaded: returning a hundred rows should
    // not cost five million. `unlimited` is refused here rather than honoured,
    // because the whole point is that this file does not fit.
    if let Some(scan) = ctx.scan_for(&source) {
        let want = match cap {
            Some(0) | None => scan.row_count(),
            Some(n) => n,
        };
        // The scan is ordered by the first column so a page is reproducible:
        // DuckDB parallelises a scan, and an unordered LIMIT would return a
        // different hundred rows each call.
        let page = scan.page(0, want, Some((0, true)), None)?;
        let mut out = table_to_json(&page, cap, ctx.cell_byte_cap);
        if let Some(m) = out.as_object_mut() {
            m.insert("streamed".to_string(), Value::Bool(true));
            m.insert(
                "total_rows_available".to_string(),
                Value::from(scan.row_count()),
            );
        }
        return Ok(out);
    }
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let dt = ctx.resolve(&source)?;
    Ok(table_to_json(
        &dt,
        ctx.resolve_row_cap(p.limit),
        ctx.cell_byte_cap,
    ))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("read_table failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
