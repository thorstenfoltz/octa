//! MCP tool: `schema_drift` - which files in a folder disagree about their
//! columns. Read-only, so it stays available under `--mcp-read-only`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use octa::data::schema_drift::{DriftOptions, analyse, collect_schemas};
use octa::formats::FormatRegistry;

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "Scan a folder of data files and report which of them disagree about their columns. Files \
are grouped by identical schema, so 500 Parquet parts come back as a handful of variants rather \
than 500 entries. Returns `has_drift`, `variants` (each with its file list and columns), \
`drifting_columns` and `skipped`. Use it when a union or dataset open failed with a type error \
that named no file.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Directory to scan.
    pub path: PathBuf,

    /// Walk subdirectories, to a depth of 8. Default `false`.
    #[serde(default)]
    pub recursive: bool,

    /// Treat column names differing only in case as one column. Default
    /// `false`.
    #[serde(default)]
    pub ignore_case: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    // The sandbox is keyed on open tabs, which are files, so a chat-side scan
    // is only allowed where the folder itself was somehow made readable. MCP
    // proper leaves `restrict_filesystem` off and this is a no-op.
    ctx.ensure_readable(&p.path)?;
    if !p.path.is_dir() {
        anyhow::bail!("{} is not a directory", p.path.display());
    }
    let registry = FormatRegistry::new();
    let (files, skipped) = collect_schemas(&p.path, p.recursive, &registry);
    if files.is_empty() {
        anyhow::bail!("no readable files in {}", p.path.display());
    }

    let mut report = analyse(
        &files,
        &DriftOptions {
            ignore_case: p.ignore_case,
            recursive: p.recursive,
        },
    );
    report.skipped = skipped;

    let variants: Vec<Value> = report
        .variants
        .iter()
        .map(|v| {
            let columns: Vec<Value> = v
                .columns
                .iter()
                .map(|c| json!({ "name": c.name, "type": c.data_type }))
                .collect();
            json!({ "files": v.files, "file_count": v.files.len(), "columns": columns })
        })
        .collect();

    let skipped_json: Vec<Value> = report
        .skipped
        .iter()
        .map(|(f, why)| json!({ "file": f, "reason": why }))
        .collect();

    let mut out = Map::new();
    out.insert("has_drift".into(), Value::Bool(report.has_drift));
    out.insert("variants".into(), Value::Array(variants));
    out.insert(
        "drifting_columns".into(),
        Value::Array(
            report
                .drifting_columns
                .iter()
                .map(|c| Value::String(c.clone()))
                .collect(),
        ),
    );
    out.insert("skipped".into(), Value::Array(skipped_json));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("schema_drift failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
