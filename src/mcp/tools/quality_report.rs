//! MCP tool: `quality_report` - the Data Quality Report: one row per column,
//! an overall score and the findings that span several columns.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::quality::build_quality_report;
use octa::data::shapes::ShapeDetail;
use octa::db::pushdown::LocalParts;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Data quality report for a table: one row per column (missing \
values, distinct values, type consistency, outliers, likely PII and more), an overall score from \
0 to 100, and `sections` for findings that span several columns. Returns `{overall_score, columns, \
sections}`. Operates on a file `path` or an `open_tab`. On a database tab that holds only part of \
its table the counts run on the server, and `from_loaded_rows` names what looked at the loaded \
rows instead.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to report on.
    #[serde(default)]
    pub table: Option<String>,

    /// Lift the streaming initial-load cap so the report sees every row of a
    /// file. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let source = source_from(&p.open_tab, &p.path, &p.table);
    let shapes = ShapeDetail::default();
    let server = ctx.db_tab(&source);
    let (report, local) = if let Some((snap, src)) = server {
        let mut c = ctx.db_tab_connect(src)?;
        let cancel = AtomicBool::new(false);
        let total = octa::db::pushdown::count_rows(&mut *c, src, &cancel)?;
        octa::db::pushdown::quality::run(&mut *c, src, &snap.table, total, shapes, &cancel)?
    } else {
        let _g = p
            .unlimited
            .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
        let table = ctx.resolve(&source)?;
        (build_quality_report(&table, shapes)?, LocalParts::default())
    };

    let row_cap = ctx.resolve_row_cap(None);
    let sections: Vec<Value> = report
        .sections
        .iter()
        .map(|section| {
            let mut m = Map::new();
            m.insert(
                "title".to_string(),
                Value::String(octa::i18n::t(&section.title_key)),
            );
            m.insert(
                "table".to_string(),
                table_to_json(&section.table, row_cap, ctx.cell_byte_cap),
            );
            Value::Object(m)
        })
        .collect();

    let mut out = Map::new();
    out.insert(
        "overall_score".to_string(),
        Value::from(report.overall_score),
    );
    out.insert(
        "columns".to_string(),
        table_to_json(&report.table, None, ctx.cell_byte_cap),
    );
    out.insert("sections".to_string(), Value::Array(sections));
    if server.is_some() {
        out.insert("on_server".to_string(), Value::Bool(true));
    }
    if !local.is_empty() {
        let parts = [local.engine, local.by_design, local.failed].concat();
        out.insert("from_loaded_rows".to_string(), Value::from(parts));
    }
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("quality_report failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
