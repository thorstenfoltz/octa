//! MCP tool: `reference_data` - currency at the row's date, In today's
//! money, places and region keys. The same lookups as Data > Reference
//! data... and `octa --reference`. Changes no file.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::DataTable;
use octa::data::reference::currency::{CurrencySpec, Mark};
use octa::data::reference::inflation::InflationSpec;
use octa::data::reference::places::{self, KeyKind, Output, PlacesSpec};
use octa::data::reference::{Pick, Rounding, Several, insert_columns};
use octa::reference::{Request, lookup};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Reference data, downloaded on request from public sources once the user \
switched them on in Octa's Settings > Online data (off by default; the call is refused naming the \
settings key while off). `mode`: `currency` converts `amount` from `from` (a column, or one code such as \
EUR or a name such as Euro) to `to` (code or name) at the rate of `date` (a column; omit for the newest rate), BIS rates per US \
dollar, a weekend taking the last rate before, a currency without daily rates its monthly average, \
plus a column naming the target currency by `currency_mark` `code` (default) or `symbol` (only where \
the symbol belongs to that currency alone); \
`inflation` expresses `amount` from the month of `date` (a column, or one date or YYYY-MM month for every row) in the prices of `target_month` (YYYY-MM, default \
the newest month every country has) for `country` (a column or one ISO code, XM = euro area), BIS \
consumer price index; `places` looks up `key` of `key_kind` (`postcode`, `municipality`, `kreis`, `nuts`) \
in `country` and adds `outputs` (place, region, latitude, longitude, postcode, municipality, \
municipality_name, kreis, kreis_name, nuts1, nuts1_name, nuts2, nuts2_name, nuts3, nuts3_name; default all \
available), several matches joined (`several` = `all`, default) or the first alphabetically (`first`). \
`rounding`: `usual` (currency default: the currency's usual decimals), `none`, or a number (inflation \
default 2). A value naming a column is that column, anything else a code. Returns `{columns, found, \
missed, warnings, table}`: the source column and the new ones. Changes no file.";

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
    /// `currency`, `inflation` or `places`.
    pub mode: String,
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    /// YYYY-MM.
    #[serde(default)]
    pub target_month: Option<String>,
    /// `usual`, `none` or a number of decimals.
    #[serde(default)]
    pub rounding: Option<String>,
    /// Currency: `code` (default) or `symbol` in the currency column.
    #[serde(default)]
    pub currency_mark: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    /// `postcode` (default), `municipality`, `kreis` or `nuts`.
    #[serde(default)]
    pub key_kind: Option<String>,
    #[serde(default)]
    pub outputs: Option<Vec<String>>,
    /// `all` (default) or `first`.
    #[serde(default)]
    pub several: Option<String>,
    /// At most this many rows in the answer; 0 for all.
    #[serde(default)]
    pub limit: Option<usize>,
}

fn pick(table: &DataTable, v: &str) -> Pick {
    table
        .column_index(v)
        .map(Pick::Column)
        .unwrap_or_else(|_| Pick::Fixed(v.to_string()))
}

fn need<'a>(v: &'a Option<String>, what: &str) -> anyhow::Result<&'a str> {
    v.as_deref()
        .ok_or_else(|| anyhow::anyhow!("`{what}` is needed for this mode"))
}

fn rounding(p: &Params, default: Rounding) -> anyhow::Result<Rounding> {
    match p.rounding.as_deref() {
        None => Ok(default),
        Some(s) => Rounding::parse(s)
            .ok_or_else(|| anyhow::anyhow!("`rounding` takes usual, none or a number, not {s}")),
    }
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let settings = octa::ui::settings::AppSettings::load();
    let r = &settings.reference;
    let mut data = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let req = match p.mode.as_str() {
        "currency" => Request::Currency(CurrencySpec {
            amount: data.column_index(need(&p.amount, "amount")?)?,
            from: pick(&data, need(&p.from, "from")?),
            to: need(&p.to, "to")?.to_string(),
            date: p
                .date
                .as_deref()
                .filter(|d| *d != "latest")
                .map(|d| data.column_index(d))
                .transpose()?,
            rounding: rounding(p, Rounding::Usual)?,
            mark: match p.currency_mark.as_deref() {
                None => Mark::Code,
                Some(m) => Mark::parse(m).ok_or_else(|| {
                    anyhow::anyhow!("`currency_mark` takes code or symbol, not {m}")
                })?,
            },
        }),
        "inflation" => {
            let target = p
                .target_month
                .as_deref()
                .map(|m| chrono::NaiveDate::parse_from_str(&format!("{m}-01"), "%Y-%m-%d"))
                .transpose()
                .map_err(|_| anyhow::anyhow!("`target_month` is YYYY-MM"))?;
            Request::Inflation(InflationSpec {
                amount: data.column_index(need(&p.amount, "amount")?)?,
                date: pick(&data, need(&p.date, "date")?),
                country: pick(&data, need(&p.country, "country")?),
                target,
                rounding: rounding(p, Rounding::Decimals(2))?,
            })
        }
        "places" => {
            let kind =
                KeyKind::parse(p.key_kind.as_deref().unwrap_or("postcode")).ok_or_else(|| {
                    anyhow::anyhow!("`key_kind` takes postcode, municipality, kreis or nuts")
                })?;
            let outputs = match &p.outputs {
                Some(list) => list
                    .iter()
                    .map(|o| Output::parse(o).ok_or_else(|| anyhow::anyhow!("unknown output {o}")))
                    .collect::<anyhow::Result<Vec<_>>>()?,
                None => Output::ALL
                    .into_iter()
                    .filter(|o| places::available(r, *o))
                    .collect(),
            };
            Request::Places(PlacesSpec {
                key: data.column_index(need(&p.key, "key")?)?,
                kind,
                country: p
                    .country
                    .as_deref()
                    .map(|c| pick(&data, c))
                    .unwrap_or(Pick::Fixed(String::new())),
                outputs,
                several: if p.several.as_deref() == Some("first") {
                    Several::First
                } else {
                    Several::All
                },
            })
        }
        other => anyhow::bail!("`mode` takes currency, inflation or places, not {other}"),
    };
    let (computed, warnings) = lookup(&data, &req, r)?;
    let names = insert_columns(&mut data, &computed);
    data.apply_edits();
    let keep: Vec<usize> = std::iter::once(computed.after)
        .chain(computed.after + 1..computed.after + 1 + names.len())
        .collect();
    let slim = DataTable {
        columns: keep.iter().map(|&c| data.columns[c].clone()).collect(),
        rows: data
            .rows
            .iter()
            .map(|row| keep.iter().map(|&c| row[c].clone()).collect())
            .collect(),
        ..DataTable::empty()
    };
    Ok(json!({
        "columns": names,
        "found": computed.found,
        "missed": computed.missed,
        "warnings": warnings,
        "table": table_to_json(&slim, ctx.resolve_row_cap(p.limit), ctx.cell_byte_cap),
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("reference_data failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
