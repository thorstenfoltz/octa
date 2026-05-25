//! MCP tool: `search` — find cells matching a query across every column.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::SearchMode;
use octa::data::multi_search::search_table;
use octa::data::search::RowMatcher;

use crate::mcp::OctaMcpServer;

use super::read_with_registry;

// Tool description lives inline at the `#[tool]` site in `src/mcp/mod.rs`.

/// Snippet width for each hit. Matches the active-search default.
const SNIPPET_CHARS: usize = 200;

/// Match mode for the query.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Case-insensitive substring match (default).
    #[default]
    Plain,
    /// `*` matches any run of characters, `?` matches one.
    Wildcard,
    /// Full regular expression (regex crate syntax).
    Regex,
}

impl Mode {
    fn to_search_mode(self) -> SearchMode {
        match self {
            Self::Plain => SearchMode::Plain,
            Self::Wildcard => SearchMode::Wildcard,
            Self::Regex => SearchMode::Regex,
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file.
    pub path: PathBuf,

    /// For multi-table sources, the specific table to search.
    #[serde(default)]
    pub table: Option<String>,

    /// Text or pattern to search for.
    pub query: String,

    /// Match mode: `plain` (default), `wildcard`, or `regex`.
    #[serde(default)]
    pub mode: Mode,

    /// Maximum hits to return. Default is the server's configured limit.
    /// Pass 0 for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,

    /// Lift the streaming initial-load cap so the search scans every row
    /// in the file. Without this, only the first `initial_load_rows` rows
    /// are scanned. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let row_cap = server.resolve_row_cap(p.limit);
    let path = p.path.clone();
    let table_name = p.table.clone();
    let query = p.query.clone();
    let mode = p.mode.to_search_mode();
    let unlimited = p.unlimited;

    let hits = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let _g = unlimited.then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
        if query.trim().is_empty() {
            anyhow::bail!("query must not be empty");
        }
        let dt = read_with_registry(&path, table_name.as_deref())?;
        let matcher = RowMatcher::new(&query, mode);
        if matches!(matcher, RowMatcher::Invalid) {
            anyhow::bail!("invalid regex / wildcard pattern: {query}");
        }
        Ok(search_table(
            &dt,
            &matcher,
            "search",
            None,
            None,
            SNIPPET_CHARS,
        ))
    })
    .await
    .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
    .map_err(|e| McpError::invalid_params(format!("search failed: {e}"), None))?;

    let total = hits.len();
    let emit = match row_cap {
        None => total,
        Some(n) => n.min(total),
    };
    let truncated = emit < total;

    let hit_values: Vec<Value> = hits
        .iter()
        .take(emit)
        .map(|h| {
            let mut m = Map::new();
            m.insert("row".to_string(), Value::from(h.row));
            m.insert("col".to_string(), Value::from(h.col));
            m.insert(
                "column_name".to_string(),
                Value::String(h.column_name.clone()),
            );
            m.insert("snippet".to_string(), Value::String(h.snippet.clone()));
            Value::Object(m)
        })
        .collect();

    let mut out = Map::new();
    out.insert("hit_count".to_string(), Value::from(total));
    out.insert("returned".to_string(), Value::from(emit));
    out.insert("truncated".to_string(), Value::Bool(truncated));
    out.insert("hits".to_string(), Value::Array(hit_values));
    Ok(CallToolResult::success(vec![Content::text(
        Value::Object(out).to_string(),
    )]))
}
