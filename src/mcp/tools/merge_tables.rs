//! MCP tool: `merge_tables` - merge two or more versions of a table,
//! optionally against their original. Read-only: it returns the merged rows
//! and the open conflicts; writing the result is `write_table`'s job.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::DataTable;
use octa::data::merge_versions::{conflict_table, merge_versions};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Merge two or more versions of a table (`versions`, each a `path` \
or an `open_tab`), per row and per cell. With an `original` (the table they were all edited from) \
a change made in one version is taken and only different changes to one cell, or a row deleted in \
one version and edited in another, conflict. Without it, every cell where the versions differ \
conflicts and rows from any version are kept. Rows are matched by `keys`, or by position when \
empty (`suggest_join_keys` finds one). `prefer` (1-based version number) settles every conflict. \
Returns `{merged, conflicts, conflict_count, status_counts}`; `merged` is present only once no \
conflict is open.";

/// One version: a file (may be a cloud URL) or an open GUI tab.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct Version {
    #[serde(default)]
    pub path: PathBuf,
    /// An open GUI tab (name, or `@active`) instead of a file.
    #[serde(default)]
    pub open_tab: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// The versions to merge, two or more (or one plus `original`).
    pub versions: Vec<Version>,
    /// The table every version was edited from. Optional.
    #[serde(default)]
    pub original: Option<Version>,
    /// Column(s) that identify a row, matched by name. Empty matches by
    /// row position.
    #[serde(default)]
    pub keys: Vec<String>,
    /// Settle every conflict in favour of this version (1 = the first).
    #[serde(default)]
    pub prefer: Option<usize>,
    /// Maximum merged rows to return. Pass 0 for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Lift the streaming initial-load cap so every row is read.
    #[serde(default)]
    pub unlimited: bool,
}

fn read(ctx: &ToolContext, v: &Version) -> anyhow::Result<DataTable> {
    ctx.resolve(&source_from(&v.open_tab, &v.path, &None))
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let original = p.original.as_ref().map(|v| read(ctx, v)).transpose()?;
    let versions: Vec<DataTable> = p
        .versions
        .iter()
        .map(|v| read(ctx, v))
        .collect::<anyhow::Result<_>>()?;
    let refs: Vec<&DataTable> = versions.iter().collect();
    let mut result = merge_versions(original.as_ref(), &refs, &p.keys)?;
    if let Some(n) = p.prefer {
        if n == 0 || n > versions.len() {
            anyhow::bail!("prefer takes 1 to {}", versions.len());
        }
        result.prefer_all(n - 1);
    }

    let mut counts = Map::new();
    for s in &result.status {
        let n = counts.entry(s.as_str()).or_insert(Value::from(0u64));
        *n = Value::from(n.as_u64().unwrap_or(0) + 1);
    }
    let mut out = Map::new();
    out.insert("conflict_count".into(), Value::from(result.unresolved()));
    out.insert(
        "conflicts".into(),
        table_to_json(&conflict_table(&result), None, ctx.cell_byte_cap),
    );
    out.insert("status_counts".into(), Value::Object(counts));
    if result.unresolved() == 0 {
        out.insert(
            "merged".into(),
            table_to_json(
                &result.finish()?,
                ctx.resolve_row_cap(p.limit),
                ctx.cell_byte_cap,
            ),
        );
    }
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("merge_tables failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
