//! MCP tool: `generate_test_data` - new rows shaped like one or more real
//! tables. Read-only: it returns the generated tables and the plan; saving
//! them is `write_table`'s job.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::DataTable;
use octa::data::test_data::{
    Generator, apply_links, generate, plan_table, profile_table, renamed_category, suggest_links,
};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Generate test data shaped like one or more real tables \
(`sources`, each a `path` or an `open_tab`): the same columns, numbers and dates drawn from the \
real spread, fake names, emails, IBANs and IDs, code-like text in the same shape, empty cells at \
the real rate. Several sources are generated together so links between them (an \
`orders.customer_id` pointing at `customers.id`) still join. `rows` per table (default: as many \
as the real one), `seed` for a repeatable result. Small category columns keep their real values \
unless `rename_categories` is true; the `plan` lists every column's generator and which ones kept \
real values. Returns `{plan, tables: {name: table}}`; pass a table to `write_table` to keep it.";

/// One source: a file (may be a cloud URL) or an open GUI tab.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct Source {
    #[serde(default)]
    pub path: PathBuf,
    /// An open GUI tab (name, or `@active`) instead of a file.
    #[serde(default)]
    pub open_tab: Option<String>,
    /// For multi-table sources, the table to read.
    #[serde(default)]
    pub table: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// One or more tables to imitate.
    pub sources: Vec<Source>,
    /// Rows per generated table. Default: as many as the real table has.
    #[serde(default)]
    pub rows: Option<usize>,
    /// Seed; the same seed gives the same rows. Default 0.
    #[serde(default)]
    pub seed: u64,
    /// Replace the real values of small category columns with `value_1`,
    /// `value_2`, ...
    #[serde(default)]
    pub rename_categories: bool,
    /// Maximum rows returned per table. Pass 0 for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,
}

fn name_of(s: &Source, i: usize) -> String {
    s.open_tab.clone().unwrap_or_else(|| {
        s.path
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("table_{}", i + 1))
    })
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.sources.is_empty() {
        anyhow::bail!("generate_test_data needs at least one source");
    }
    let tables: Vec<DataTable> = p
        .sources
        .iter()
        .map(|s| ctx.resolve(&source_from(&s.open_tab, &s.path, &s.table)))
        .collect::<anyhow::Result<_>>()?;
    let mut plans: Vec<_> = tables
        .iter()
        .zip(&p.sources)
        .enumerate()
        .map(|(i, (t, s))| profile_table(t, &name_of(s, i)))
        .collect();
    let refs: Vec<&DataTable> = tables.iter().collect();
    apply_links(&mut plans, &suggest_links(&refs));
    for plan in &mut plans {
        if let Some(n) = p.rows {
            plan.rows = n;
        }
        if p.rename_categories {
            for col in &mut plan.columns {
                if let Generator::Category { values } = &col.generator {
                    col.generator = renamed_category(values);
                }
            }
        }
    }
    let out = generate(&plans, p.seed)?;

    let mut by_name = Map::new();
    for (plan, t) in plans.iter().zip(&out) {
        by_name.insert(
            plan.name.clone(),
            table_to_json(t, ctx.resolve_row_cap(p.limit), ctx.cell_byte_cap),
        );
    }
    let mut result = Map::new();
    result.insert(
        "plan".into(),
        table_to_json(&plan_table(&plans), None, ctx.cell_byte_cap),
    );
    result.insert("tables".into(), Value::Object(by_name));
    Ok(Value::Object(result))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("generate_test_data failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
