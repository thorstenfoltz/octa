//! MCP tool: `query_api` - read a saved REST/JSON endpoint as a table.
//!
//! The endpoint is named, never supplied. `path` is joined under the saved
//! base URL (see `ApiConnection::url_for`), so a model cannot move the request
//! to another host however it spells the argument: the human who saved the
//! connection chose the host, and that is the only place it is chosen.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::Value;

use octa::api::client;
use octa::ui::settings::api_secrets;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, table_to_json};

pub const DESCRIPTION: &str = "Read a saved REST/JSON API endpoint as a table (see \
`list_api_connections` for the names). Handles the endpoint's authentication and walks its \
pagination, so you get every page's rows, not just the first. `path` is optional and is joined \
under the saved base URL - you cannot point this at an arbitrary address, only at a path of an \
endpoint the user already saved. Use `records_path` only to override which array in the \
response holds the rows (a JSON pointer like `/data/items`); leaving it out uses the \
connection's own setting.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Saved connection name or id.
    pub connection: String,
    /// Path under the endpoint's base URL, overriding the connection's own.
    #[serde(default)]
    pub path: Option<String>,
    /// JSON pointer to the array of rows, overriding the connection's own.
    #[serde(default)]
    pub records_path: Option<String>,
    /// Cap the rows returned. Omitted uses the server's default.
    #[serde(default)]
    pub limit: Option<usize>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let mut conn = octa::api::find_connection(&ctx.api_connections, &p.connection)?;
    if let Some(rp) = &p.records_path {
        conn.records_pointer = rp.clone();
    }
    // Settings are re-read here rather than carried on the context: the
    // credential should not sit in a struct that every tool call holds.
    let settings = octa::ui::settings::AppSettings::load();
    let secret = api_secrets::get_api_secret(&conn.id, &settings);

    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let opts = client::FetchOptions {
        path: p.path.clone(),
        max_rows: None,
        max_pages: None,
    };
    let out = client::fetch_table(&conn, secret.as_deref(), &opts, &cancel)?;
    let limit = p.limit.or(ctx.default_row_limit);
    let mut payload = table_to_json(&out.table, limit, ctx.cell_byte_cap);
    if let Value::Object(map) = &mut payload {
        map.insert("pages_read".to_string(), Value::from(out.pages));
        if out.capped {
            map.insert(
                "note".to_string(),
                Value::String(
                    "The read stopped on a row or page limit, so this is not the whole \
                     endpoint. Say so rather than implying it is complete."
                        .to_string(),
                ),
            );
        }
    }
    Ok(payload)
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = run(&ctx, &p)
        .map_err(|e| McpError::invalid_params(format!("query_api failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
