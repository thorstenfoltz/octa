//! MCP tool: `compare_folders` - N folders, file by file. Read-only, so it
//! stays available under `--mcp-read-only`.

use std::sync::atomic::AtomicBool;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::folder_compare::any_difference;
use octa::data::folder_compare::scan::{FolderSource, cloud_prefix, compare_sources};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, table_to_json};

pub const DESCRIPTION: &str = "Compare two or more folders file by file. `folders` are local paths \
or cloud URLs (s3://, az://, gs://, sftp://, davs://). Files are matched by their path inside each \
folder; copies of equal size are compared by a BLAKE3 hash of their contents. Returns one row per \
file: path, status (same / different / missing), in, missing from, and size / modified / hash per \
folder; plus any_difference and the folders whose listing stopped at the file limit \
(truncated_folders). `unlimited: true` lists every file.";

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Two or more local folder paths or cloud URLs.
    pub folders: Vec<String>,
    /// Include subfolders. Default true.
    #[serde(default = "default_true")]
    pub recursive: bool,
    /// Response row cap; 0 = unlimited. Default: the server's row limit.
    #[serde(default)]
    pub limit: Option<usize>,
    /// List every file in every folder instead of stopping at the file limit
    /// from Settings (default 100,000 per folder).
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let sources = p
        .folders
        .iter()
        .map(|root| {
            let src = if octa::cloud::parse_cloud_url(root).is_some() {
                // The sandbox-aware resolver: the chat assistant reaches only
                // buckets the user saved.
                let (provider, loc) = ctx.cloud_provider_for(root)?;
                FolderSource::Cloud {
                    provider,
                    prefix: cloud_prefix(&loc.key),
                }
            } else {
                // The chat sandbox allows open files, never a folder, so the
                // assistant cannot list local folders; MCP proper can.
                let path = std::path::PathBuf::from(root);
                ctx.ensure_readable(&path)?;
                if !path.is_dir() {
                    anyhow::bail!("{root} is not a folder");
                }
                FolderSource::Local(path)
            };
            Ok((root.clone(), src))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let cap = if p.unlimited {
        usize::MAX
    } else {
        octa::ui::settings::AppSettings::load().folder_compare_cap()
    };
    let (table, listings) = compare_sources(
        sources,
        p.recursive,
        cap,
        &AtomicBool::new(false),
        &mut |_| {},
    )?;
    let truncated: Vec<&str> = listings
        .iter()
        .filter(|l| l.truncated)
        .map(|l| l.label.as_str())
        .collect();
    let mut out = table_to_json(&table, ctx.resolve_row_cap(p.limit), ctx.cell_byte_cap);
    if let Value::Object(map) = &mut out {
        map.insert("any_difference".into(), json!(any_difference(&table)));
        map.insert("truncated_folders".into(), json!(truncated));
    }
    Ok(out)
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("compare_folders failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
