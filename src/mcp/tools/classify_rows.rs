//! MCP tool: `classify_rows` - ask TypeSafe AI's Jev model one question about
//! every row and write the table with the answer and its certainty as two new
//! columns. A write tool (dropped under `--mcp-read-only`). Same engine as the
//! GUI dialog and `octa --classify`.
//!
//! The chosen cells leave the machine, so the tool refuses until the user has
//! switched the feature on; a model calling it cannot do that for them.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::api::jev::JevConfig;
use octa::data::transform::classify::{Category, ClassifyMode, ClassifySpec, add_classify_columns};
use octa::ui::settings::AppSettings;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, read_with_registry};

pub const DESCRIPTION: &str = "Label every row with TypeSafe AI's Jev decision model and write \
the result. SENDS the cells of `columns` to TypeSafe AI (servers in the United States); it is \
refused until the user has switched Classify with Jev on and set a key, which only they can do. \
`mode`: `categories` (the answer is one of `categories`, 2 to 255, each a name and an optional \
description of what belongs there), `yes_no` (true or false), `scale` (a grade on `levels`, 2 to \
10, lowest first, answered as a number). `instructions` is the question in plain \
words. Adds two columns: `new_column` (default `category`, `answer` or `score`) and the certainty \
beside it (`<name>_confidence`, or `<name>_probability` for yes_no), both 0 to 1. Each distinct \
combination of values is sent once. Rows whose chosen cells are all empty, and values that got \
no answer, stay empty; `failed` counts the latter. Writes to `output_path` (default: overwrite \
`path`); the format follows its extension. Database files are not valid sources or targets.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the source file.
    pub path: PathBuf,
    /// Names of the columns whose cells are sent.
    pub columns: Vec<String>,
    /// categories, yes_no or scale.
    pub mode: ClassifyMode,
    /// The question, in plain words.
    pub instructions: String,
    /// For categories: the answers the model may pick.
    #[serde(default)]
    pub categories: Vec<Category>,
    /// For scale: the levels, lowest first.
    #[serde(default)]
    pub levels: Vec<String>,
    /// Name of the answer column.
    #[serde(default)]
    pub new_column: Option<String>,
    /// Where to write the result. Defaults to `path` (overwrite in place).
    #[serde(default)]
    pub output_path: Option<PathBuf>,
    /// Lift the streaming initial-load cap so every row is read.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    // Before any read: a switched-off feature should say so, not a missing file.
    let config = match &ctx.cloud_settings {
        Some(settings) => JevConfig::resolve(settings),
        None => JevConfig::resolve(&AppSettings::load()),
    }?;
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    ctx.ensure_readable(&p.path)?;
    let mut table = read_with_registry(&p.path, None)?;
    if table.db_meta.is_some() {
        anyhow::bail!("database files (SQLite/DuckDB/GeoPackage) are not valid sources");
    }
    let requested = p.output_path.clone().unwrap_or_else(|| p.path.clone());
    table.refuse_partial_overwrite(&p.path, &requested)?;
    let spec = ClassifySpec {
        columns: Vec::new(),
        mode: p.mode,
        instructions: p.instructions.clone(),
        categories: p.categories.clone(),
        levels: p.levels.clone(),
    };
    let send = config.sender();
    let (name, report) = add_classify_columns(
        &mut table,
        &p.columns,
        spec,
        p.new_column.as_deref(),
        &config.model,
        config.parallel,
        &send,
    )?;
    let target = ctx.write_transformed(&requested, &table)?;
    let mut out = Map::new();
    out.insert("rows_written".to_string(), Value::from(table.row_count()));
    out.insert("new_column".to_string(), Value::String(name));
    out.insert("output".to_string(), Value::String(target));
    out.insert("model".to_string(), Value::String(config.model));
    out.insert("values_sent".to_string(), Value::from(report.sent));
    out.insert("failed".to_string(), Value::from(report.failed));
    out.insert("input_tokens".to_string(), Value::from(report.input_tokens));
    if let Some(e) = report.first_error {
        out.insert("first_error".to_string(), Value::String(e));
    }
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("classify_rows failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
