//! Chat tool: `read_text` - return the textual content of an open text / code
//! / markdown file. This is a **chat-only** tool (no MCP `handle`): text, code,
//! and Markdown files load into Octa as a single `Line` column, so this joins
//! that column back into the file's text for the assistant to read, summarise,
//! or explain. Structured formats (JSON parsed into columns, Parquet, ...)
//! should be read with `read_table` instead.

use std::path::PathBuf;

use octa::data::{CellValue, DataTable};
use serde::Deserialize;
use serde_json::{Map, Value};

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Read the plain-text content of an open text, code, or Markdown file (or tab) and return it as \
one string. Text/code/markdown files load as a single line-per-row column; this re-joins them \
into the file's text. Use this (not read_table) when the user wants to read, summarise, explain, \
or edit prose or source code. Returns `text`, `line_count`, and `char_count`. Pair with \
`write_text` to save changes.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Open tab to read: a tab handle like "#2", "@active", or the tab name.
    #[serde(default)]
    pub open_tab: Option<String>,
    /// File to read instead of a tab (must be a file open in Octa).
    #[serde(default)]
    pub path: PathBuf,
}

/// Join the first column's cells into newline-separated text. Text / Markdown /
/// code readers store one line per row in a single `Line` column, so this
/// reconstructs the source faithfully (including any in-memory edits captured
/// in the snapshot).
fn join_lines(table: &DataTable) -> String {
    let rows = table.row_count();
    let mut out = String::new();
    for r in 0..rows {
        if r > 0 {
            out.push('\n');
        }
        match table.get(r, 0) {
            Some(CellValue::String(s)) => out.push_str(s),
            Some(v) => out.push_str(&v.to_string()),
            None => {}
        }
    }
    out
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let table = ctx.resolve(&source_from(&p.open_tab, &p.path, &None))?;
    let text = join_lines(&table);
    let mut out = Map::new();
    out.insert("line_count".to_string(), Value::from(table.row_count()));
    out.insert("char_count".to_string(), Value::from(text.chars().count()));
    out.insert("text".to_string(), Value::String(text));
    Ok(Value::Object(out))
}
