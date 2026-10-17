//! MCP tool: `query_db` - run one SQL statement on a saved live-database
//! connection, server-side and in the engine's native dialect. Mutations are
//! gated on the connection's allow-writes switch AND the server not being
//! read-only.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, table_to_json};

pub const DESCRIPTION: &str = "Run one SQL statement on a saved live-database connection \
(see `list_db_connections`), server-side and in the engine's NATIVE dialect (PostgreSQL, \
MySQL/MariaDB, or SQL Server - not DuckDB SQL). SELECTs return `{schema, rows, ...}` like \
`read_table` (the `limit` param caps the response). Mutations (INSERT/UPDATE/DELETE/DDL) \
return `{rows_affected}` and are refused unless the connection's \"Allow writes\" switch is \
on; without it only statements starting with SELECT, WITH, SHOW, DESCRIBE or EXPLAIN run. Add your own LIMIT/TOP for large tables - the whole result is fetched from the server. Pass explain: true to get the query plan instead of running the query.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Saved connection name (or id) from Settings -> Databases.
    pub connection: String,
    /// The SQL statement, in the server's native dialect.
    pub sql: String,
    /// Response row cap for SELECTs. Absent: the server default. 0: unlimited.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Return the statement's plan instead of running it (PostgreSQL, MySQL /
    /// MariaDB; BigQuery returns `bytes_scanned` from a dry run). Nothing
    /// runs, so the write switch does not apply.
    #[serde(default)]
    pub explain: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let conn = ctx.find_db_connection(&p.connection)?;
    if p.explain {
        let mut c = ctx.db_connect(&conn)?;
        let mut out = super::plan_json(&octa::sql::explain::explain_db(&mut *c, &p.sql)?);
        out["connection"] = json!(conn.name);
        return Ok(out);
    }
    if ctx.read_only {
        let why = if octa::sql::is_mutation(&p.sql) {
            Some("this statement writes".to_string())
        } else {
            octa::db::write_risk(conn.engine, &p.sql)
        };
        if let Some(why) = why {
            anyhow::bail!(
                "database writes are disabled for this session (a read-only MCP server or a chat profile without \"Allow writes\"), and {why}"
            );
        }
    }
    octa::db::ensure_write_allowed(&conn, Some(&p.sql))?;
    if octa::sql::is_mutation(&p.sql) {
        let mut c = ctx.db_connect(&conn)?;
        let affected = octa::db::with_identifier_fix(conn.engine, &p.sql, |s| c.execute(s)).0?;
        return Ok(json!({
            "kind": "mutation",
            "connection": conn.name,
            "rows_affected": affected,
        }));
    }
    let mut c = ctx.db_connect(&conn)?;
    let table = octa::db::with_identifier_fix(conn.engine, &p.sql, |s| c.query(s)).0?;
    let row_cap = match p.limit {
        Some(n) => Some(n),
        None => ctx.default_row_limit,
    };
    let mut payload = table_to_json(&table, row_cap, ctx.cell_byte_cap);
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("connection".into(), json!(conn.name));
    }
    Ok(payload)
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("query_db failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}

#[cfg(test)]
#[path = "query_db_tests.rs"]
mod tests;
