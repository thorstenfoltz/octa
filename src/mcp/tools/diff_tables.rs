//! MCP tool: `diff_tables` - data comparison of two files.
//!
//! Reads both sources through the shared registry (or open tabs) and compares
//! them in one of three modes (`set`, `ordered`, `join`):
//! * `set` (default) delegates to `octa::data::diff::diff_rows` (whole-row
//!   membership). Response carries `only_in_a` / `only_in_b` + `shared_keys`.
//! * `ordered` / `join` use `octa::data::compare`. Response additionally
//!   carries `changed_a` / `changed_b` (the differing rows, parallel order),
//!   a `changed` array naming the differing columns per matched pair, and
//!   `changed_count` / `unchanged_count`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::compare::{self, CompareMode};
use octa::data::diff::diff_rows;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Compare two tabular sources (files or open tabs). `mode` picks the strategy: \
`set` (default) reports whole rows unique to each side (`only_in_a`/`only_in_b` + `shared_keys`); \
`ordered` compares row-by-row in order and reports cell-level changes; `join` matches rows on the \
`on` key column(s) and reports added/removed/changed rows. For `ordered`/`join` the response also \
carries `changed_a`/`changed_b` (the differing rows, parallel order), a `changed` array naming the \
differing columns per pair, and `changed_count`/`unchanged_count`. `limit` caps rows per side \
(0 = unlimited). Run `compare_schemas` first if the column layouts might differ.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the first file (side A). Omit when `open_tab_a` is set.
    #[serde(default)]
    pub path_a: PathBuf,

    /// Path to the second file (side B). Omit when `open_tab_b` is set.
    #[serde(default)]
    pub path_b: PathBuf,

    /// Operate on an open GUI tab for side A (name, or `@active`).
    #[serde(default)]
    pub open_tab_a: Option<String>,

    /// Operate on an open GUI tab for side B (name, or `@active`).
    #[serde(default)]
    pub open_tab_b: Option<String>,

    /// For multi-table sources, the table name to read from file A.
    #[serde(default)]
    pub table_a: Option<String>,

    /// For multi-table sources, the table name to read from file B.
    #[serde(default)]
    pub table_b: Option<String>,

    /// Comparison mode: `set` (default), `ordered`, or `join`.
    #[serde(default)]
    pub mode: Option<String>,

    /// Key column(s) for `mode: "join"` (matched by name).
    #[serde(default)]
    pub on: Option<Vec<String>>,

    /// Maximum rows to return *per side*. Default is the server's configured
    /// limit. Pass 0 for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Lift the streaming initial-load cap for this call so every row in both
    /// files is read from disk. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let a = ctx.resolve(&source_from(&p.open_tab_a, &p.path_a, &p.table_a))?;
    let b = ctx.resolve(&source_from(&p.open_tab_b, &p.path_b, &p.table_b))?;

    let mode = match p.mode.as_deref() {
        None => CompareMode::Set,
        Some(s) => CompareMode::parse(s)
            .ok_or_else(|| anyhow::anyhow!("mode must be one of: set, ordered, join"))?,
    };

    let row_cap = ctx.resolve_row_cap(p.limit);
    let cell_cap = ctx.cell_byte_cap;

    let mut out = Map::new();
    out.insert("mode".to_string(), Value::from(mode.as_str()));

    match mode {
        CompareMode::Set => {
            let diff = diff_rows(&a, &b);
            let a_sub = compare::subset(&a, &diff.only_in_a);
            let b_sub = compare::subset(&b, &diff.only_in_b);
            out.insert(
                "only_in_a".to_string(),
                table_to_json(&a_sub, row_cap, cell_cap),
            );
            out.insert(
                "only_in_b".to_string(),
                table_to_json(&b_sub, row_cap, cell_cap),
            );
            out.insert(
                "only_in_a_count".to_string(),
                Value::from(diff.only_in_a.len()),
            );
            out.insert(
                "only_in_b_count".to_string(),
                Value::from(diff.only_in_b.len()),
            );
            out.insert("shared_keys".to_string(), Value::from(diff.shared_keys));
        }
        CompareMode::Ordered | CompareMode::Join => {
            let result = match mode {
                CompareMode::Ordered => compare::compare_ordered(&a, &b),
                CompareMode::Join => {
                    let on = p.on.clone().unwrap_or_default();
                    compare::compare_join(&a, &b, &on)?
                }
                CompareMode::Set => unreachable!(),
            };
            let a_only = compare::subset(&a, &result.only_in_a);
            let b_only = compare::subset(&b, &result.only_in_b);
            let a_changed_idx: Vec<usize> = result.changed.iter().map(|c| c.row_a).collect();
            let b_changed_idx: Vec<usize> = result.changed.iter().map(|c| c.row_b).collect();
            let a_changed = compare::subset(&a, &a_changed_idx);
            let b_changed = compare::subset(&b, &b_changed_idx);

            out.insert(
                "only_in_a".to_string(),
                table_to_json(&a_only, row_cap, cell_cap),
            );
            out.insert(
                "only_in_b".to_string(),
                table_to_json(&b_only, row_cap, cell_cap),
            );
            out.insert(
                "changed_a".to_string(),
                table_to_json(&a_changed, row_cap, cell_cap),
            );
            out.insert(
                "changed_b".to_string(),
                table_to_json(&b_changed, row_cap, cell_cap),
            );
            let changed: Vec<Value> = result
                .changed
                .iter()
                .map(|c| {
                    let mut m = Map::new();
                    m.insert("row_a".to_string(), Value::from(c.row_a));
                    m.insert("row_b".to_string(), Value::from(c.row_b));
                    m.insert(
                        "changed_columns".to_string(),
                        Value::from(c.changed_columns.clone()),
                    );
                    Value::Object(m)
                })
                .collect();
            out.insert("changed".to_string(), Value::Array(changed));
            out.insert(
                "only_in_a_count".to_string(),
                Value::from(result.only_in_a.len()),
            );
            out.insert(
                "only_in_b_count".to_string(),
                Value::from(result.only_in_b.len()),
            );
            out.insert(
                "changed_count".to_string(),
                Value::from(result.changed.len()),
            );
            out.insert("unchanged_count".to_string(), Value::from(result.unchanged));
        }
    }
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("diff_tables failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
