//! MCP tool: `check_rules` - does this table still satisfy a rules file?
//!
//! Read-only, so it stays available under `--mcp-read-only`. Wraps the same
//! `octa::data::validation` engine the GUI paints red cells with, and the same
//! TOML rules file `--check` reads, so an agent and a CI step agree.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use octa::data::validation::rules_file::{load, resolve, to_named};
use octa::data::validation::{ValidationRule, violations};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

/// Offending values named per rule, so a failure says what is wrong.
const MAX_SAMPLES: usize = 3;

pub const DESCRIPTION: &str = "Check a table's values against a TOML rules file and report which rules failed. Rules name \
their columns, and each supports one check: `not_null`, `unique`, `range` (min/max), `regex` \
(pattern) or `max_length`. The response carries `passed`, one entry per failing rule with its \
failure count and up to three offending values, and `unknown` for rules naming columns the table \
does not have. A rule that cannot run counts as a failure, never as a pass. Use it to verify an \
extract before trusting it; the same file works from the command line as `octa --check FILE \
--rules RULES`.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the data file. Omit when `open_tab` is set. May be a cloud
    /// object URL (`s3://`, `az://`, `gs://`).
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab (name, or `@active`).
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the table name to read.
    #[serde(default)]
    pub table: Option<String>,

    /// Path to the TOML rules file.
    pub rules_path: PathBuf,
}

/// The on-disk kind name, for the report's `rule` field.
fn rule_label(rule: &ValidationRule) -> String {
    to_named(std::slice::from_ref(rule), &[])
        .rule
        .first()
        .map(|r| r.kind.clone())
        .unwrap_or_default()
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    ctx.ensure_readable(&p.rules_path)?;
    let table = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let file = load(&p.rules_path)?;
    let (resolved, unknown) = resolve(&file, &table.columns);

    let mut failures = Vec::new();
    // One pass per rule: `violations` answers for a whole set at once, so
    // attributing a failing cell back to its rule means asking one at a time.
    for rule in &resolved {
        let hits = violations(&table, std::slice::from_ref(rule));
        if hits.is_empty() {
            continue;
        }
        let mut coords: Vec<(usize, usize)> = hits.into_iter().collect();
        coords.sort_unstable();
        let samples: Vec<String> = coords
            .iter()
            .take(MAX_SAMPLES)
            .filter_map(|(r, c)| table.get(*r, *c).map(|v| v.to_string()))
            .collect();
        failures.push(json!({
            "rule": rule_label(rule),
            "column": rule
                .column
                .and_then(|i| table.columns.get(i))
                .map(|c| c.name.clone())
                .unwrap_or_default(),
            "failures": coords.len(),
            "samples": samples,
        }));
    }

    let mut out = Map::new();
    // A rule that could not run is never a pass: a renamed column would
    // otherwise turn a skipped check into a green result.
    out.insert(
        "passed".into(),
        Value::Bool(failures.is_empty() && unknown.is_empty()),
    );
    out.insert("rules_checked".into(), Value::from(resolved.len()));
    out.insert("violations".into(), Value::Array(failures));
    out.insert(
        "unknown".into(),
        Value::Array(unknown.into_iter().map(Value::String).collect()),
    );
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("check_rules failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
