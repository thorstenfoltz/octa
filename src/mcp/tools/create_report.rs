//! MCP tool: `create_report` - write an HTML profiling report for a table.
//!
//! A **write** tool: it is in `WRITE_TOOL_NAMES`, removed under
//! `--mcp-read-only`, and bails on a read-only context.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::report::{ReportOptions, ReportSection, build_report};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Write a self-contained HTML profiling report for a table: \
per-column statistics, distribution charts, the most common values and a correlation matrix. \
The document embeds its own CSS and SVG and fetches nothing, so it can be mailed or published \
as-is. `out_path` is where to write it. `sections` picks a subset from stats, distributions, \
top_values and correlation (default: all four). `sample_rows` profiles a random sample instead \
of every row, and the report states that it did.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the source file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to read.
    #[serde(default)]
    pub table: Option<String>,

    /// Where to write the report. Must end in `.html`.
    pub out_path: String,

    /// Title shown at the top of the document and in the browser tab.
    /// Defaults to the source file's name.
    #[serde(default)]
    pub title: Option<String>,

    /// Sections to include: `stats`, `distributions`, `top_values`,
    /// `correlation`. Default: all four.
    #[serde(default)]
    pub sections: Option<Vec<String>>,

    /// Profile a random sample of this many rows instead of every row. The
    /// report states that it sampled.
    #[serde(default)]
    pub sample_rows: Option<usize>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if ctx.read_only {
        anyhow::bail!("create_report writes files and this server is read-only");
    }
    let out = ctx.resolve_write_path(std::path::Path::new(&p.out_path))?;

    let sections = match &p.sections {
        None => ReportSection::ALL.to_vec(),
        Some(list) => {
            let mut picked = Vec::new();
            for name in list {
                match ReportSection::parse(name) {
                    Some(s) => picked.push(s),
                    None => anyhow::bail!(
                        "unknown report section {name:?}; valid: stats, distributions, top_values, correlation"
                    ),
                }
            }
            picked
        }
    };

    let source = source_from(&p.open_tab, &p.path, &p.table);
    let table = ctx.resolve(&source)?;

    let title = p.title.clone().unwrap_or_else(|| {
        p.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Data report".to_string())
    });

    let rows: Vec<usize> = (0..table.row_count()).collect();
    let html = build_report(
        &table,
        &rows,
        &ReportOptions {
            sections,
            sample_rows: p.sample_rows,
            title,
            ..ReportOptions::default()
        },
        &AtomicBool::new(false),
    )?;

    let bytes = html.len();
    std::fs::write(&out, html)?;
    Ok(json!({ "path": out.display().to_string(), "bytes": bytes }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("create_report failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
