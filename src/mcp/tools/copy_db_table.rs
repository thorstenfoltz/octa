//! MCP tool: `copy_db_table` - table copy between two saved live-database
//! connections (`octa::db::copy`), any engine to any engine. Removed under
//! `--mcp-read-only`; always gated on the target connection's allow-writes
//! switch. Postgres<->MySQL/Redshift use the fast DuckDB lane; every other pair
//! streams through Octa.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "Copy a table from one saved live-database connection to another \
(see `list_db_connections`), any engine to any engine. Postgres/MySQL/Redshift pairs stream \
server-to-server through DuckDB (fast, no row cap); every other pair (the warehouses, SQL \
Server) is streamed through Octa in batches. The target connection's \"Allow writes\" switch \
must be on. `mode` is `create` (default, error if the target table exists), `append`, or \
`replace` (DROP + CREATE). Target schema/table default to the source's. Returns \
`{rows_copied, created}`. On Snowflake, Databricks and BigQuery pass `source_catalog` / \
`target_catalog` for the top namespace level.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Source connection name (or id) from Settings -> Databases.
    pub source_connection: String,
    /// Source schema (the database name on MySQL).
    pub source_schema: String,
    /// Source table name.
    pub source_table: String,
    /// Target connection name (or id).
    pub target_connection: String,
    /// Target schema; defaults to `public` on Postgres/Redshift, the
    /// connection's own database otherwise.
    #[serde(default)]
    pub target_schema: Option<String>,
    /// Target table name; defaults to the source table name.
    #[serde(default)]
    pub target_table: Option<String>,
    /// `create` (default) | `append` | `replace`.
    #[serde(default)]
    pub mode: Option<String>,
    /// Source catalog on Snowflake, Databricks or BigQuery.
    #[serde(default)]
    pub source_catalog: Option<String>,
    /// Target catalog on Snowflake, Databricks or BigQuery.
    #[serde(default)]
    pub target_catalog: Option<String>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if ctx.read_only {
        anyhow::bail!(
            "database mutations are disabled for this session (read-only MCP server or a chat \
             profile without \"Allow writes\")"
        );
    }
    let src_conn = ctx.find_db_connection(&p.source_connection)?;
    let tgt_conn = ctx.find_db_connection(&p.target_connection)?;
    octa::db::reject_catalog(src_conn.engine, p.source_catalog.as_deref())?;
    octa::db::reject_catalog(tgt_conn.engine, p.target_catalog.as_deref())?;
    let mode = match p.mode.as_deref() {
        None | Some("create") => octa::db::DbWriteMode::Create,
        Some("append") => octa::db::DbWriteMode::Append,
        Some("replace") => octa::db::DbWriteMode::Replace,
        Some(other) => anyhow::bail!("mode must be create, append, or replace (got '{other}')"),
    };
    let target_schema = match &p.target_schema {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => match tgt_conn.engine {
            octa::db::DbEngine::Postgres | octa::db::DbEngine::Redshift => "public".to_string(),
            _ => tgt_conn.database.clone(),
        },
    };
    let target_table = match &p.target_table {
        Some(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => p.source_table.clone(),
    };

    let src_secret = ctx.db_secret(&src_conn);
    let tgt_secret = ctx.db_secret(&tgt_conn);
    let source = octa::db::copy::DbCopyEnd {
        conn: src_conn,
        catalog: p.source_catalog.clone(),
        schema: p.source_schema.clone(),
        table: p.source_table.clone(),
    };
    let target = octa::db::copy::DbCopyEnd {
        conn: tgt_conn,
        catalog: p.target_catalog.clone(),
        schema: target_schema.clone(),
        table: target_table.clone(),
    };
    let report = octa::db::copy::copy_table(
        &source,
        src_secret.as_deref(),
        &target,
        tgt_secret.as_deref(),
        mode,
    )?;
    Ok(json!({
        "source": format!("{}.{} @ {}", source.schema, source.table, source.conn.name),
        "target": format!("{target_schema}.{target_table} @ {}", target.conn.name),
        "rows_copied": report.rows_copied,
        "created": report.created,
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("copy_db_table failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(name: &str, engine: octa::db::DbEngine, allow_writes: bool) -> octa::db::DbConnection {
        octa::db::DbConnection {
            id: format!("db-{name}"),
            name: name.into(),
            engine,
            host: "localhost".into(),
            port: engine.default_port(),
            database: "app".into(),
            username: "u".into(),
            auth: octa::db::DbAuth::Password,
            allow_writes,
            oauth_client_id: None,
            oauth_tenant: None,
        }
    }

    fn params() -> Params {
        Params {
            source_connection: "src".into(),
            source_schema: "app".into(),
            source_table: "t".into(),
            target_connection: "tgt".into(),
            target_schema: None,
            target_table: None,
            mode: None,
            source_catalog: None,
            target_catalog: None,
        }
    }

    #[test]
    fn read_only_ctx_refuses_before_any_network() {
        let conns = vec![
            conn("src", octa::db::DbEngine::MySql, false),
            conn("tgt", octa::db::DbEngine::Postgres, true),
        ];
        let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, conns, true);
        let err = run(&ctx, &params()).unwrap_err().to_string();
        assert!(err.contains("disabled for this session"), "{err}");
    }

    #[test]
    fn readonly_target_connection_is_refused() {
        let conns = vec![
            conn("src", octa::db::DbEngine::MySql, false),
            conn("tgt", octa::db::DbEngine::Postgres, false),
        ];
        let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, conns, false);
        let err = run(&ctx, &params()).unwrap_err().to_string();
        assert!(err.contains("Allow writes"), "{err}");
    }
}
