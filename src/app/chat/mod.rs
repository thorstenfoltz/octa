//! In-GUI chat assistant: a docked panel where the user chats with an LLM that
//! drives Octa's existing data tools (the same `crate::mcp::tools::<name>::run`
//! the MCP server uses) against the open tabs and files on disk.
//!
//! ## Layout
//! - `types`     - provider-neutral message + event model.
//! - `tools`     - turns the MCP tools into LLM tool defs + a dispatch table.
//! - `providers` - one adapter per backend (Anthropic / OpenAI / compatible /
//!   Gemini), each translating to its wire format and parsing the SSE stream.
//! - `session`   - the live `Arc<Mutex<ChatSessionState>>` the UI drains.
//! - `agent`     - the worker-thread turn loop (stream -> run tools -> repeat).
//! - `persist`   - JSON session save / load under `chat_sessions/`.
//! - `secrets`   - per-provider API keys (env -> keyring -> plaintext).
//!
//! The GUI thread builds a `ToolContext` of table snapshots and a system
//! prompt, then `agent::spawn_turn` moves them onto a `std::thread`. No tokio
//! in the GUI process.

pub mod agent;
pub mod audit;
pub mod ollama;
pub mod persist;
pub mod providers;
/// API-key storage moved into the library (`octa::ui::settings::secrets`) so
/// the Settings dialog can manage keys too; re-exported here so existing
/// `chat::secrets` call sites keep working.
pub use crate::ui::settings::secrets;
pub mod session;
pub mod tools;
pub mod types;

use serde_json::Value;

/// Build the system prompt, embedding a compact description of what tabs the
/// user currently has open so the model can reach for `open_tab: "@active"`.
pub fn build_system_prompt(tab_summaries: &[Value]) -> String {
    let mut s = String::new();
    s.push_str(
        "You are Octa's built-in data assistant. Octa is a desktop viewer/editor for tabular \
data (Parquet, CSV, JSON, Excel, SQLite, DuckDB, and more) AND for text, source code, and \
Markdown files. Help the user inspect, query, understand, and edit whatever they have open by \
calling the provided tools, then explain the results in plain language.\n\n",
    );
    s.push_str(
        "Guidance:\n\
- You can ONLY access files the user has open in Octa (listed below). Each tab has a stable \
handle like `#1`. Always address open data with `open_tab` - `open_tab: \"#2\"` (preferred when \
names repeat), `open_tab: \"@active\"`, or `open_tab: \"<tab name>\"`. NEVER invent a filesystem \
`path` for data that is already open (don't put a handle or file name in `path`). The user may \
point you at one with an `@` mention (`@#2`, `@<tab name>`, or `@<column>` for a column); infer \
the target from context when they don't.\n\
- You CANNOT open arbitrary files from disk. If the user asks about a file that is not open, \
tell them to open it in Octa first (File > Open). For another sheet or table of an open Excel \
workbook or DuckDB/SQLite database, call `list_tables` then `read_table` with that open file's \
`path` and the inner table/sheet name - the other sheets/tables of an open file are reachable.\n\
- Prefer `schema` or `describe_file` to orient yourself before reading rows, and `run_sql` \
(DuckDB, the active source is exposed as `data`) for aggregation, filtering, and joins. To JOIN \
open tabs, set `open_tab` to the first and add EACH other tab as an `extra_tables` entry whose \
`path` is its handle or name, e.g. `extra_tables: [{name: \"b\", path: \"#2\"}, {name: \"c\", \
path: \"#3\"}]`, then JOIN `data` with `b`, `c`, ... Any number of tables can be joined.\n\
- Text, source-code, and Markdown files open as a single line-per-row column. For those, use \
`read_text` (not `read_table`) to get the file's text, and `write_text` to save changes - either \
to a new file or back to the open tab's file on disk (the user reloads with Ctrl+R to see it). \
Use these to summarise, explain, refactor, or edit prose and code.\n\
- Keep responses concise. Report concrete numbers from tool results rather than guessing.\n\
- To save results, give a bare filename; Octa writes it into the user's export directory (all \
file writes are confined there). To save a query or JOIN result, use \
`run_sql` with `write_to` (the extension picks the format: csv / parquet / xlsx / ... or \
duckdb / sqlite). Use `write_table` for inline data, `convert` to transcode a whole source, and \
`create_chart` for charts. Writing changes back into an open tab is not supported; `edit_table` \
edits an open file on disk in place.\n",
    );

    if tab_summaries.is_empty() {
        s.push_str("\nThe user currently has no tabs open.\n");
    } else {
        s.push_str("\nOpen tabs right now:\n");
        for t in tab_summaries {
            let handle = t["handle"].as_str().unwrap_or("?");
            let name = t["display_name"].as_str().unwrap_or("?");
            let active = t["active"].as_bool().unwrap_or(false);
            let rows = t["row_count"].as_u64().unwrap_or(0);
            let cols = t["column_count"].as_u64().unwrap_or(0);
            let marker = if active { " (active)" } else { "" };
            s.push_str(&format!(
                "- {handle} \"{name}\"{marker}: {rows} rows, {cols} columns\n"
            ));
        }
    }
    s
}
