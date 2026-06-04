//! MCP tool: `describe_file` - one-shot orientation snapshot of a
//! tabular file. Format, file size, row count, schema, and a small
//! sample of rows in a single call.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::CellValue;
use octa::data::DataTable;
use octa::data::describe::{FileDescription, describe_file};

use crate::mcp::OctaMcpServer;

use super::{Source, ToolContext, source_from};

pub const DESCRIPTION: &str = "One-shot orientation snapshot of a tabular file or open tab. Collapses the \
list_tables -> schema -> read_table dance into one call. Returns path, format, size, row count, \
columns, and a small `sample_rows` (default 5, max 100). Use this first when meeting an \
unfamiliar file.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to describe.
    #[serde(default)]
    pub table: Option<String>,

    /// Number of sample rows to include (default 5, max 100).
    #[serde(default)]
    pub sample_rows: Option<usize>,

    /// Lift the streaming initial-load cap so the row count reflects
    /// every row in the file. Without this, the count is bounded by
    /// the cap and `initial_load_capped` flags `true`. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let source = source_from(&p.open_tab, &p.path, &p.table);
    match &source {
        Source::Path { path, table } => {
            let d = describe_file(path, table.as_deref(), p.sample_rows)?;
            Ok(description_to_json(&d, ctx.cell_byte_cap))
        }
        _ => {
            // An open tab is already materialised; synthesise a description.
            let name = match &source {
                Source::ActiveTab => ctx
                    .active_tab
                    .and_then(|i| ctx.open_tabs.get(i))
                    .map(|t| t.display_name.clone())
                    .unwrap_or_else(|| "active tab".to_string()),
                Source::OpenTab(n) => n.clone(),
                Source::Path { .. } => unreachable!(),
            };
            let dt = ctx.resolve(&source)?;
            Ok(tab_description_to_json(
                &name,
                &dt,
                p.sample_rows,
                ctx.cell_byte_cap,
            ))
        }
    }
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("describe failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}

/// Build the same response shape as the file path, from an in-memory tab.
fn tab_description_to_json(
    name: &str,
    dt: &DataTable,
    sample_rows: Option<usize>,
    cell_cap: usize,
) -> Value {
    let n = sample_rows.unwrap_or(5).min(100);
    let mut out = Map::new();
    out.insert("path".to_string(), Value::String(name.to_string()));
    out.insert(
        "format_name".to_string(),
        Value::String("Open tab".to_string()),
    );
    out.insert("file_size_bytes".to_string(), Value::Null);
    out.insert("table".to_string(), Value::Null);
    out.insert("row_count".to_string(), Value::from(dt.row_count()));
    out.insert("initial_load_capped".to_string(), Value::Bool(false));
    out.insert(
        "initial_load_cap".to_string(),
        Value::from(octa::formats::initial_load_rows()),
    );

    let columns: Vec<Value> = dt
        .columns
        .iter()
        .map(|c| {
            let mut m = Map::new();
            m.insert("name".to_string(), Value::String(c.name.clone()));
            m.insert("type".to_string(), Value::String(c.data_type.clone()));
            Value::Object(m)
        })
        .collect();
    out.insert("column_count".to_string(), Value::from(columns.len()));
    out.insert("columns".to_string(), Value::Array(columns));

    let mut cell_truncated = false;
    let emit = n.min(dt.row_count());
    let mut sample: Vec<Value> = Vec::with_capacity(emit);
    for r in 0..emit {
        let arr: Vec<Value> = (0..dt.col_count())
            .map(|c| {
                let (v, t) =
                    super::cell_to_json(dt.get(r, c).unwrap_or(&CellValue::Null), cell_cap);
                if t {
                    cell_truncated = true;
                }
                v
            })
            .collect();
        sample.push(Value::Array(arr));
    }
    out.insert("sample_rows".to_string(), Value::Array(sample));
    out.insert("sample_row_count".to_string(), Value::from(emit));
    out.insert("cell_truncated".to_string(), Value::Bool(cell_truncated));
    Value::Object(out)
}

fn description_to_json(d: &FileDescription, cell_cap: usize) -> Value {
    let mut out = Map::new();
    out.insert("path".to_string(), Value::String(d.path.clone()));
    out.insert(
        "format_name".to_string(),
        d.format_name
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null),
    );
    out.insert(
        "file_size_bytes".to_string(),
        d.file_size_bytes.map(Value::from).unwrap_or(Value::Null),
    );
    out.insert(
        "table".to_string(),
        d.table
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null),
    );
    out.insert("row_count".to_string(), Value::from(d.row_count));
    out.insert(
        "initial_load_capped".to_string(),
        Value::Bool(d.initial_load_capped),
    );
    out.insert(
        "initial_load_cap".to_string(),
        Value::from(d.initial_load_cap),
    );

    let columns: Vec<Value> = d
        .columns
        .iter()
        .map(|c| {
            let mut m = Map::new();
            m.insert("name".to_string(), Value::String(c.name.clone()));
            m.insert("type".to_string(), Value::String(c.data_type.clone()));
            Value::Object(m)
        })
        .collect();
    out.insert("columns".to_string(), Value::Array(columns));
    out.insert("column_count".to_string(), Value::from(d.columns.len()));

    let mut cell_truncated = false;
    let sample: Vec<Value> = d
        .sample_rows
        .iter()
        .map(|row| {
            let arr: Vec<Value> = row
                .iter()
                .map(|cell| {
                    let (v, t) = super::cell_to_json(cell, cell_cap);
                    if t {
                        cell_truncated = true;
                    }
                    v
                })
                .collect();
            Value::Array(arr)
        })
        .collect();
    out.insert("sample_rows".to_string(), Value::Array(sample));
    out.insert(
        "sample_row_count".to_string(),
        Value::from(d.sample_rows.len()),
    );
    out.insert("cell_truncated".to_string(), Value::Bool(cell_truncated));
    Value::Object(out)
}
