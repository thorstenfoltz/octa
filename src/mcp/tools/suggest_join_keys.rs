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
sides plus `overlap`, `left_distinct`, `right_distinct`, `score`, and orphan counts BOTH ways \
round - `left_orphans` out of `left_distinct_values` and `right_orphans` out of \
`right_distinct_values`, how many distinct values on each side find no partner on the other. Two \
candidates tie exactly when both tables number their rows from 1, and only the count read from \
the CHILD side separates them, so check both. The \
orphan count is what separates two candidates that overlap identically, which happens whenever \
both tables number their rows from 1. Sampled (`sample`, default 10000 rows per table), so a high \
overlap is strong evidence rather than proof. Read-only.";

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

    // Through the relationship map rather than the raw ranking, so every
    // candidate also carries its orphan count. `min_score: 0.0` keeps the
    // ranking's own noise floor as the only filter, exactly as before.
    let named: Vec<(String, &octa::data::DataTable)> =
        labels.iter().cloned().zip(refs.iter().copied()).collect();
    let map = octa::data::rel_map::build_map(
        &named,
        &octa::data::rel_map::RelMapOptions {
            sample,
            min_score: 0.0,
        },
    );
    let candidates: Vec<Value> = map
        .edges
        .iter()
        .take(limit)
        .map(|e| {
            json!({
                "left_table": map.nodes[e.left_table].name,
                "left_column": map.nodes[e.left_table].columns[e.left_col],
                "right_table": map.nodes[e.right_table].name,
                "right_column": map.nodes[e.right_table].columns[e.right_col],
                "overlap": e.overlap,
                "left_distinct": e.left_distinct,
                "right_distinct": e.right_distinct,
                "score": e.score,
                "left_orphans": e.left_orphans,
                "left_distinct_values": e.left_distinct_values,
                "right_orphans": e.right_orphans,
                "right_distinct_values": e.right_distinct_values,
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
