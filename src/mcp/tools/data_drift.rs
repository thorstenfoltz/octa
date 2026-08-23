//! MCP tool: `data_drift` - how two versions of the same dataset differ.
//!
//! Read-only, so it stays available under `--mcp-read-only`. Wraps the pure
//! `octa::data::drift` engine, which in turn reads everything out of the same
//! Summary pass the GUI's Summary tab uses.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use octa::data::drift::{DEFAULT_CATEGORY_CAP, DriftOptions, compare_profiles, parse_thresholds};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Compare two versions of the same dataset and report how it moved: columns added or \
removed, and per shared column the change in null rate, distinct count and (for numeric columns) \
minimum, maximum and mean. Columns with few distinct values also report which category values \
appeared and vanished. Use it to answer whether today's extract still looks like yesterday's, \
before trusting a load. Pass `fail_on` (e.g. `null_rate:0.05,rows:0.1`) to have the response's \
`failed` flag act as a gate. Either path may be an open tab or a cloud object URL. This measures \
distributions, not rows: use `diff_tables` when you need to know which rows changed.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the earlier version (side A). Omit when `open_tab_a` is set.
    /// May be a cloud object URL (`s3://`, `az://`, `gs://`).
    #[serde(default)]
    pub path_a: PathBuf,

    /// Path to the later version (side B). Omit when `open_tab_b` is set. May
    /// be a cloud object URL (`s3://`, `az://`, `gs://`).
    #[serde(default)]
    pub path_b: PathBuf,

    /// Operate on an open GUI tab for side A (name, or `@active`).
    #[serde(default)]
    pub open_tab_a: Option<String>,

    /// Operate on an open GUI tab for side B (name, or `@active`).
    #[serde(default)]
    pub open_tab_b: Option<String>,

    /// For multi-table sources, the table name to read from side A.
    #[serde(default)]
    pub table_a: Option<String>,

    /// For multi-table sources, the table name to read from side B.
    #[serde(default)]
    pub table_b: Option<String>,

    /// Columns with more distinct values than this are not compared value by
    /// value. Default 50.
    #[serde(default)]
    pub category_cap: Option<usize>,

    /// Comma-separated `metric:max_relative_change` gates, for example
    /// `null_rate:0.05,rows:0.1`. Metrics are `rows`, `null_rate`,
    /// `distinct_count`, `min`, `max` and `mean`.
    #[serde(default)]
    pub fail_on: Option<String>,

    /// Lift the streaming initial-load cap for this call so every row in both
    /// files is read from disk. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let opts = DriftOptions {
        category_cap: p.category_cap.unwrap_or(DEFAULT_CATEGORY_CAP),
        thresholds: match &p.fail_on {
            Some(spec) => parse_thresholds(spec)?,
            None => Vec::new(),
        },
    };

    let a = ctx.resolve(&source_from(&p.open_tab_a, &p.path_a, &p.table_a))?;
    let b = ctx.resolve(&source_from(&p.open_tab_b, &p.path_b, &p.table_b))?;
    let report = compare_profiles(&a, &b, &opts)?;

    let drift: Vec<Value> = report
        .rows
        .iter()
        .map(|d| {
            json!({
                "column": d.column,
                "metric": d.metric,
                "before": d.before,
                "after": d.after,
                "before_text": d.before_text,
                "after_text": d.after_text,
                "change": d.relative_change,
                "breached": d.breached,
            })
        })
        .collect();

    let mut out = Map::new();
    out.insert("rows_before".into(), Value::from(report.rows_before));
    out.insert("rows_after".into(), Value::from(report.rows_after));
    out.insert(
        "added_columns".into(),
        Value::Array(
            report
                .added_columns
                .iter()
                .map(|c| Value::String(c.clone()))
                .collect(),
        ),
    );
    out.insert(
        "removed_columns".into(),
        Value::Array(
            report
                .removed_columns
                .iter()
                .map(|c| Value::String(c.clone()))
                .collect(),
        ),
    );
    out.insert("failed".into(), Value::Bool(report.failed));
    out.insert("drift".into(), Value::Array(drift));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("data_drift failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
