//! MCP tool: `db_relationships` - which tables on this server point at which,
//! according to the server itself? Read-only.
//!
//! The file-based sibling is `suggest_join_keys`, which infers a link from
//! values. This one reads the foreign keys somebody declared, which costs two
//! catalog queries and no table data at all.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "Read the foreign keys a live database declares, so you learn how \
its tables connect without reading a single row. Takes a saved `connection` (see \
`list_db_connections`), optional `catalog` for Snowflake/Databricks/BigQuery, and `schemas` \
(default: every schema). Returns `relationships`, each naming the child and parent table and \
column plus the `constraint` name. Postgres, MySQL, SQL Server, Oracle and Exasol enforce their foreign \
keys, so an edge from those is also true of the rows; Redshift, Snowflake, Databricks and \
BigQuery accept a declaration and enforce nothing. Pass `measure: true` to read a sample of rows \
and add `overlap`, `score` and orphan counts both ways round per edge, which is how you find a \
declared key nothing honours (a declared key is measured child to parent, so `left_orphans` is \
the child rows pointing at a parent that does not exist). ClickHouse has no foreign keys at all. Read-only.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Saved connection name (or id) from Settings -> Databases.
    pub connection: String,

    /// Catalog (top namespace level) on Snowflake, Databricks or BigQuery.
    /// An error on any other engine, which has no catalog level.
    #[serde(default)]
    pub catalog: Option<String>,

    /// Schemas to read. Empty: every schema the connection lists.
    #[serde(default)]
    pub schemas: Vec<String>,

    /// Draw only these `schema.table` labels. Absent: the tables that take
    /// part in a foreign key, which is what makes the answer a map rather
    /// than an inventory.
    #[serde(default)]
    pub tables: Option<Vec<String>>,

    /// Tables carried in the answer before it stops. Default 30.
    #[serde(default)]
    pub max_tables: Option<usize>,

    /// Also read a sample of rows and put real numbers on every edge.
    /// Default false: the declaration alone costs no table data.
    #[serde(default)]
    pub measure: bool,

    /// Rows sampled per table when `measure` is set. Default 10000.
    #[serde(default)]
    pub sample: Option<usize>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let conn = ctx.find_db_connection(&p.connection)?;
    if !conn.engine.has_foreign_keys() {
        anyhow::bail!(
            "{} has no foreign keys to read; use suggest_join_keys over exported files instead",
            conn.engine.label()
        );
    }
    octa::db::reject_catalog(conn.engine, p.catalog.as_deref())?;
    let mut c = ctx.db_connect(&conn)?;

    let catalog = p.catalog.as_deref();
    let schemas = if p.schemas.is_empty() {
        c.list_schemas(catalog)?
    } else {
        p.schemas.clone()
    };
    let (columns, fks) = octa::db::relationships::scan(c.as_mut(), catalog, &schemas)?;

    let wanted: Option<std::collections::HashSet<String>> =
        p.tables.as_ref().map(|t| t.iter().cloned().collect());
    let max_tables = p
        .max_tables
        .unwrap_or(octa::data::rel_map::DEFAULT_MAX_FILES);
    let built = octa::db::relationships::build_db_map(&columns, &fks, wanted.as_ref(), max_tables);
    let mut map = built.map;

    let sample = p
        .sample
        .unwrap_or(octa::data::join_keys::DEFAULT_SAMPLE_ROWS);
    if p.measure {
        // One connection for every table, and the same scorer the GUI and
        // suggest_join_keys use, so no two surfaces can report different
        // numbers for one edge.
        let mut tables: Vec<(String, octa::data::DataTable)> = Vec::new();
        for node in &map.nodes {
            let (schema, table) = node
                .name
                .split_once('.')
                .unwrap_or(("", node.name.as_str()));
            let sql = octa::db::select_sample_sql(conn.engine, catalog, schema, table, sample);
            tables.push((node.name.clone(), c.query(&sql)?));
        }
        octa::data::rel_map::score_edges(&tables, &mut map, sample);
    }

    let relationships: Vec<Value> = map
        .edges
        .iter()
        .map(|e| {
            let mut row = json!({
                "child": map.nodes[e.left_table].name,
                "child_column": map.nodes[e.left_table].columns[e.left_col],
                "parent": map.nodes[e.right_table].name,
                "parent_column": map.nodes[e.right_table].columns[e.right_col],
                "constraint": e.constraint,
                "measured": e.scored,
            });
            if e.scored {
                row["overlap"] = json!(e.overlap);
                row["score"] = json!(e.score);
                row["left_orphans"] = json!(e.left_orphans);
                row["left_distinct_values"] = json!(e.left_distinct_values);
                row["right_orphans"] = json!(e.right_orphans);
                row["right_distinct_values"] = json!(e.right_distinct_values);
            }
            row
        })
        .collect();

    Ok(json!({
        "connection": conn.name,
        "engine": conn.engine.label(),
        // A declaration and a measurement are different claims, and four of
        // the ten engines never enforce one.
        "enforced": conn.engine.enforces_foreign_keys(),
        "schemas": schemas,
        "tables": map.nodes.iter().map(|n| &n.name).collect::<Vec<_>>(),
        "count": relationships.len(),
        "relationships": relationships,
        "skipped_edges": built.skipped_edges,
        "truncated": built.truncated,
        "measured": p.measure,
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("db_relationships failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_connection_is_required() {
        let p: Params = serde_json::from_value(json!({ "connection": "warehouse" })).unwrap();
        assert!(p.schemas.is_empty());
        assert!(p.tables.is_none());
        // Measuring reads table data, so it must never be the default.
        assert!(!p.measure);
    }

    #[test]
    fn params_accept_a_catalog_and_an_explicit_table_set() {
        let p: Params = serde_json::from_value(json!({
            "connection": "wh",
            "catalog": "sales_prod",
            "schemas": ["analytics"],
            "tables": ["analytics.orders", "analytics.customers"],
            "measure": true,
        }))
        .unwrap();
        assert_eq!(p.catalog.as_deref(), Some("sales_prod"));
        assert_eq!(p.tables.unwrap().len(), 2);
        assert!(p.measure);
    }
}
