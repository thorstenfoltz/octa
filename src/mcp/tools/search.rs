//! MCP tool: `search` - find cells matching a query across every column.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::SearchMode;
use octa::data::multi_search::search_table;
use octa::data::search::RowMatcher;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Search every cell of a tabular file or open tab for a query string. `mode` is `plain` \
(case-insensitive substring, default), `wildcard` (`*`/`?`), or `regex`. Returns `hits` as \
`{row, col, column_name, snippet}` plus `hit_count`. `limit` caps hits (0 = unlimited).";

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
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

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

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    if p.query.trim().is_empty() {
        anyhow::bail!("query must not be empty");
    }
    let dt = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let matcher = RowMatcher::new(&p.query, p.mode.to_search_mode());
    if matches!(matcher, RowMatcher::Invalid) {
        anyhow::bail!("invalid regex / wildcard pattern: {}", p.query);
    }
    let hits = search_table(&dt, &matcher, "search", None, None, SNIPPET_CHARS);

    let total = hits.len();
    let emit = match ctx.resolve_row_cap(p.limit) {
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
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("search failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
