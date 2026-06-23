//! MCP tool: `detect_outliers` - flag numeric outlier cells per column using
//! IQR or z-score. Thin wrapper over `octa::data::outliers`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::outliers::{OutlierMethod, detect_outliers};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Flag numeric outlier cells per column. `method` is `iqr` (default, \
interquartile range: flags values outside [q1 - k*IQR, q3 + k*IQR]) or `zscore` (flags values \
where |z| > k). Default `k` is 1.5 for IQR and 3.0 for z-score; override with `k`. `columns` \
restricts detection to named columns (default: all numeric columns). Columns with fewer than 4 \
numeric values are skipped. Returns `{flagged: [{row, column}, ...], count, method, k}`.";

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

    /// Column names to check for outliers. Defaults to all columns.
    #[serde(default)]
    pub columns: Option<Vec<String>>,

    /// Detection method: `iqr` (default) or `zscore`.
    #[serde(default)]
    pub method: Option<String>,

    /// Threshold multiplier. Default 1.5 for IQR, 3.0 for z-score.
    #[serde(default)]
    pub k: Option<f64>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let table = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;

    let method = match p.method.as_deref() {
        None | Some("iqr") => OutlierMethod::Iqr,
        Some("zscore") => OutlierMethod::ZScore,
        Some(other) => anyhow::bail!("unknown method `{other}` (use iqr or zscore)"),
    };

    let k = p.k.unwrap_or(match method {
        OutlierMethod::Iqr => 1.5,
        OutlierMethod::ZScore => 3.0,
    });

    let col_indices: Vec<usize> = match &p.columns {
        None => (0..table.col_count()).collect(),
        Some(names) => {
            let mut idxs = Vec::new();
            for name in names {
                let idx = table
                    .columns
                    .iter()
                    .position(|c| &c.name == name)
                    .ok_or_else(|| anyhow::anyhow!("no column named `{name}`"))?;
                idxs.push(idx);
            }
            idxs
        }
    };

    let flagged_set = detect_outliers(&table, &col_indices, method, k);

    // Sort for stable, deterministic output.
    let mut pairs: Vec<(usize, usize)> = flagged_set.into_iter().collect();
    pairs.sort_unstable();

    let flagged: Vec<Value> = pairs
        .into_iter()
        .map(|(r, c)| {
            let col_name = table.columns[c].name.clone();
            let mut m = Map::new();
            m.insert("row".to_string(), Value::from(r));
            m.insert("column".to_string(), Value::String(col_name));
            Value::Object(m)
        })
        .collect();

    let count = flagged.len();
    let method_str = match method {
        OutlierMethod::Iqr => "iqr",
        OutlierMethod::ZScore => "zscore",
    };

    let mut out = Map::new();
    out.insert("flagged".to_string(), Value::Array(flagged));
    out.insert("count".to_string(), Value::from(count));
    out.insert("method".to_string(), Value::String(method_str.to_string()));
    out.insert("k".to_string(), Value::from(k));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("detect_outliers failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
