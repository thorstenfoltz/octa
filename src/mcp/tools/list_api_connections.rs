//! MCP tool: `list_api_connections` - list the saved REST/JSON API endpoints
//! (Settings -> API endpoints). Read-only; never touches the network.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "List the saved REST/JSON API endpoints (Settings -> API \
endpoints): name, base URL, default path, how it authenticates and how it pages. Use a \
connection's `name` with `query_api`. You cannot add an endpoint or call an arbitrary URL - \
only the endpoints the user saved are reachable. Read-only; does not contact any server.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {}

pub fn run(ctx: &ToolContext, _p: &Params) -> anyhow::Result<Value> {
    let connections: Vec<Value> = ctx
        .api_connections
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "base_url": c.base_url,
                "path": c.path,
                "auth": format!("{:?}", octa::api::ApiAuthKind::of(&c.auth)),
                "paging": format!("{:?}", octa::api::ApiPagingKind::of(&c.paging)),
                "records_path": c.records_pointer,
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
        .map_err(|e| McpError::invalid_params(format!("list_api_connections failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
