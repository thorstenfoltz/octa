//! MCP tool: `list_objects` - list one folder level of a cloud bucket
//! (S3/Azure/GCS) by URL. Read-only.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "List one folder level of a cloud object store by URL: \
`s3://bucket/prefix`, `az://container/prefix`, or `gs://bucket/prefix` (an empty/absent prefix \
lists the bucket root). Returns `objects` as an array of `{name, key, url, is_folder, size, \
modified}`; folders (common prefixes) have `is_folder: true`. Open a listed file with `read_table` \
(or any read tool) by passing its `url` as the `path`. Credentials: the MCP/CLI server uses ambient \
credentials (AWS_* env, a cached SSO session, Azure CLI login, or Google application-default \
credentials); the in-app assistant uses your saved cloud connections.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Cloud URL to list, e.g. `s3://my-bucket/data/` or `gs://bucket` for the
    /// root. Schemes: `s3://`, `az://`, `gs://`.
    pub url: String,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.url.trim().is_empty() {
        anyhow::bail!("`url` is required (e.g. s3://bucket/prefix)");
    }
    let (provider, loc) = ctx.cloud_provider_for(&p.url)?;
    let entries = provider.list(&loc.key)?;
    let objects: Vec<Value> = entries
        .iter()
        .map(|e| {
            json!({
                "name": e.name,
                "key": e.key,
                "url": format!("{}://{}/{}", loc.kind.scheme(), loc.bucket, e.key),
                "is_folder": e.is_prefix,
                "size": e.size,
                "modified": e.modified.map(|m| m.to_rfc3339()),
            })
        })
        .collect();
    Ok(json!({
        "url": p.url,
        "count": objects.len(),
        "objects": objects,
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("list_objects failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tools::ToolContext;

    #[test]
    fn empty_url_errors() {
        let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true);
        let p = Params { url: "  ".into() };
        assert!(run(&ctx, &p).is_err());
    }

    #[test]
    fn non_cloud_url_errors() {
        let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true);
        let p = Params {
            url: "/local/file.csv".into(),
        };
        let err = run(&ctx, &p).unwrap_err().to_string();
        assert!(err.contains("not a cloud URL"), "{err}");
    }
}
