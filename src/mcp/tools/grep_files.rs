//! MCP tool: `grep_files` - search every tabular file in a directory for a
//! value. Mirrors the GUI Multi-search "Directory" scope: one level deep,
//! per-file size cap, reusing `octa::data::multi_search::search_table`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::SearchMode;
use octa::data::multi_search::search_table;
use octa::data::search::RowMatcher;
use octa::ui::directory_tree::read_sorted_dir;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, read_with_registry};

pub const DESCRIPTION: &str = "Search every tabular file in a directory (one level deep) for a \
value, like grep across files. `query` + `mode` (`plain` default / `wildcard` / `regex`), with \
optional `case_sensitive` and `whole_word`. Skips files larger than `max_file_size_mb` (default \
50) and files no reader can parse. Returns `hits` (`{file, row, column, snippet}`), `skipped` \
(`{file, reason}`), `files_searched`, `total_hits`, and `truncated`. Caps at `max_results` \
matches overall (default 1000) and 1000 per file.";

/// Per-file hit cap, matching the GUI Multi-search.
const PER_FILE_CAP: usize = 1000;
const SNIPPET_CHARS: usize = 200;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Directory to search (one level deep; subdirectories are not recursed).
    pub dir: PathBuf,

    /// The text / pattern to search for.
    pub query: String,

    /// Match mode: `plain` (default), `wildcard` (`*`/`?`), or `regex`.
    #[serde(default)]
    pub mode: Option<String>,

    /// Case-sensitive match (default false).
    #[serde(default)]
    pub case_sensitive: bool,

    /// Whole-word match (default false).
    #[serde(default)]
    pub whole_word: bool,

    /// Skip files larger than this many megabytes (default 50).
    #[serde(default)]
    pub max_file_size_mb: Option<u64>,

    /// Cap on total matches returned (default 1000).
    #[serde(default)]
    pub max_results: Option<usize>,
}

fn parse_mode(s: Option<&str>) -> anyhow::Result<SearchMode> {
    match s.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        None | Some("plain") => Ok(SearchMode::Plain),
        Some("wildcard") => Ok(SearchMode::Wildcard),
        Some("regex") => Ok(SearchMode::Regex),
        Some(other) => anyhow::bail!("unknown mode `{other}` (use plain/wildcard/regex)"),
    }
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.query.is_empty() {
        anyhow::bail!("`query` must not be empty");
    }
    ctx.ensure_readable(&p.dir)?;
    let mode = parse_mode(p.mode.as_deref())?;
    let matcher = RowMatcher::with_options(&p.query, mode, p.case_sensitive, p.whole_word);
    if matches!(matcher, RowMatcher::Invalid) {
        anyhow::bail!("invalid search pattern");
    }

    let size_cap = p.max_file_size_mb.unwrap_or(50).saturating_mul(1024 * 1024);
    let total_cap = match p.max_results {
        Some(0) => usize::MAX,
        Some(n) => n,
        None => 1000,
    };

    let entries = read_sorted_dir(&p.dir)
        .map_err(|e| anyhow::anyhow!("cannot read directory {}: {e}", p.dir.display()))?;

    let mut hits: Vec<Value> = Vec::new();
    let mut skipped: Vec<Value> = Vec::new();
    let mut files_searched = 0usize;
    let mut truncated = false;

    for path in entries {
        if !path.is_file() {
            continue;
        }
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        // Chat sandbox: silently skip files the agent may not read.
        if ctx.ensure_readable(&path).is_err() {
            continue;
        }
        if let Ok(meta) = std::fs::metadata(&path)
            && size_cap > 0
            && meta.len() > size_cap
        {
            skipped.push(skip_entry(&label, "oversized"));
            continue;
        }

        let table = match read_with_registry(&path, None) {
            Ok(t) => t,
            Err(_) => {
                skipped.push(skip_entry(&label, "parse_error"));
                continue;
            }
        };
        files_searched += 1;

        let file_hits = search_table(
            &table,
            &matcher,
            &label,
            Some(path.clone()),
            None,
            SNIPPET_CHARS,
        );
        for h in file_hits.into_iter().take(PER_FILE_CAP) {
            if hits.len() >= total_cap {
                truncated = true;
                break;
            }
            let mut m = Map::new();
            m.insert("file".to_string(), Value::String(label.clone()));
            m.insert("row".to_string(), Value::from(h.row));
            m.insert("column".to_string(), Value::String(h.column_name));
            m.insert("snippet".to_string(), Value::String(h.snippet));
            hits.push(Value::Object(m));
        }
        if truncated {
            break;
        }
    }

    let mut out = Map::new();
    out.insert(
        "dir".to_string(),
        Value::String(p.dir.to_string_lossy().to_string()),
    );
    out.insert("query".to_string(), Value::String(p.query.clone()));
    out.insert("files_searched".to_string(), Value::from(files_searched));
    out.insert("total_hits".to_string(), Value::from(hits.len()));
    out.insert("truncated".to_string(), Value::Bool(truncated));
    out.insert("hits".to_string(), Value::Array(hits));
    out.insert("skipped".to_string(), Value::Array(skipped));
    Ok(Value::Object(out))
}

fn skip_entry(file: &str, reason: &str) -> Value {
    let mut m = Map::new();
    m.insert("file".to_string(), Value::String(file.to_string()));
    m.insert("reason".to_string(), Value::String(reason.to_string()));
    Value::Object(m)
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("grep_files failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
