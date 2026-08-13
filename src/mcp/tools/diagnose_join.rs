//! MCP tool: `diagnose_join` - why do these two key columns not join?

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Explain why a join between two key columns returns fewer rows than expected. Returns \
matched and unmatched key counts per side, sample unmatched values, and which single \
normalisation would increase the match: trim_whitespace, ignore_case, collapse_whitespace, \
strip_punctuation or strip_leading_zeros. A fix is reported only when it strictly beats the \
current match count, so an empty `fixes` list means the columns genuinely hold different \
things. Counts are over distinct keys, not rows, and are sampled (`sample`, default 10000 \
rows per side). Use after suggest_join_keys and before join_tables. Read-only: diagnoses \
only, changes neither table.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Left-hand file. Combine with `open_tab_a` (one of the two).
    #[serde(default)]
    pub path: PathBuf,

    /// Left-hand open GUI tab (tab name, or `@active`).
    #[serde(default)]
    pub open_tab_a: Option<String>,

    /// Key column on the left, by name.
    pub left_column: String,

    /// Right-hand file. Combine with `open_tab_b` (one of the two).
    #[serde(default)]
    pub path_b: PathBuf,

    /// Right-hand open GUI tab (tab name, or `@active`).
    #[serde(default)]
    pub open_tab_b: Option<String>,

    /// Key column on the right, by name.
    pub right_column: String,

    /// Rows sampled per side. Default 10000.
    #[serde(default)]
    pub sample: Option<usize>,
}

/// Resolve a column name to its index, with a message that lists what is
/// actually there. A silent index-0 fallback would produce a confident,
/// meaningless diagnosis.
fn column_index(table: &octa::data::DataTable, name: &str, side: &str) -> anyhow::Result<usize> {
    table
        .columns
        .iter()
        .position(|c| c.name == name)
        .ok_or_else(|| {
            let have: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
            anyhow::anyhow!("{side} column {name:?} not found; available: {have:?}")
        })
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let left = ctx.resolve(&source_from(&p.open_tab_a, &p.path, &None))?;
    let right = ctx.resolve(&source_from(&p.open_tab_b, &p.path_b, &None))?;

    let lc = column_index(&left, &p.left_column, "left")?;
    let rc = column_index(&right, &p.right_column, "right")?;

    let sample = p
        .sample
        .unwrap_or(octa::data::join_keys::DEFAULT_SAMPLE_ROWS);
    let d = octa::data::join_diag::diagnose(&left, lc, &right, rc, sample);

    Ok(json!({
        "left_rows": d.left_rows,
        "right_rows": d.right_rows,
        "distinct_left": d.distinct_left,
        "distinct_right": d.distinct_right,
        "matched_left": d.matched_left,
        "matched_right": d.matched_right,
        "unmatched_left": d.unmatched_left,
        "unmatched_right": d.unmatched_right,
        "fixes": d.fixes.iter().map(|f| json!({
            "kind": f.kind.id(),
            "would_match": f.would_match,
        })).collect::<Vec<_>>(),
        "capped": d.capped,
        "sampled_rows_per_side": sample,
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("diagnose_join failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
