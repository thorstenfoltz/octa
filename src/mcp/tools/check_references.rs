//! MCP tool: `check_references` - which child rows point at a parent that is
//! not there? Thin wrapper over `octa::data::referential`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::referential::check;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "List the child rows whose foreign key has no matching parent. \
Point `path` / `open_tab` at the PARENT and `child_path` / `child_open_tab` at the child, or omit \
the child source for a self-reference (a `manager_id` pointing at `id` in one table). Keys are \
compared as trimmed text, so `1` matches `1` across a CSV/database boundary. **A null or empty \
key is not an orphan** - in a relational database it means \"no parent\" - and is counted \
separately as `null_keys`. Returns `{clean, parent_values, checked_rows, null_keys, orphan_rows, \
orphan_values, orphans: [{value, rows}], sentence}`, orphans most-rows-first and capped at 500 \
listed values (the counts stay exact).";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the parent file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Open GUI tab holding the parent. Pass its name, or `@active`.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table parent sources, the specific table.
    #[serde(default)]
    pub table: Option<String>,

    /// The parent's key column.
    pub parent_column: String,

    /// Path to the child file. Omit for a self-reference.
    #[serde(default)]
    pub child_path: Option<PathBuf>,

    /// Open tab holding the child. Omit for a self-reference.
    #[serde(default)]
    pub child_open_tab: Option<String>,

    /// For multi-table child sources, the specific table.
    #[serde(default)]
    pub child_table: Option<String>,

    /// The child's foreign-key column.
    pub child_column: String,

    /// Lift the streaming initial-load cap so every row is checked.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let parent = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let child = if p.child_path.is_some() || p.child_open_tab.is_some() {
        let path = p.child_path.clone().unwrap_or_default();
        ctx.resolve(&source_from(&p.child_open_tab, &path, &p.child_table))?
    } else {
        parent.clone()
    };

    let pcol = column_index(&parent, &p.parent_column)?;
    let ccol = column_index(&child, &p.child_column)?;
    let report = check(&parent, pcol, &child, ccol);

    let orphans: Vec<Value> = report
        .orphans
        .iter()
        .map(|o| {
            let mut m = Map::new();
            m.insert("value".to_string(), Value::String(o.value.clone()));
            m.insert("rows".to_string(), Value::from(o.rows));
            Value::Object(m)
        })
        .collect();

    let mut out = Map::new();
    out.insert("clean".to_string(), Value::Bool(report.is_clean()));
    out.insert("sentence".to_string(), Value::String(report.sentence()));
    out.insert(
        "parent_values".to_string(),
        Value::from(report.parent_values),
    );
    out.insert("checked_rows".to_string(), Value::from(report.checked_rows));
    out.insert("null_keys".to_string(), Value::from(report.null_keys));
    out.insert("orphan_rows".to_string(), Value::from(report.orphan_rows));
    out.insert(
        "orphan_values".to_string(),
        Value::from(report.orphan_values),
    );
    out.insert("orphans".to_string(), Value::Array(orphans));
    Ok(Value::Object(out))
}

fn column_index(t: &octa::data::DataTable, name: &str) -> anyhow::Result<usize> {
    t.columns
        .iter()
        .position(|c| c.name == name)
        .ok_or_else(|| anyhow::anyhow!("no column named `{name}`"))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("check_references failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
