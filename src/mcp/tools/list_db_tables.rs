//! MCP tool: `list_db_tables` - list the schemas and tables of one saved
//! live-database connection. Read-only.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "List the schemas and tables of a saved live-database connection \
(see `list_db_connections` for the names). Pass `schema` to list only that schema's tables. \
Returns `tables` as an array of `{schema, table}`. On Snowflake, Databricks and BigQuery there \
is a catalog level above the schema: calling this without `catalog` returns `kind: \"catalogs\"` \
and the catalog list, so call it again with `catalog` set to drill down. Query a listed table \
with `query_db` (e.g. SELECT * FROM schema.table). Read-only.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Saved connection name (or id) from Settings -> Databases.
    pub connection: String,
    /// Restrict the listing to one schema. Absent: every schema.
    #[serde(default)]
    pub schema: Option<String>,
    /// Catalog (top namespace level) on Snowflake, Databricks or BigQuery.
    /// Absent on those engines: the catalogs themselves are listed. An error
    /// on any other engine, which has no catalog level.
    #[serde(default)]
    pub catalog: Option<String>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let conn = ctx.find_db_connection(&p.connection)?;
    octa::db::reject_catalog(conn.engine, p.catalog.as_deref())?;
    let mut c = ctx.db_connect(&conn)?;

    // A catalog engine with no catalog chosen: hand back the catalogs rather
    // than an empty listing. A model that receives an empty result concludes
    // the database is empty and stops, instead of drilling down.
    if conn.engine.has_catalogs() && p.catalog.is_none() {
        let catalogs = c.list_catalogs()?;
        return Ok(json!({
            "connection": conn.name,
            "kind": "catalogs",
            "count": catalogs.len(),
            "catalogs": catalogs,
            "note": "This engine has a catalog level. Call list_db_tables again \
                     with `catalog` set to one of these to list its tables.",
        }));
    }

    let catalog = p.catalog.as_deref();
    let schemas = match &p.schema {
        Some(s) => vec![s.clone()],
        None => c.list_schemas(catalog)?,
    };
    let mut tables: Vec<Value> = Vec::new();
    for schema in &schemas {
        for table in c.list_tables(catalog, schema)? {
            match catalog {
                Some(cat) => {
                    tables.push(json!({ "catalog": cat, "schema": schema, "table": table }))
                }
                None => tables.push(json!({ "schema": schema, "table": table })),
            }
        }
    }
    Ok(json!({
        "connection": conn.name,
        "kind": "tables",
        "count": tables.len(),
        "tables": tables,
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("list_db_tables failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_accept_a_catalog() {
        let p: Params = serde_json::from_value(json!({
            "connection": "wh",
            "catalog": "sales_prod",
            "schema": "analytics",
        }))
        .unwrap();
        assert_eq!(p.catalog.as_deref(), Some("sales_prod"));
        assert_eq!(p.schema.as_deref(), Some("analytics"));
    }

    #[test]
    fn catalog_is_optional() {
        let p: Params = serde_json::from_value(json!({ "connection": "wh" })).unwrap();
        assert!(p.catalog.is_none());
    }
}
