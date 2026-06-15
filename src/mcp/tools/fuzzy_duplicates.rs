//! MCP tool: `fuzzy_duplicates` - find rows that are *almost* the same on the
//! chosen columns and return them grouped into clusters with a score.
//! Read-only analytics (stays available under `--mcp-read-only`).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::fuzzy_duplicates::{
    FuzzyDupConfig, NormalizeOpts, SimilarityMethod, find_fuzzy_duplicates,
};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Find near-duplicate rows in a tabular file or open tab: rows that \
are almost the same on the chosen columns (typos, spacing, reordered words). `key_columns` are \
the columns compared (averaged). `method` is `edit_ratio` (default, typos), `jaro_winkler` \
(names), or `token_set` (word order). `threshold` is 0.0..=1.0 (default 0.85). Normalisation \
flags `lower` / `collapse_ws` / `strip_punct` default true. Optional `block_column` only compares \
rows sharing that column's exact value (makes large tables feasible). `max_rows` caps the scan \
(default 20000). Returns `clusters` (each `{rows, score}`, score = lowest linking similarity), \
`compared_rows`, and `capped`.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file (`@active` or a tab name).
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to scan.
    #[serde(default)]
    pub table: Option<String>,

    /// Columns whose values are compared (averaged). Must be non-empty.
    pub key_columns: Vec<String>,

    /// Similarity method: `edit_ratio` (default), `jaro_winkler`, `token_set`.
    #[serde(default)]
    pub method: SimilarityMethod,

    /// Match threshold, 0.0..=1.0. Default 0.85.
    #[serde(default = "default_threshold")]
    pub threshold: f64,

    /// Lowercase before comparing (default true).
    #[serde(default = "default_true")]
    pub lower: bool,
    /// Collapse whitespace before comparing (default true).
    #[serde(default = "default_true")]
    pub collapse_ws: bool,
    /// Strip punctuation before comparing (default true).
    #[serde(default = "default_true")]
    pub strip_punct: bool,

    /// Optional exact-match blocking column (only compare rows sharing its
    /// value).
    #[serde(default)]
    pub block_column: Option<String>,

    /// Cap on rows scanned. Default 20000.
    #[serde(default = "default_max_rows")]
    pub max_rows: usize,

    /// Lift the streaming initial-load cap so every row is loaded from disk.
    #[serde(default)]
    pub unlimited: bool,
}

fn default_threshold() -> f64 {
    0.85
}
fn default_true() -> bool {
    true
}
fn default_max_rows() -> usize {
    20_000
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    if p.key_columns.is_empty() {
        anyhow::bail!("key_columns must not be empty");
    }
    let dt = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;

    let resolve = |name: &str| {
        dt.columns
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| anyhow::anyhow!("no such column: {name}"))
    };
    let mut key_cols = Vec::with_capacity(p.key_columns.len());
    for name in &p.key_columns {
        key_cols.push(resolve(name)?);
    }
    let block_col = match &p.block_column {
        Some(name) => Some(resolve(name)?),
        None => None,
    };

    let cfg = FuzzyDupConfig {
        key_cols,
        method: p.method,
        threshold: p.threshold,
        normalize: NormalizeOpts {
            lower: p.lower,
            collapse_ws: p.collapse_ws,
            strip_punct: p.strip_punct,
        },
        block_col,
        max_rows: p.max_rows,
    };

    let cancel = AtomicBool::new(false);
    let res = find_fuzzy_duplicates(&dt, &cfg, &cancel);

    let clusters: Vec<Value> = res
        .clusters
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut m = Map::new();
            m.insert("cluster".to_string(), Value::from(i + 1));
            m.insert(
                "rows".to_string(),
                Value::Array(c.rows.iter().map(|&r| Value::from(r)).collect()),
            );
            m.insert("score".to_string(), Value::from(c.score));
            Value::Object(m)
        })
        .collect();

    let mut out = Map::new();
    out.insert("cluster_count".to_string(), Value::from(res.clusters.len()));
    out.insert("clusters".to_string(), Value::Array(clusters));
    out.insert("compared_rows".to_string(), Value::from(res.compared_rows));
    out.insert("capped".to_string(), Value::Bool(res.capped));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("fuzzy_duplicates failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
