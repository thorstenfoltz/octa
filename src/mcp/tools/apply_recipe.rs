//! MCP tool: `apply_recipe` - replay a saved `.ocp` recipe on a table and
//! return the result. Read-only: it changes nothing on disk; hand the rows to
//! `write_table` to keep them.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use octa::data::recipe::{Recipe, apply_recipe};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Replay a saved recipe (`.ocp`, recorded in the Octa GUI: renames, type \
changes, sorts, dropped duplicates, filled gaps, split/merged columns and more, all by column name) \
on a table and return the result. `recipe_path` is the recipe; `path` / `open_tab` the table. \
Returns `{steps: [{step, ran, error}], skipped, table}`. A step whose column is missing is skipped \
and reported, the rest still run. Nothing is written; pass `table` to `write_table` to keep it.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the `.ocp` recipe file.
    pub recipe_path: PathBuf,
    /// Path to the data file. Omit when `open_tab` is set. May be a cloud
    /// object URL.
    #[serde(default)]
    pub path: PathBuf,
    /// Operate on an open GUI tab (name, or `@active`).
    #[serde(default)]
    pub open_tab: Option<String>,
    /// For multi-table sources, the table to read.
    #[serde(default)]
    pub table: Option<String>,
    /// Maximum rows to return. Pass 0 for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Lift the streaming row cap so every row is read.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    ctx.ensure_readable(&p.recipe_path)?;
    let recipe = Recipe::load(&p.recipe_path)?;
    let mut table = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let outcome = apply_recipe(&mut table, &recipe);

    let steps: Vec<Value> = outcome
        .iter()
        .map(|o| json!({ "step": o.description, "ran": o.error.is_none(), "error": o.error }))
        .collect();
    let mut out = Map::new();
    out.insert(
        "skipped".into(),
        Value::from(outcome.iter().filter(|o| o.error.is_some()).count()),
    );
    out.insert("steps".into(), Value::Array(steps));
    out.insert(
        "table".into(),
        table_to_json(&table, ctx.resolve_row_cap(p.limit), ctx.cell_byte_cap),
    );
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("apply_recipe failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
