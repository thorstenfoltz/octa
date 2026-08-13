//! MCP tool: `batch_convert` - convert N files into one target format in a
//! single run. A **write** tool: it is in `WRITE_TOOL_NAMES`, removed under
//! `--mcp-read-only`, and refuses to run on a read-only context.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::batch_convert::{BatchStatus, plan_batch, run_batch};

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "Convert several files into one target format in a single run. \
`inputs` is the list of source paths, `out_dir` the directory the outputs go into, and `to` \
the target file extension without a dot (`parquet`, `csv`, `json`, ...). Set `overwrite` to \
replace existing outputs; by default they are skipped. Gzip and zstd inputs are decompressed \
automatically. One failed file does not stop the run: the response reports \
`{converted, failed, skipped, items}` with a per-file status. Multi-table sources convert \
their first table only. Writes files, so it is unavailable in read-only mode.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Source files to convert.
    pub inputs: Vec<PathBuf>,

    /// Directory the converted files are written into. Created if absent.
    pub out_dir: PathBuf,

    /// Target file extension without the leading dot (`parquet`, `csv`, ...).
    pub to: String,

    /// Replace outputs that already exist. Default: skip them.
    #[serde(default)]
    pub overwrite: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if ctx.read_only {
        anyhow::bail!("batch_convert writes files and this server is read-only");
    }
    // Sandbox rules are the same as every other write tool's.
    let out_dir = ctx.resolve_write_path(&p.out_dir)?;
    std::fs::create_dir_all(&out_dir)?;

    let plan = plan_batch(&p.inputs, &out_dir, &p.to, p.overwrite);
    let report = run_batch(
        plan,
        &|_, _| {},
        &AtomicBool::new(false),
        &Default::default(),
    );

    let items: Vec<Value> = report
        .items
        .iter()
        .map(|i| {
            let (status, rows, error) = match &i.status {
                BatchStatus::Pending => ("pending", None, None),
                BatchStatus::Skipped(_) => ("skipped", None, None),
                BatchStatus::Done { rows } => ("done", Some(*rows), None),
                BatchStatus::Failed(e) => ("failed", None, Some(e.clone())),
            };
            json!({
                "input": i.input.display().to_string(),
                "output": i.output.display().to_string(),
                "status": status,
                "rows": rows,
                "error": error,
            })
        })
        .collect();

    Ok(json!({
        "converted": report.converted,
        "failed": report.failed,
        "skipped": report.skipped,
        "items": items,
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("batch_convert failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
