//! `octa --rfm FILE --rfm-customer COL --rfm-date COL --rfm-amount COL`:
//! customer segments, every customer marked 1 to 5 for how recently, how
//! often and how much they bought, and named. Same engine as the GUI dialog
//! and the MCP tool `rfm_segments`.

use std::path::PathBuf;

use chrono::NaiveDate;
use octa::data::rfm::{Names, RfmSpec, collect, table};

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub struct Args {
    pub path: PathBuf,
    pub customer: String,
    pub date: String,
    pub amount: String,
    pub order: Option<String>,
    pub as_of: Option<NaiveDate>,
    pub names: Names,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<()> {
    let data = super::read_table(&args.path)?;
    let spec = RfmSpec {
        customer: data.column_index(&args.customer)?,
        date: data.column_index(&args.date)?,
        amount: data.column_index(&args.amount)?,
        order: args
            .order
            .as_deref()
            .map(|c| data.column_index(c))
            .transpose()?,
        as_of: args.as_of,
    };
    let totals = collect(&data, &spec);
    let (result, as_of) = table(&totals, args.as_of, args.names)
        .map_err(|e| anyhow::anyhow!("--rfm: {}", e.message()))?;
    write_table(&result, format)?;
    eprintln!("recency counted to {as_of}");
    if totals.skipped > 0 {
        eprintln!(
            "{} rows had no customer or no readable date and were left out",
            totals.skipped
        );
    }
    Ok(())
}
