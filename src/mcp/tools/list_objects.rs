//! MCP tool: `list_objects` - list one folder level of a cloud bucket
//! (S3/Azure/GCS) by URL. Read-only.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "List a cloud object store by URL: \
`s3://bucket/prefix`, `az://container/prefix`, or `gs://bucket/prefix` (an empty/absent prefix \
lists the bucket root). Default: one folder level. Pass `recursive: true` to flatten everything \
under the prefix (no folder entries; capped at 100,000 objects, `truncated: true` when hit). \
Returns `objects` as an array of `{name, key, url, is_folder, size, modified, etag, version}`; \
folders (common prefixes) have `is_folder: true`. Open a listed file with `read_table` \
(or any read tool) by passing its `url` as the `path`. Credentials: the MCP/CLI server uses ambient \
credentials (AWS_* env, a cached SSO session, Azure CLI login, or Google application-default \
credentials); the in-app assistant uses your saved cloud connections.";

/// Recursive-listing cap, mirroring the GUI inventory's.
const RECURSIVE_CAP: usize = 100_000;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Cloud URL to list, e.g. `s3://my-bucket/data/` or `gs://bucket` for the
    /// root. Schemes: `s3://`, `az://`, `gs://`.
    pub url: String,
    /// When true, list everything under the prefix recursively (flat, files
    /// only, capped at 100,000 objects). Default false: one folder level.
    #[serde(default)]
    pub recursive: Option<bool>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.url.trim().is_empty() {
        anyhow::bail!("`url` is required (e.g. s3://bucket/prefix)");
    }
    let (provider, loc) = ctx.cloud_provider_for(&p.url)?;
    let (entries, truncated) = if p.recursive == Some(true) {
        provider.list_recursive(&loc.key, RECURSIVE_CAP)?
    } else {
        (provider.list(&loc.key)?, false)
    };
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
                "etag": e.etag,
                "version": e.version,
            })
        })
        .collect();
    Ok(json!({
        "url": p.url,
        "count": objects.len(),
        "truncated": truncated,
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
        let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false);
        let p = Params {
            url: "  ".into(),
            recursive: None,
        };
        assert!(run(&ctx, &p).is_err());
    }

    #[test]
    fn empty_url_errors_recursive_too() {
        let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false);
        let p = Params {
            url: "".into(),
            recursive: Some(true),
        };
        assert!(run(&ctx, &p).is_err());
    }

    #[test]
    fn non_cloud_url_errors() {
        let ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false);
        let p = Params {
            url: "/local/file.csv".into(),
            recursive: None,
        };
        let err = run(&ctx, &p).unwrap_err().to_string();
        assert!(err.contains("not a cloud URL"), "{err}");
    }
}
