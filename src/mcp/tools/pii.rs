//! MCP tool: `detect_pii` - scan a table for likely PII columns and suggest
//! anonymisation rules. Combines column-name and cell-value signals.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::pii::{scan_pii, suggested_anon_rules};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Detect likely PII columns in a tabular file or open tab. Combines \
the column header (email/name/gender/country/birthdate/...) with the cell values (email, phone, \
IP, credit card, IBAN, SSN, date, postal code patterns), sampling up to `sample_rows` rows per \
column (default 500). Returns `{findings: [{column, kind, confidence, by_name, value_match}, \
...], suggested_rules: [...]}`. `confidence` (0..1): value_match>=0.6 -> value_match (+0.2 if \
the header also matches); else if the header matches -> 0.6+0.4*value_match; else value_match. \
Reported when >= 0.5. `suggested_rules` are default anonymise rules (full SHA-256 hash) ready \
to pass to the `anonymize` tool.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to scan.
    #[serde(default)]
    pub table: Option<String>,

    /// Maximum rows to sample per column for pattern matching (default 500).
    #[serde(default)]
    pub sample_rows: Option<usize>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let table = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let sample = p.sample_rows.unwrap_or(500);

    let findings = scan_pii(&table, sample);
    let rules = suggested_anon_rules(&findings);

    let findings_json: Vec<Value> = findings
        .iter()
        .map(|f| {
            let col_name = table
                .columns
                .get(f.column)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let mut m = Map::new();
            m.insert("column".to_string(), Value::String(col_name));
            m.insert("kind".to_string(), Value::String(f.kind.id().to_string()));
            m.insert("confidence".to_string(), Value::from(f.confidence));
            m.insert("by_name".to_string(), Value::Bool(f.by_name));
            m.insert("value_match".to_string(), Value::from(f.value_match));
            Value::Object(m)
        })
        .collect();

    // AnonRule derives Serialize, so we can convert directly.
    let rules_json = serde_json::to_value(&rules).unwrap_or(Value::Array(Vec::new()));

    let mut out = Map::new();
    out.insert("findings".to_string(), Value::Array(findings_json));
    out.insert("suggested_rules".to_string(), rules_json);
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("detect_pii failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
