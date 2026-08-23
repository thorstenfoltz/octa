//! MCP tool: `sync_sql` - return the SQL that would make a live database
//! table match a file or an open tab, without running any of it.
//!
//! **Read-only**: it reads the server table and returns text, so it stays
//! available under `--mcp-read-only` and is not in `WRITE_TOOL_NAMES`. The
//! rendering is `db::write_back::render_plan_sql`, the same function the GUI's
//! Save SQL and the CLI's `--sync-sql` use, so all three agree statement for
//! statement.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::compare::compare_join;
use octa::db::write_back::{plan_from_comparison, render_plan_sql};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Return the SQL that would make a live database table match a \
file or an open tab, WITHOUT running it. Use when a change to a server table has to be \
reviewed before it is applied. Reads the table (see `list_db_connections`), compares it to \
the source on the `on` key columns, and returns one transaction: DELETEs for rows only on \
the server, full-row UPDATEs for rows whose values differ, INSERTs for rows only in the \
source. Nothing is written. Columns present only in the source are reported and ignored - \
this never emits ALTER TABLE. Numbers are compared as numbers, so 120.50 and 120.5 are not \
treated as a change.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Source file holding the desired state. Omit when using `open_tab`.
    #[serde(default)]
    pub path: PathBuf,
    /// Open tab name, `@active`, or a handle such as `#2`, instead of `path`.
    #[serde(default)]
    pub open_tab: Option<String>,
    /// Inner table/sheet name for a multi-table source file.
    #[serde(default)]
    pub table: Option<String>,
    /// Saved connection name (or id) from Settings -> Databases.
    pub connection: String,
    /// Target table as `SCHEMA.TABLE` (or `CATALOG.SCHEMA.TABLE`).
    pub target: String,
    /// Key columns matching source rows to table rows. Required: without a key
    /// there is no way to tell an updated row from a delete plus an insert.
    pub on: Vec<String>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let keys: Vec<String> =
        p.on.iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    if keys.is_empty() {
        anyhow::bail!("`on` needs at least one key column name");
    }

    let source = source_from(&p.open_tab, &p.path, &p.table);
    let desired = ctx.resolve(&source)?;
    for key in &keys {
        if !desired.columns.iter().any(|c| &c.name == key) {
            anyhow::bail!(
                "key column '{key}' is not in the source; available: {}",
                desired
                    .columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    let conn = ctx.find_db_connection(&p.connection)?;
    let (catalog, schema, table) = octa::db::fetch_table::split_qualified(&p.target);
    let mut c = ctx.db_connect(&conn)?;
    let sql = octa::db::select_sample_sql(
        conn.engine,
        catalog.as_deref(),
        &schema,
        &table,
        octa::formats::initial_load_rows(),
    );
    let live = c.query(&sql)?;

    let result = compare_join(&live, &desired, &keys)?;
    let synced = plan_from_comparison(&live, &desired, &result, &keys)?;
    let plan = synced.plan;

    let script = render_plan_sql(conn.engine, &schema, &table, &desired.columns, &keys, &plan);
    Ok(json!({
        "connection": conn.name,
        "target": format!("{schema}.{table}"),
        "sql": script,
        "deletes": plan.deletes.len(),
        "updates": plan.updates.len(),
        "inserts": plan.inserts.len(),
        "ignored_columns": synced.ignored_columns,
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("sync_sql failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
