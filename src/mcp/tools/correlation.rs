//! MCP tool: `correlation` - pairwise correlation matrix over a table's
//! numeric columns. Thin wrapper over `octa::data::correlation`.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::correlation::{CorrMethod, correlation_matrix, numeric_columns};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Compute a pairwise correlation matrix over the numeric columns of \
a tabular file or open tab. `method` is `pearson` (linear, default) or `spearman` (monotonic, \
rank-based). Non-numeric columns are ignored. For each pair only rows where both values are \
present are used; a coefficient is `null` when undefined (fewer than two paired rows or zero \
variance). Returns `{columns: [...], matrix: [[r, ...], ...]}` where `matrix[i][j]` is the \
correlation of `columns[i]` with `columns[j]`.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to analyse.
    #[serde(default)]
    pub table: Option<String>,

    /// `pearson` (default) or `spearman`.
    #[serde(default)]
    pub method: Option<String>,

    /// Lift the streaming initial-load cap so the correlation uses every row.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let method = match p.method.as_deref() {
        None => CorrMethod::Pearson,
        Some(s) => CorrMethod::parse(s)
            .ok_or_else(|| anyhow::anyhow!("unknown method `{s}` (use pearson or spearman)"))?,
    };

    let source = source_from(&p.open_tab, &p.path, &p.table);
    let server = ctx.db_tab(&source);
    let cm = if let Some((snap, src)) = server {
        // The loaded rows pick the numeric columns, as in the GUI: Postgres NUMERIC
        // and MySQL DECIMAL reach the tab as text.
        let columns: Vec<_> = numeric_columns(&snap.table)
            .into_iter()
            .map(|i| snap.table.columns[i].clone())
            .collect();
        let mut c = ctx.db_tab_connect(src)?;
        let cancel = AtomicBool::new(false);
        octa::db::pushdown::correlation::run(&mut *c, src, &columns, method, &cancel)?
    } else {
        let _g = p
            .unlimited
            .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
        correlation_matrix(&ctx.resolve(&source)?, method)
    };

    let columns: Vec<Value> = cm.columns.iter().cloned().map(Value::String).collect();
    let matrix: Vec<Value> = cm
        .matrix
        .iter()
        .map(|row| {
            Value::Array(
                row.iter()
                    .map(|cell| match cell {
                        Some(r) => Value::from(*r),
                        None => Value::Null,
                    })
                    .collect(),
            )
        })
        .collect();

    let mut out = Map::new();
    out.insert(
        "method".to_string(),
        Value::String(format!("{method:?}").to_lowercase()),
    );
    out.insert("columns".to_string(), Value::Array(columns));
    out.insert("matrix".to_string(), Value::Array(matrix));
    if server.is_some() {
        out.insert("on_server".to_string(), Value::Bool(true));
    }
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("correlation failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
