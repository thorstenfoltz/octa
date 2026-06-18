//! MCP tool: `drop_duplicates` - remove duplicate rows by key columns.
//!
//! The source is resolved through the shared `ToolContext` (a file path or an
//! open GUI tab). Deduplication is handled by `octa::data::dedupe::dedupe_rows`,
//! which keeps the first or last occurrence of each key and preserves original
//! row order in the output.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use octa::data::dedupe::{KeepWhich, dedupe_rows};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Remove duplicate rows from a tabular file or open tab. `on` lists the column \
     names whose combined value forms the duplicate key; omit it (or pass an empty \
     list) to deduplicate on all columns (whole-row equality). `keep` controls which \
     occurrence to retain: `first` (default) keeps the earliest, `last` keeps the \
     latest. Surviving rows are returned in original order. Returns the same \
     `{schema, rows, row_count, truncated, ...}` shape as `read_table`.";

/// Parameters for `drop_duplicates`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to deduplicate.
    #[serde(default)]
    pub table: Option<String>,

    /// Column names that form the duplicate key. Omit or pass an empty list
    /// to deduplicate on all columns (whole-row equality).
    #[serde(default)]
    pub on: Option<Vec<String>>,

    /// Which occurrence to keep: `first` (default) or `last`.
    #[serde(default)]
    pub keep: Option<String>,

    /// Maximum rows to return in the response. Default is the server's
    /// configured limit. Pass `0` for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Lift the streaming initial-load cap so every row in the source file
    /// is read from disk. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

fn parse_keep(keep: Option<&str>) -> anyhow::Result<KeepWhich> {
    match keep.unwrap_or("first").to_ascii_lowercase().as_str() {
        "first" => Ok(KeepWhich::First),
        "last" => Ok(KeepWhich::Last),
        other => anyhow::bail!("unknown keep value \"{other}\"; expected first or last"),
    }
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let dt = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;

    // Resolve column names to indices.
    let key_cols: Vec<usize> = match &p.on {
        Some(names) if !names.is_empty() => {
            let mut indices = Vec::with_capacity(names.len());
            for name in names {
                let idx = dt
                    .columns
                    .iter()
                    .position(|c| &c.name == name)
                    .ok_or_else(|| anyhow::anyhow!("no such column: {name}"))?;
                indices.push(idx);
            }
            indices
        }
        // Empty / absent -> whole-row key.
        _ => vec![],
    };

    let keep = parse_keep(p.keep.as_deref())?;
    let out = dedupe_rows(&dt, &key_cols, keep);

    let row_cap = ctx.resolve_row_cap(p.limit);
    let cell_cap = ctx.cell_byte_cap;
    Ok(table_to_json(&out, row_cap, cell_cap))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("drop_duplicates failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
