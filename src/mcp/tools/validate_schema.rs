//! MCP tool: `validate_against_schema` - check a file's column schema
//! against a JSON Schema (e.g. one exported by `export_schema --target
//! json-schema`).
//!
//! The schema can come from disk (`schema_path`) or inline
//! (`schema_inline`). Exactly one of the two must be provided.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::validate_schema::{ValidationReport, validate_against_json_schema};

use crate::mcp::OctaMcpServer;

use super::compare_schemas::diff_to_json;
use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Validate a tabular source's column schema against an expected JSON Schema (typically one \
from `export_schema --target json-schema`). Returns `matches`, a full `diff`, and \
`unparsed_types`. Provide the expected schema via `schema_path` OR `schema_inline` (exactly \
one).";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file whose schema is being validated. Omit when `open_tab`
    /// is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to inspect.
    #[serde(default)]
    pub table: Option<String>,

    /// Path to a JSON Schema file (typically one produced by
    /// `export_schema --target json-schema`). Exactly one of
    /// `schema_path` / `schema_inline` must be provided.
    #[serde(default)]
    pub schema_path: Option<PathBuf>,

    /// Inline JSON Schema string. Exactly one of `schema_path` /
    /// `schema_inline` must be provided.
    #[serde(default)]
    pub schema_inline: Option<String>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.schema_path.is_some() == p.schema_inline.is_some() {
        anyhow::bail!("exactly one of `schema_path` or `schema_inline` must be provided");
    }
    let dt = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let schema_text = match (&p.schema_path, &p.schema_inline) {
        (Some(sp), None) => std::fs::read_to_string(sp)
            .map_err(|e| anyhow::anyhow!("read schema_path {}: {e}", sp.display()))?,
        (None, Some(s)) => s.clone(),
        _ => unreachable!("xor checked above"),
    };
    let report = validate_against_json_schema(&dt.columns, &schema_text)?;
    Ok(report_to_json(&report))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("validate_schema failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}

fn report_to_json(report: &ValidationReport) -> Value {
    let mut out = Map::new();
    out.insert("matches".to_string(), Value::Bool(report.matches));
    out.insert("diff".to_string(), diff_to_json(&report.diff));
    let unparsed: Vec<Value> = report
        .unparsed_types
        .iter()
        .map(|s| Value::String(s.clone()))
        .collect();
    out.insert("unparsed_types".to_string(), Value::Array(unparsed));
    Value::Object(out)
}
