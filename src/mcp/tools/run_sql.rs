//! MCP tool: `run_sql` — run a DuckDB SQL query against a loaded file.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::sql::{QueryKind, run_query};

use crate::mcp::OctaMcpServer;

use super::{read_with_registry, table_to_json};

// Tool description lives inline at the `#[tool]` site in `src/mcp/mod.rs`.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file.
    pub path: PathBuf,

    /// SQL query string. The file is exposed as `data`.
    pub query: String,

    /// Maximum rows to return. Default is the server's configured limit (1000
    /// unless changed via Octa's Settings → MCP). Pass 0 for unlimited.
    /// Slices the *response* — set `unlimited` to also lift the file-loader
    /// cap so the query sees every row.
    #[serde(default)]
    pub limit: Option<usize>,

    /// For multi-table sources, load this specific table as `data`.
    #[serde(default)]
    pub table: Option<String>,

    /// Lift the streaming initial-load cap so the query operates on every
    /// row in the file. Combine with `limit: 0` to return every result row.
    /// Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let row_cap = server.resolve_row_cap(p.limit);
    let cell_cap = server.cell_byte_cap;
    let path = p.path.clone();
    let table_name = p.table.clone();
    let query = p.query.clone();
    let unlimited = p.unlimited;
    let outcome = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let _g = unlimited.then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
        let dt = read_with_registry(&path, table_name.as_deref())?;
        run_query(&dt, &query)
    })
    .await
    .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
    .map_err(|e| McpError::invalid_params(format!("run_sql failed: {e}"), None))?;

    let kind_str = match outcome.kind {
        QueryKind::Select => "select",
        QueryKind::Mutation => "mutation",
    };
    let table_value = table_to_json(&outcome.table, row_cap, cell_cap);

    let mut out = Map::new();
    out.insert("kind".to_string(), Value::String(kind_str.to_string()));
    if let Some(n) = outcome.affected {
        out.insert("affected".to_string(), Value::from(n));
    }
    out.insert("result".to_string(), table_value);
    Ok(CallToolResult::success(vec![Content::text(
        Value::Object(out).to_string(),
    )]))
}
