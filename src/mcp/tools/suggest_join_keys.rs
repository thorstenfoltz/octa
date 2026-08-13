//! MCP tool: `suggest_join_keys` - which columns of these tables would join?

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Rank the column pairs that would actually join two or more tables, by how much their \
values overlap, weighted by distinctness so a status column cannot outrank a real key. Use before \
join_tables when the key columns have different names or are unknown. Takes `paths` and/or \
`open_tabs` (two or more sources in total). Returns `candidates` best first, each naming both \
sides plus `overlap`, `left_distinct`, `right_distinct` and `score`. Sampled (`sample`, default \
10000 rows per table), so a high overlap is strong evidence rather than proof. Read-only.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Files to compare. Combine with `open_tabs`; two sources in total.
    #[serde(default)]
    pub paths: Vec<PathBuf>,

    /// Open GUI tabs to include (tab name, or `@active`).
    #[serde(default)]
    pub open_tabs: Vec<String>,

    /// Rows sampled per table. Default 10000.
    #[serde(default)]
    pub sample: Option<usize>,

    /// Maximum candidates to return. Default 20.
    #[serde(default)]
    pub limit: Option<usize>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let mut tables = Vec::new();
    let mut labels = Vec::new();
    for path in &p.paths {
        tables.push(ctx.resolve(&source_from(&None, path, &None))?);
        labels.push(path.display().to_string());
    }
    for tab in &p.open_tabs {
        tables.push(ctx.resolve(&source_from(&Some(tab.clone()), &PathBuf::new(), &None))?);
        labels.push(tab.clone());
    }
    if tables.len() < 2 {
        anyhow::bail!("suggest_join_keys needs at least two tables (paths and/or open_tabs)");
    }

    let refs: Vec<&octa::data::DataTable> = tables.iter().collect();
    let sample = p
        .sample
        .unwrap_or(octa::data::join_keys::DEFAULT_SAMPLE_ROWS);
    let limit = p.limit.unwrap_or(20);

    let candidates: Vec<Value> = octa::data::join_keys::suggest_keys(&refs, sample)
        .into_iter()
        .take(limit)
        .map(|k| {
            json!({
                "left_table": labels[k.left.0],
                "left_column": refs[k.left.0].columns[k.left.1].name,
                "right_table": labels[k.right.0],
                "right_column": refs[k.right.0].columns[k.right.1].name,
                "overlap": k.overlap,
                "left_distinct": k.left_distinct,
                "right_distinct": k.right_distinct,
                "score": k.score,
            })
        })
        .collect();

    Ok(json!({
        "candidates": candidates,
        "sampled_rows_per_table": sample,
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("suggest_join_keys failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
