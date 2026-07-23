//! MCP tool: `query_db` - run one SQL statement on a saved live-database
//! connection, server-side and in the engine's native dialect. Mutations are
//! gated on the connection's allow-writes switch AND the server not being
//! read-only.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, table_to_json};

pub const DESCRIPTION: &str = "Run one SQL statement on a saved live-database connection \
(see `list_db_connections`), server-side and in the engine's NATIVE dialect (PostgreSQL, \
MySQL/MariaDB, or SQL Server - not DuckDB SQL). SELECTs return `{schema, rows, ...}` like \
`read_table` (the `limit` param caps the response). Mutations (INSERT/UPDATE/DELETE/DDL) \
return `{rows_affected}` and are refused unless the connection's \"Allow writes\" switch is \
on. Add your own LIMIT/TOP for large tables - the whole result is fetched from the server.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Saved connection name (or id) from Settings -> Databases.
    pub connection: String,
    /// The SQL statement, in the server's native dialect.
    pub sql: String,
    /// Response row cap for SELECTs. Absent: the server default. 0: unlimited.
    #[serde(default)]
    pub limit: Option<usize>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let conn = ctx.find_db_connection(&p.connection)?;
    if octa::sql::is_mutation(&p.sql) {
        if ctx.read_only {
            anyhow::bail!(
                "database mutations are disabled for this session (read-only MCP server or a chat profile without \"Allow writes\")"
            );
        }
        octa::db::ensure_write_allowed(&conn, Some(&p.sql))?;
        let mut c = ctx.db_connect(&conn)?;
        let affected = c.execute(&p.sql)?;
        return Ok(json!({
            "kind": "mutation",
            "connection": conn.name,
            "rows_affected": affected,
        }));
    }
    let mut c = ctx.db_connect(&conn)?;
    let table = c.query(&p.sql)?;
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
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("query_db failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with(conn: octa::db::DbConnection, read_only: bool) -> ToolContext {
        ToolContext::for_mcp(Some(1000), 65536, false, true, vec![conn], read_only)
    }

    fn conn(allow_writes: bool) -> octa::db::DbConnection {
        octa::db::DbConnection {
            id: "db-1".into(),
            name: "wh".into(),
            engine: octa::db::DbEngine::Postgres,
            host: "localhost".into(),
            port: 5432,
            database: "app".into(),
            username: "u".into(),
            auth: octa::db::DbAuth::Password,
            allow_writes,
            oauth_client_id: None,
            oauth_tenant: None,
        }
    }

    #[test]
    fn unknown_connection_lists_names() {
        let ctx = ctx_with(conn(false), false);
        let p = Params {
            connection: "nope".into(),
            sql: "SELECT 1".into(),
            limit: None,
        };
        let err = run(&ctx, &p).unwrap_err().to_string();
        assert!(err.contains("nope") && err.contains("wh"), "{err}");
    }

    #[test]
    fn mutation_refused_without_allow_writes() {
        let ctx = ctx_with(conn(false), false);
        let p = Params {
            connection: "wh".into(),
            sql: "DELETE FROM t".into(),
            limit: None,
        };
        let err = run(&ctx, &p).unwrap_err().to_string();
        assert!(err.to_lowercase().contains("write"), "{err}");
    }

    #[test]
    fn read_only_server_refuses_mutation_even_when_conn_allows() {
        let ctx = ctx_with(conn(true), true);
        let p = Params {
            connection: "wh".into(),
            sql: "UPDATE t SET x = 1".into(),
            limit: None,
        };
        let err = run(&ctx, &p).unwrap_err().to_string();
        assert!(err.contains("disabled for this session"), "{err}");
    }
}
