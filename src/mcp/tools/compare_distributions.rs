//! MCP tool: `compare_distributions` - do two columns come from the same
//! population? Thin wrapper over `octa::data::distribution_compare`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::distribution_compare::{Outcome, compare_columns};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Compare the distributions of two columns and say whether they look \
like the same population. Numeric columns are compared with a two-sample Kolmogorov-Smirnov test \
(the whole shape, not just the mean); anything else is compared as categories with a chi-square \
test of homogeneity. The second column may live in the same source or in another one: pass \
`path_b` / `open_tab_b` / `table_b` to point at it, or omit them to compare two columns of the \
same table. Returns `{headline, verdict, test, statistic, p_value, degrees_of_freedom, \
first_rows, second_rows}`, where `headline` is a plain sentence such as \"The second sample skews \
12% higher.\" and `verdict` is `same` or `different` at p = 0.05. Reports `{skipped: \
\"na_<reason>\"}` when the test does not apply: fewer than 20 usable values in either column, or \
more than 50 distinct categories.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the first file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table holding the first column.
    #[serde(default)]
    pub table: Option<String>,

    /// Name of the first column.
    pub column: String,

    /// Path to the second file. Omit to use the first source.
    #[serde(default)]
    pub path_b: Option<PathBuf>,

    /// Open tab holding the second column. Omit to use the first source.
    #[serde(default)]
    pub open_tab_b: Option<String>,

    /// For multi-table second sources, the specific table.
    #[serde(default)]
    pub table_b: Option<String>,

    /// Name of the second column. Defaults to `column`, which is what you want
    /// when comparing the same column across two files.
    #[serde(default)]
    pub column_b: Option<String>,

    /// Lift the streaming initial-load cap so every row is compared.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let first = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    // No second source named means both columns live in the first table, which
    // is the "did these two fields drift apart" case.
    let second = if p.path_b.is_some() || p.open_tab_b.is_some() {
        let path = p.path_b.clone().unwrap_or_default();
        ctx.resolve(&source_from(&p.open_tab_b, &path, &p.table_b))?
    } else {
        first.clone()
    };

    let name_b = p.column_b.clone().unwrap_or_else(|| p.column.clone());
    let ia = column_index(&first, &p.column)?;
    let ib = column_index(&second, &name_b)?;

    let outcome = compare_columns(&first, ia, &second, ib);
    let mut out = Map::new();
    out.insert("first".to_string(), Value::String(p.column.clone()));
    out.insert("second".to_string(), Value::String(name_b));
    match outcome {
        Outcome::Skipped(reason) => {
            out.insert("skipped".to_string(), Value::String(reason.to_string()));
        }
        Outcome::Compared(c) => {
            out.insert("headline".to_string(), Value::String(c.headline.sentence()));
            out.insert(
                "headline_id".to_string(),
                Value::String(c.headline.id().to_string()),
            );
            out.insert(
                "verdict".to_string(),
                Value::String(if c.same { "same" } else { "different" }.to_string()),
            );
            out.insert("test".to_string(), Value::String(c.kind.id().to_string()));
            out.insert("statistic".to_string(), Value::from(c.statistic));
            out.insert("p_value".to_string(), Value::from(c.p_value));
            out.insert(
                "degrees_of_freedom".to_string(),
                match c.degrees_of_freedom {
                    Some(d) => Value::from(d),
                    None => Value::Null,
                },
            );
            out.insert("first_rows".to_string(), Value::from(c.n_a));
            out.insert("second_rows".to_string(), Value::from(c.n_b));
        }
    }
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
        .map_err(|e| {
            McpError::invalid_params(format!("compare_distributions failed: {e}"), None)
        })?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
