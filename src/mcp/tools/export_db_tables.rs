//! MCP tool: `export_db_tables` - the tables of one schema of a saved
//! live-database connection into a folder, one file per table, each streamed
//! page by page (`octa::db::export`). A **write** tool: it writes files, so
//! it is removed under `--mcp-read-only` and bails on a read-only context; the
//! folder goes through the same sandbox as every other write tool's output.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::db::export::{ExportFormat, ExportRequest, TableOutcome};

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "Export the tables of one schema of a saved live-database \
connection (see `list_db_connections`) into a folder, one file per table. `format` is \
`parquet` (default), `csv`, `tsv` or `jsonl`; `tables` limits it to some tables (default: every \
table of the schema). Each table is read page by page, so its size does not matter. Existing \
files are skipped unless `overwrite`. One failed table does not stop the rest. Returns \
`{written, skipped, failed, rows, items}`. On Snowflake, Databricks and BigQuery pass \
`catalog`. Writes files, so it is unavailable in read-only mode.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Connection name (or id) from Settings -> Databases.
    pub connection: String,
    /// Schema whose tables are exported (the database name on MySQL).
    pub schema: String,
    /// Catalog on Snowflake, Databricks or BigQuery.
    #[serde(default)]
    pub catalog: Option<String>,
    /// Tables to export. Default: every table of the schema.
    #[serde(default)]
    pub tables: Vec<String>,
    /// Folder the files go into. Created if absent.
    pub out_dir: PathBuf,
    /// `parquet` (default) | `csv` | `tsv` | `jsonl`.
    #[serde(default)]
    pub format: Option<String>,
    /// Replace files that are already there. Default: skip them.
    #[serde(default)]
    pub overwrite: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if ctx.read_only {
        anyhow::bail!("export_db_tables writes files and this session is read-only");
    }
    let format = match p.format.as_deref() {
        None => ExportFormat::Parquet,
        Some(name) => ExportFormat::parse(name).ok_or_else(|| {
            anyhow::anyhow!("format must be parquet, csv, tsv or jsonl (got '{name}')")
        })?,
    };
    let conn = ctx.find_db_connection(&p.connection)?;
    octa::db::reject_catalog(conn.engine, p.catalog.as_deref())?;
    // Sandbox rules are the same as every other write tool's.
    let dir = ctx.resolve_write_path(&p.out_dir)?;

    let mut c = ctx.db_connect(&conn)?;
    let tables = if p.tables.is_empty() {
        c.list_tables(p.catalog.as_deref(), &p.schema)?
    } else {
        p.tables.clone()
    };
    if tables.is_empty() {
        anyhow::bail!("schema '{}' has no tables", p.schema);
    }
    let request = ExportRequest {
        catalog: p.catalog.clone(),
        schema: p.schema.clone(),
        tables,
        dir,
        format,
        overwrite: p.overwrite,
        options: octa::ui::settings::AppSettings::load().write_options,
    };
    let outcomes = octa::db::export::export_tables(
        c.as_mut(),
        &request,
        // No terminal to report into: the answer carries the counts.
        &|_| {},
        &AtomicBool::new(false),
    )?;
    Ok(summary(&outcomes))
}

fn summary(outcomes: &[TableOutcome]) -> Value {
    let mut written = 0;
    let mut skipped = 0;
    let mut failed = 0;
    let mut rows = 0;
    let items: Vec<Value> = outcomes
        .iter()
        .map(|o| match o {
            TableOutcome::Written {
                table,
                path,
                rows: n,
            } => {
                written += 1;
                rows += n;
                json!({"table": table, "status": "written", "path": path.display().to_string(), "rows": n})
            }
            TableOutcome::Skipped { table, path } => {
                skipped += 1;
                json!({"table": table, "status": "skipped", "path": path.display().to_string()})
            }
            TableOutcome::Failed { table, error } => {
                failed += 1;
                json!({"table": table, "status": "failed", "error": error})
            }
        })
        .collect();
    json!({
        "written": written,
        "skipped": skipped,
        "failed": failed,
        "rows": rows,
        "items": items,
    })
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("export_db_tables failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}

#[cfg(test)]
#[path = "export_db_tables_tests.rs"]
mod tests;
