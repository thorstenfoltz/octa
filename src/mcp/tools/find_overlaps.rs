//! MCP tool: `find_overlaps` - rows whose time spans overlap inside a lane.
//! Read-only; the same pairs the GUI's Timeline view and `--overlaps` show.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::timeline::{
    build, column_named, detect, lanes_with_overlaps, overlaps, overlaps_table,
};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Find rows whose time spans overlap: two bookings of one room, one \
person on two shifts. Each row runs from `start` to `end` (column names; default: the first two \
date columns, and no `end` makes every row a point). With `lane`, overlaps are only looked for \
among rows with the same value there. Spans that only touch (one ends as the next starts) do not \
overlap. Returns `{overlap_count, lanes_with_overlaps, backwards_rows, overlaps}`; `overlaps` has \
one row per pair with the lane and each side's 1-based row, label, start and end.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,
    /// An open GUI tab (name, or `@active`) instead of a file.
    #[serde(default)]
    pub open_tab: Option<String>,
    /// For multi-table sources, the table to read.
    #[serde(default)]
    pub table: Option<String>,
    /// Start column. Default: the first date column.
    #[serde(default)]
    pub start: Option<String>,
    /// End column. Default: the second date column, if there is one.
    #[serde(default)]
    pub end: Option<String>,
    /// Group rows by this column; overlaps are only looked for inside a group.
    #[serde(default)]
    pub lane: Option<String>,
    /// Column shown beside each row of a pair.
    #[serde(default)]
    pub label: Option<String>,
    /// Maximum pairs to return. Pass 0 for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let table = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let col = |n: &Option<String>| n.as_deref().map(|n| column_named(&table, n)).transpose();
    let (start, end) = match (col(&p.start)?, col(&p.end)?) {
        (Some(s), e) => (s, e),
        (None, e) => {
            let (s, d) = detect(&table)
                .ok_or_else(|| anyhow::anyhow!("no date column found; pass `start`"))?;
            (s, e.or(d))
        }
    };
    let rows: Vec<usize> = (0..table.row_count()).collect();
    let tl = build(&table, &rows, start, end, col(&p.label)?, col(&p.lane)?);
    let pairs = overlaps(&tl);

    let mut out = Map::new();
    out.insert("overlap_count".into(), Value::from(pairs.len()));
    out.insert(
        "lanes_with_overlaps".into(),
        Value::from(lanes_with_overlaps(&pairs, &tl)),
    );
    out.insert(
        "backwards_rows".into(),
        Value::from(tl.bad_rows.iter().map(|r| r + 1).collect::<Vec<_>>()),
    );
    out.insert(
        "overlaps".into(),
        table_to_json(
            &overlaps_table(&tl, &pairs),
            ctx.resolve_row_cap(p.limit),
            ctx.cell_byte_cap,
        ),
    );
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("find_overlaps failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
