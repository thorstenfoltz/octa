//! MCP tool: `sample` - load a file and return a random N-row sample.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;

use octa::data::sample::sample_table;

use crate::mcp::OctaMcpServer;

use super::{read_with_registry, table_to_json};

// Tool description lives inline at the `#[tool]` site in `src/mcp/mod.rs`.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Absolute or working-directory-relative path to the file.
    pub path: PathBuf,

    /// Sample size. Default is the server's configured limit. Pass 0 for
    /// "every row" (no sampling).
    #[serde(default)]
    pub limit: Option<usize>,

    /// For multi-table sources, the specific table to load.
    #[serde(default)]
    pub table: Option<String>,

    /// Seed for reproducible sampling. Default 0.
    #[serde(default)]
    pub seed: u64,

    /// Lift the streaming initial-load cap so sampling sees every row on disk.
    /// Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let row_cap = server.resolve_row_cap(p.limit);
    let cell_cap = server.cell_byte_cap;
    let path = p.path.clone();
    let table_name = p.table.clone();
    let seed = p.seed;
    let unlimited = p.unlimited;

    let sampled = tokio::task::spawn_blocking(move || {
        let _g = unlimited.then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
        let dt = read_with_registry(&path, table_name.as_deref())?;
        // None (unlimited) means "every row"; otherwise sample `n`.
        let n = row_cap.unwrap_or_else(|| dt.row_count());
        anyhow::Ok(sample_table(&dt, n, seed))
    })
    .await
    .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
    .map_err(|e| McpError::invalid_params(format!("read failed: {e}"), None))?;

    // Rows are already the sample; serialise all of them.
    let payload = table_to_json(&sampled, None, cell_cap);
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
