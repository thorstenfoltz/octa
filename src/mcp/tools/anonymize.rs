//! MCP tool: `anonymize` - mask / scramble chosen columns of a file and write
//! the sanitised result. A write tool (dropped under `--mcp-read-only`).

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::transform::{NamedAnonRule, anonymize_by_name};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, read_with_registry};

pub const DESCRIPTION: &str = "Anonymise / mask sensitive columns of a tabular file and write the \
sanitised result. `rules` is a list of `{columns, strategy, new_column?}`; `columns` is one name \
or an array of names (two or more with a hash strategy combine them into one new column named \
`new_column`). `output` is `in_place` (default, overwrite) or `new_columns` (keep originals, \
append). `strategy` is one of: \
`{type:\"hash\", algo:\"sha256\"|\"blake3\", length?:N}` (omit length for the full 64-char \
digest; stable + join-able), \
`{type:\"partial_mask\", keep:\"first\"|\"last\", count:N, mask_char:\"*\"}`, \
`{type:\"redact\", token:{\"fixed\":\"[REDACTED]\"}|\"null\"}`, or `{type:\"fake\", \
kind:\"name\"|\"email\"|\"city\"|\"company\"|\"phone\"|\"uuid\"}`. A shared `salt` makes the \
output non-guessable; the same value+salt always maps the same way (duplicates stay linked). \
Null/empty cells pass through. Writes to `output_path` (default: overwrite `path`); the format \
follows its extension. Database files are not valid sources or targets.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the source file.
    pub path: PathBuf,

    /// Reserved: anonymising an open GUI tab is not supported here. Setting
    /// this returns an error - operate on a file path instead.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// Where to write the result. Defaults to `path` (overwrite in place).
    #[serde(default)]
    pub output_path: Option<PathBuf>,

    /// Per-column rules. Must be non-empty; every `column` must exist.
    pub rules: Vec<NamedAnonRule>,

    /// Shared salt for all rules. Empty = plain deterministic hashing.
    #[serde(default)]
    pub salt: String,

    /// `in_place` (default) overwrites the columns; `new_columns` keeps the
    /// originals and appends the anonymised values as new columns.
    #[serde(default)]
    pub output: Option<String>,

    /// Lift the streaming initial-load cap so every row is read + rewritten.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.open_tab.is_some() {
        anyhow::bail!("anonymising an open tab is not supported; operate on a file path");
    }
    if p.rules.is_empty() {
        anyhow::bail!("rules must not be empty");
    }
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    ctx.ensure_readable(&p.path)?;
    let mut table = read_with_registry(&p.path, None)?;
    if table.db_meta.is_some() {
        anyhow::bail!("database files (SQLite/DuckDB/GeoPackage) are not valid sources");
    }

    let new_columns = matches!(p.output.as_deref(), Some("new_columns"));

    let columns_anonymized = anonymize_by_name(&mut table, &p.rules, &p.salt, new_columns)?;

    // Resolve + validate output.
    let requested = p.output_path.clone().unwrap_or_else(|| p.path.clone());
    let target = ctx.write_transformed(&requested, &table)?;
    let rows_written = table.row_count();

    let mut out = Map::new();
    out.insert("rows_written".to_string(), Value::from(rows_written));
    out.insert(
        "columns_anonymized".to_string(),
        Value::from(columns_anonymized),
    );
    out.insert("output".to_string(), Value::String(target));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("anonymize failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
