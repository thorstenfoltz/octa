//! MCP tool: `rfm_segments` - customer segments (RFM): every customer marked
//! 1 to 5 for how recently, how often and how much they bought, and named.
//! Read-only; the same engine as the GUI's Analyse > Customer segments
//! (RFM)... and `octa --rfm`.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::rfm::{Names, RfmSpec, collect, table};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Customer segments (RFM): every `customer` gets three marks from \
1 to 5, recency (fewer days since the last purchase is better), frequency (orders) and monetary \
(the summed `amount`, exact), as quintiles where equal values share a mark. `date` is a date, \
timestamp or ISO text; `order` makes rows with the same order number one order, a row without \
one its own (default: every row is one); `as_of` (a date) is the day the segments are made for, \
later purchases left out (default: the latest date). \
`names`: `seven` (default: Best, Loyal, Promising, New, Needs attention, Slipping away, Lost), \
`eleven` (Champions, Loyal customers, ..., Can't lose them, Lost) or `none`; the name is read \
off recency and the average of frequency and monetary. Rows without a customer or a readable \
date are left out and counted in `skipped`. Returns `{as_of, skipped, table}`; columns \
`customer, last_date, days_since, orders, amount, recency, frequency, monetary, segment`, \
largest amount first.";

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
    /// The column naming a customer.
    pub customer: String,
    /// The date or timestamp of each purchase.
    pub date: String,
    /// What each row was worth.
    pub amount: String,
    /// Rows with the same value here are one order. Omit: every row is one.
    #[serde(default)]
    pub order: Option<String>,
    /// The day recency counts to, e.g. `2024-12-31`. Omit: the latest date.
    #[serde(default)]
    pub as_of: Option<String>,
    /// `seven` (default), `eleven` or `none`.
    #[serde(default)]
    pub names: Option<String>,
    /// At most this many customers in the answer; 0 for all.
    #[serde(default)]
    pub limit: Option<usize>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let data = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let as_of = p
        .as_of
        .as_deref()
        .map(|s| {
            octa::data::time_calc::parse_date(s)
                .ok_or_else(|| anyhow::anyhow!("`as_of` is not a date: {s}"))
        })
        .transpose()?;
    let spec = RfmSpec {
        customer: data.column_index(&p.customer)?,
        date: data.column_index(&p.date)?,
        amount: data.column_index(&p.amount)?,
        order: p
            .order
            .as_deref()
            .map(|c| data.column_index(c))
            .transpose()?,
        as_of,
    };
    let names = match p.names.as_deref() {
        None => Names::default(),
        Some(s) => Names::parse(s)
            .ok_or_else(|| anyhow::anyhow!("`names` takes seven, eleven or none, not {s}"))?,
    };
    let totals = collect(&data, &spec);
    let (result, as_of) =
        table(&totals, as_of, names).map_err(|e| anyhow::anyhow!("{}", e.message()))?;
    Ok(json!({
        "as_of": as_of.format("%Y-%m-%d").to_string(),
        "skipped": totals.skipped,
        "table": table_to_json(&result, ctx.resolve_row_cap(p.limit), ctx.cell_byte_cap),
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("rfm_segments failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
