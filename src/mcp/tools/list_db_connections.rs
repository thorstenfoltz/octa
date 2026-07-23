//! MCP tool: `list_db_connections` - list the saved live-database
//! connections (Settings -> Databases). Read-only; never touches the network.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "List the saved live-database connections (Settings -> Databases): \
name, engine (PostgreSQL / MySQL / SQL Server), host, port, database, and whether writes are \
allowed. Use a connection's `name` with `list_db_tables`, `query_db`, and `write_db_table`. \
Read-only; does not contact any server.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {}

pub fn run(ctx: &ToolContext, _p: &Params) -> anyhow::Result<Value> {
    let connections: Vec<Value> = ctx
        .db_connections
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "engine": c.engine.label(),
                "host": c.host,
                "port": c.port,
                "database": c.database,
                "allow_writes": c.allow_writes,
            })
        })
        .collect();
    Ok(json!({
        "count": connections.len(),
        "connections": connections,
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = run(&ctx, &p)
        .map_err(|e| McpError::invalid_params(format!("list_db_connections failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
