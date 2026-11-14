//! MCP tool: `number_bands` - numbers into labelled bands (`18-34`, `65+`),
//! or labelled bands back into numbers. Read-only; the same engine as the
//! GUI's Columns > Number bands... and `octa --bands`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::DataTable;
use octa::data::bands::{Back, BandBy, BandSpec, add_band_column};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Number bands. Without `back`: puts each number of `column` into \
a band and writes its label, by `by`: `width` (default, `count` bands of equal width), `count` \
(`count` bands with about as many values each: 4 quartiles, 10 deciles) or `edges` (bands between \
the given `edges`, `<first` below and `last+` from the last). Whole numbers get labels like \
`18-34` (both ends included), others `[1.5, 3)`. With `back` (`middle`, `lower` or `upper`): reads \
band labels in any common shape (`18-24`, `18 to 24`, `[18, 25)`, `10,000-19,999`, `20k`, `65+`, \
`<18`, or a lone number with words such as `65 and over`, placed by the other bands) and gives \
that number of each band; an open band takes the width of the band next to it. Returns \
`{column, unread, table}`: the source column and the new one, `unread` counting labels without a \
number. Changes no file.";

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
    /// The column to band, or to read back.
    pub column: String,
    /// `width` (default), `count` or `edges`.
    #[serde(default)]
    pub by: Option<String>,
    /// How many bands for `width` and `count`. Default 5.
    #[serde(default)]
    pub count: Option<usize>,
    /// Where bands start, for `edges`.
    #[serde(default)]
    pub edges: Option<Vec<f64>>,
    /// Read bands back: `middle`, `lower` or `upper`.
    #[serde(default)]
    pub back: Option<String>,
    /// At most this many rows in the answer; 0 for all.
    #[serde(default)]
    pub limit: Option<usize>,
}

fn spec(p: &Params) -> anyhow::Result<BandSpec> {
    if let Some(b) = p.back.as_deref() {
        return Back::parse(b)
            .map(BandSpec::Back)
            .ok_or_else(|| anyhow::anyhow!("`back` takes middle, lower or upper, not {b}"));
    }
    let count = p.count.unwrap_or(5);
    let by =
        p.by.as_deref()
            .unwrap_or(if p.edges.is_some() { "edges" } else { "width" });
    if by == "edges" && p.count.is_some() {
        anyhow::bail!("`count` is for `by` width or count, not edges");
    }
    if by != "edges" && p.edges.is_some() {
        anyhow::bail!("`edges` goes with `by` edges");
    }
    Ok(BandSpec::ToBands(match by {
        "width" => BandBy::Width(count),
        "count" => BandBy::Count(count),
        "edges" => BandBy::Edges(p.edges.clone().unwrap_or_default()),
        other => anyhow::bail!("`by` takes width, count or edges, not {other}"),
    }))
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let mut data = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let col = data.column_index(&p.column)?;
    let (name, unread) = add_band_column(&mut data, col, &spec(p)?)
        .map_err(|e| anyhow::anyhow!("{}", octa::i18n::t(e.key())))?;
    data.apply_edits();
    let pair = DataTable {
        columns: data.columns[col..=col + 1].to_vec(),
        rows: data
            .rows
            .iter()
            .map(|r| r[col..=col + 1].to_vec())
            .collect(),
        ..DataTable::empty()
    };
    Ok(json!({
        "column": name,
        "unread": unread,
        "table": table_to_json(&pair, ctx.resolve_row_cap(p.limit), ctx.cell_byte_cap),
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("number_bands failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
