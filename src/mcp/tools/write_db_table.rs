//! MCP tool: `write_db_table` - write a file or open tab into a table on a
//! saved live-database connection. Removed under `--mcp-read-only`; always
//! gated on the connection's allow-writes switch.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Write a table into a saved live-database connection (see \
`list_db_connections`): source rows come from `path` (any readable file, cloud URLs included) \
or `open_tab` (in-GUI assistant only). Target is `schema` + `table`; `mode` is `create` \
(default, error if the table exists), `append`, or `replace` (DROP + CREATE). Refused unless \
the connection's \"Allow writes\" switch is on. Returns `{rows_written, created}`. \
On Snowflake, Databricks and BigQuery pass `catalog` for the top namespace level.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Saved connection name (or id) from Settings -> Databases.
    pub connection: String,
    /// Target schema on the server (e.g. `public`, `dbo`, or the database
    /// name on MySQL).
    pub schema: String,
    /// Target table name.
    pub table: String,
    /// `create` (default) | `append` | `replace`.
    #[serde(default)]
    pub mode: Option<String>,
    /// Source file to read (any supported format).
    #[serde(default)]
    pub path: Option<String>,
    /// Source open tab (in-GUI assistant only): `@active`, a tab name, or a
    /// handle like `#2`.
    #[serde(default)]
    pub open_tab: Option<String>,
    /// Catalog (top namespace level) on Snowflake, Databricks or BigQuery.
    /// An error on any other engine.
    #[serde(default)]
    pub catalog: Option<String>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let conn = ctx.find_db_connection(&p.connection)?;
    octa::db::ensure_write_allowed(&conn, None)?;
    octa::db::reject_catalog(conn.engine, p.catalog.as_deref())?;
    let mode = match p.mode.as_deref() {
        None | Some("create") => octa::db::DbWriteMode::Create,
        Some("append") => octa::db::DbWriteMode::Append,
        Some("replace") => octa::db::DbWriteMode::Replace,
        Some(other) => anyhow::bail!("mode must be create, append, or replace (got '{other}')"),
    };
    let path = PathBuf::from(p.path.clone().unwrap_or_default());
    let source = source_from(&p.open_tab, &path, &None);
    let data = ctx.resolve(&source)?;
    let mut c = ctx.db_connect(&conn)?;
    let report = c.write_table(p.catalog.as_deref(), &p.schema, &p.table, mode, &data)?;
    let target = match &p.catalog {
        Some(cat) => format!("{cat}.{}.{}", p.schema, p.table),
        None => format!("{}.{}", p.schema, p.table),
    };
    Ok(json!({
        "connection": conn.name,
        "target": target,
        "rows_written": report.rows_written,
        "created": report.created,
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("write_db_table failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_refused_without_allow_writes() {
        let conn = octa::db::DbConnection {
            id: "db-1".into(),
            name: "wh".into(),
            engine: octa::db::DbEngine::Postgres,
            host: "localhost".into(),
            port: 5432,
            database: "app".into(),
            username: "u".into(),
            auth: octa::db::DbAuth::Password,
            allow_writes: false,
            oauth_client_id: None,
            oauth_tenant: None,
        };
        let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, vec![conn], false);
        let p = Params {
            connection: "wh".into(),
            schema: "public".into(),
            table: "t".into(),
            mode: None,
            path: Some("whatever.csv".into()),
            open_tab: None,
            catalog: None,
        };
        let err = run(&ctx, &p).unwrap_err().to_string();
        assert!(err.to_lowercase().contains("write"), "{err}");
    }

    #[test]
    fn bad_mode_errors_before_any_read() {
        let conn = octa::db::DbConnection {
            id: "db-1".into(),
            name: "wh".into(),
            engine: octa::db::DbEngine::Postgres,
            host: "localhost".into(),
            port: 5432,
            database: "app".into(),
            username: "u".into(),
            auth: octa::db::DbAuth::Password,
            allow_writes: true,
            oauth_client_id: None,
            oauth_tenant: None,
        };
        let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, vec![conn], false);
        let p = Params {
            connection: "wh".into(),
            schema: "public".into(),
            table: "t".into(),
            mode: Some("upsert".into()),
            path: Some("whatever.csv".into()),
            open_tab: None,
            catalog: None,
        };
        let err = run(&ctx, &p).unwrap_err().to_string();
        assert!(err.contains("upsert"), "{err}");
    }
}
