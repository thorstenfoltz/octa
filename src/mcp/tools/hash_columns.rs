//! MCP tool: `hash_columns` - append one column holding an MD5 / SHA-256 /
//! SHA-512 hex digest of chosen columns, and write the result. A write tool
//! (dropped under `--mcp-read-only`). Same engine as the GUI dialog and
//! `octa --hash-columns`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::transform::hash_columns::{HashColumnsAlgo, HashColumnsSpec, add_hash_column};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, read_with_registry};

pub const DESCRIPTION: &str = "Append one column holding a hash of chosen columns, row by row, \
and write the result. `columns` are names, joined in that order (order changes the hash). Every \
value is turned into text first, whatever its type. `algo` is `md5` (default), `sha256` or \
`sha512`; the digest is lowercase hex. `delimiter` goes between the values (default `|`, may be \
empty), `null_text` stands in for a NULL cell (default empty), `trim` strips whitespace and \
`upper` upper-cases each value before hashing. `new_column` names the result (default \
`hash_<columns>`). Writes to `output_path` (default: overwrite `path`); the format follows its \
extension. Database files are not valid sources or targets.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the source file.
    pub path: PathBuf,
    /// Column names, in the order they are joined.
    pub columns: Vec<String>,
    /// `md5` (default), `sha256` or `sha512`.
    #[serde(default)]
    pub algo: HashColumnsAlgo,
    /// Put between the values. Default `|`; may be empty.
    #[serde(default)]
    pub delimiter: Option<String>,
    /// Stands in for a NULL cell. Default empty.
    #[serde(default)]
    pub null_text: String,
    /// Strip leading and trailing whitespace from each value.
    #[serde(default)]
    pub trim: bool,
    /// Upper-case each value before hashing.
    #[serde(default)]
    pub upper: bool,
    /// Name of the new column. Default `hash_<columns>`. Must not exist yet.
    #[serde(default)]
    pub new_column: Option<String>,
    /// Where to write the result. Defaults to `path` (overwrite in place).
    #[serde(default)]
    pub output_path: Option<PathBuf>,
    /// Lift the streaming initial-load cap so every row is read and hashed.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    ctx.ensure_readable(&p.path)?;
    let mut table = read_with_registry(&p.path, None)?;
    if table.db_meta.is_some() {
        anyhow::bail!("database files (SQLite/DuckDB/GeoPackage) are not valid sources");
    }
    // A capped read would write a file holding only the rows that were read.
    if table.is_partial() {
        anyhow::bail!(
            "only part of {} was read; pass unlimited: true to hash every row",
            p.path.display()
        );
    }
    let spec = HashColumnsSpec {
        columns: Vec::new(),
        algo: p.algo,
        delimiter: p.delimiter.clone().unwrap_or_else(|| "|".to_string()),
        null_text: p.null_text.clone(),
        trim: p.trim,
        upper: p.upper,
    };
    let name = add_hash_column(&mut table, &p.columns, spec, p.new_column.as_deref())?;

    let requested = p.output_path.clone().unwrap_or_else(|| p.path.clone());
    let target = ctx.write_transformed(&requested, &table)?;
    let rows_written = table.row_count();

    let mut out = Map::new();
    out.insert("rows_written".to_string(), Value::from(rows_written));
    out.insert("new_column".to_string(), Value::String(name));
    out.insert("output".to_string(), Value::String(target));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("hash_columns failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
