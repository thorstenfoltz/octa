//! `octa --reference FILE --ref-mode currency|inflation|places ...`: the
//! same lookups as Data > Reference data... and the MCP tool
//! `reference_data`. Downloads what is due for the switched-on sources,
//! never modifies FILE, prints the table with the new columns.

use std::path::PathBuf;

use chrono::NaiveDate;

use octa::data::DataTable;
use octa::data::reference::currency::{CurrencySpec, Mark};
use octa::data::reference::inflation::InflationSpec;
use octa::data::reference::places::{self, KeyKind, Output, PlacesSpec};
use octa::data::reference::{Pick, Rounding, Several, insert_columns};
use octa::reference::{Request, lookup};
use octa::ui::settings::AppSettings;

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub enum Mode {
    Currency {
        amount: String,
        from: String,
        to: String,
        date: Option<String>,
        rounding: Rounding,
        mark: Mark,
    },
    Inflation {
        amount: String,
        date: String,
        country: String,
        target: Option<NaiveDate>,
        rounding: Rounding,
    },
    Places {
        key: String,
        kind: KeyKind,
        country: Option<String>,
        outputs: Option<Vec<Output>>,
        several: Several,
    },
}

#[derive(Debug)]
pub struct Args {
    pub path: PathBuf,
    pub mode: Mode,
}

/// A value naming a column is that column, anything else a fixed code.
fn pick(table: &DataTable, v: &str) -> Pick {
    table
        .column_index(v)
        .map(Pick::Column)
        .unwrap_or_else(|_| Pick::Fixed(v.to_string()))
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<()> {
    let settings = AppSettings::load();
    let r = &settings.reference;
    let mut table = super::read_table(&args.path)?;
    let req = match args.mode {
        Mode::Currency {
            amount,
            from,
            to,
            date,
            rounding,
            mark,
        } => Request::Currency(CurrencySpec {
            amount: table.column_index(&amount)?,
            from: pick(&table, &from),
            to,
            date: date.map(|d| table.column_index(&d)).transpose()?,
            rounding,
            mark,
        }),
        Mode::Inflation {
            amount,
            date,
            country,
            target,
            rounding,
        } => Request::Inflation(InflationSpec {
            amount: table.column_index(&amount)?,
            date: pick(&table, &date),
            country: pick(&table, &country),
            target,
            rounding,
        }),
        Mode::Places {
            key,
            kind,
            country,
            outputs,
            several,
        } => Request::Places(PlacesSpec {
            key: table.column_index(&key)?,
            kind,
            country: country
                .map(|c| pick(&table, &c))
                .unwrap_or(Pick::Fixed(String::new())),
            outputs: outputs.unwrap_or_else(|| {
                Output::ALL
                    .into_iter()
                    .filter(|o| places::available(r, *o))
                    .collect()
            }),
            several,
        }),
    };
    let (computed, warnings) = lookup(&table, &req, r)?;
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    insert_columns(&mut table, &computed);
    table.apply_edits();
    write_table(&table, format)?;
    eprintln!(
        "{} of {} rows found",
        computed.found,
        computed.found + computed.missed
    );
    Ok(())
}
