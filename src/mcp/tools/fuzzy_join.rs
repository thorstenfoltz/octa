//! MCP tool: `fuzzy_join` - join sources on similarity rather than equality.
//!
//! Read-only, so it stays available under `--mcp-read-only`. The engine is
//! `octa::data::fuzzy_join`, shared with the CLI flag and the GUI dialog.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::DataTable;
use octa::data::fuzzy_duplicates::{NormalizeOpts, SimilarityMethod};
use octa::data::fuzzy_join::{FuzzyJoinStep, fuzzy_join};
use octa::data::join::JoinType;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Join sources on how similar their values are rather than on exact \
equality, for tables that name the same thing differently (\"Mueller GmbH\" against \"Mueller \
Gmbh.\"). Each entry in `sources` has a `path` or `open_tab`; they are folded left to right. \
`on` gives the column pairs to compare as \"LEFT=RIGHT\" (repeat to average several columns). \
`method` is edit_ratio (default), jaro_winkler or token_set; `threshold` is the minimum average \
score in 0..1 (default 0.85); `block` (\"LEFT=RIGHT\") restricts comparison to rows that agree \
exactly on those columns, which is what makes a large join feasible. Each left row keeps its \
single best partner. The result adds match_score_N and ambiguous_N per step: ambiguous means \
the runner-up scored nearly as well, so that match is the one to check. Use suggest_join_keys \
first if you do not know which columns to compare.";

/// One entry in the `sources` list: a file path or an open GUI tab.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SourceParam {
    /// Path to a file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Address an open GUI tab by name or handle (e.g. `@active`). Omit when
    /// `path` is set.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources (SQLite, DuckDB), the inner table name.
    #[serde(default)]
    pub table: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Two or more sources, folded left to right.
    pub sources: Vec<SourceParam>,

    /// Column pairs to compare, each `"LEFT=RIGHT"`. Several pairs are
    /// averaged into one score.
    pub on: Vec<String>,

    /// Similarity measure: `edit_ratio` (default), `jaro_winkler`,
    /// `token_set`.
    #[serde(default)]
    pub method: Option<String>,

    /// Minimum average score for a match, in `0.0..=1.0`. Default `0.85`.
    #[serde(default)]
    pub threshold: Option<f64>,

    /// Exact-match blocking columns, `"LEFT=RIGHT"`. Only rows agreeing here
    /// are compared.
    #[serde(default)]
    pub block: Option<String>,

    /// Join type: `left` (default), `inner`, `right`, `full`.
    #[serde(default)]
    pub how: Option<String>,

    /// Rows considered per side. Default `20000`.
    #[serde(default)]
    pub max_rows: Option<usize>,

    /// Maximum rows to return in the response. Default is the server's
    /// configured limit. Pass `0` for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Lift the streaming initial-load cap so every row in all source files
    /// is read from disk. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

fn parse_method(s: Option<&str>) -> anyhow::Result<SimilarityMethod> {
    match s.unwrap_or("edit_ratio").to_ascii_lowercase().as_str() {
        "edit_ratio" | "edit" => Ok(SimilarityMethod::EditRatio),
        "jaro_winkler" | "jaro" => Ok(SimilarityMethod::JaroWinkler),
        "token_set" | "token" => Ok(SimilarityMethod::TokenSet),
        other => anyhow::bail!(
            "unknown method \"{other}\"; expected edit_ratio, jaro_winkler or token_set"
        ),
    }
}

fn parse_join_type(how: Option<&str>) -> anyhow::Result<JoinType> {
    match how.unwrap_or("left").to_ascii_lowercase().as_str() {
        "left" => Ok(JoinType::Left),
        "inner" => Ok(JoinType::Inner),
        "right" => Ok(JoinType::Right),
        "full" => Ok(JoinType::Full),
        other => {
            anyhow::bail!("unknown join type \"{other}\"; expected left, inner, right, or full")
        }
    }
}

/// Resolve `LEFT=RIGHT` against two tables, naming whichever side is missing.
fn resolve_pair(spec: &str, left: &DataTable, right: &DataTable) -> anyhow::Result<(usize, usize)> {
    let Some((l, r)) = spec.split_once('=') else {
        anyhow::bail!("expected \"LEFT=RIGHT\", got \"{spec}\"");
    };
    Ok((
        column_index(left, l.trim())?,
        column_index(right, r.trim())?,
    ))
}

fn column_index(t: &DataTable, name: &str) -> anyhow::Result<usize> {
    t.columns
        .iter()
        .position(|c| c.name == name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "column \"{name}\" not found; available: {}",
                t.columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.sources.len() < 2 {
        anyhow::bail!("fuzzy_join needs at least two sources");
    }
    if p.on.is_empty() {
        anyhow::bail!("fuzzy_join needs at least one column pair in `on`");
    }

    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let snaps: Vec<DataTable> = p
        .sources
        .iter()
        .map(|s| ctx.resolve(&source_from(&s.open_tab, &s.path, &s.table)))
        .collect::<anyhow::Result<_>>()?;

    let method = parse_method(p.method.as_deref())?;
    let how = parse_join_type(p.how.as_deref())?;
    let threshold = p.threshold.unwrap_or(0.85);
    let max_rows = p.max_rows.unwrap_or(20_000);

    let mut steps = Vec::new();
    for i in 0..snaps.len() - 1 {
        let (left, right) = (&snaps[i], &snaps[i + 1]);
        let pairs =
            p.on.iter()
                .map(|spec| resolve_pair(spec, left, right))
                .collect::<anyhow::Result<Vec<_>>>()?;
        let block = match &p.block {
            Some(spec) => Some(resolve_pair(spec, left, right)?),
            None => None,
        };
        steps.push(FuzzyJoinStep {
            pairs,
            method,
            threshold,
            normalize: NormalizeOpts::default(),
            block,
            how,
            max_rows,
        });
    }

    let refs: Vec<&DataTable> = snaps.iter().collect();
    let result = fuzzy_join(&refs, &steps, &AtomicBool::new(false))?;

    let row_cap = ctx.resolve_row_cap(p.limit);
    let mut out = table_to_json(&result.table, row_cap, ctx.cell_byte_cap);
    if let Some(obj) = out.as_object_mut() {
        obj.insert(
            "steps".into(),
            Value::Array(
                result
                    .steps
                    .iter()
                    .map(|s| {
                        json!({
                            "left_rows": s.left_rows,
                            "right_rows": s.right_rows,
                            "matched": s.matched,
                            "ambiguous": s.ambiguous,
                            "capped": s.capped,
                        })
                    })
                    .collect(),
            ),
        );
    }
    Ok(out)
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("fuzzy_join failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
