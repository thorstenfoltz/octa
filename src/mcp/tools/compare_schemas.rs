//! MCP tool: `compare_schemas` - diff the column schemas of two files.
//!
//! Reads both sources through the shared format registry (or open tabs),
//! then delegates the actual comparison to `octa::data::compare_schemas`.
//! No rows are serialised; the response is column metadata only.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::compare_schemas::{SchemaDiff, compare_schemas};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Compare the column schemas of two tabular sources (files or open tabs). Returns the four-way \
diff: `common`, `only_in_a`, `only_in_b`, and `type_mismatches` (same name, different type), \
plus `identical`. Address tabs via `open_tab_a` / `open_tab_b`.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the first file. Omit when `open_tab_a` is set.
    #[serde(default)]
    pub path_a: PathBuf,

    /// Path to the second file. Omit when `open_tab_b` is set.
    #[serde(default)]
    pub path_b: PathBuf,

    /// Operate on an open GUI tab for side A (name, or `@active`).
    #[serde(default)]
    pub open_tab_a: Option<String>,

    /// Operate on an open GUI tab for side B (name, or `@active`).
    #[serde(default)]
    pub open_tab_b: Option<String>,

    /// For multi-table sources, the table name to read from file A.
    #[serde(default)]
    pub table_a: Option<String>,

    /// For multi-table sources, the table name to read from file B.
    #[serde(default)]
    pub table_b: Option<String>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let dt_a = ctx.resolve(&source_from(&p.open_tab_a, &p.path_a, &p.table_a))?;
    let dt_b = ctx.resolve(&source_from(&p.open_tab_b, &p.path_b, &p.table_b))?;
    Ok(diff_to_json(&compare_schemas(&dt_a.columns, &dt_b.columns)))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("compare_schemas failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}

/// Render a `SchemaDiff` as the JSON shape documented in the plan:
///   `{ identical, common, only_in_a, only_in_b, type_mismatches }`.
/// Columns are emitted as `{name, type}`; type mismatches as
/// `{name, a, b}`.
pub fn diff_to_json(diff: &SchemaDiff) -> Value {
    fn cols_to_json(cols: &[octa::data::ColumnInfo]) -> Value {
        let arr: Vec<Value> = cols
            .iter()
            .map(|c| {
                let mut m = Map::new();
                m.insert("name".to_string(), Value::String(c.name.clone()));
                m.insert("type".to_string(), Value::String(c.data_type.clone()));
                Value::Object(m)
            })
            .collect();
        Value::Array(arr)
    }

    let mismatches: Vec<Value> = diff
        .type_mismatches
        .iter()
        .map(|m| {
            let mut obj = Map::new();
            obj.insert("name".to_string(), Value::String(m.name.clone()));
            obj.insert("a".to_string(), Value::String(m.type_a.clone()));
            obj.insert("b".to_string(), Value::String(m.type_b.clone()));
            Value::Object(obj)
        })
        .collect();

    let mut out = Map::new();
    out.insert("identical".to_string(), Value::Bool(diff.identical));
    out.insert("common".to_string(), cols_to_json(&diff.common));
    out.insert("only_in_a".to_string(), cols_to_json(&diff.only_in_a));
    out.insert("only_in_b".to_string(), cols_to_json(&diff.only_in_b));
    out.insert("type_mismatches".to_string(), Value::Array(mismatches));
    Value::Object(out)
}
