//! `octa --seasonality FILE --seasonality-x COL --seasonality-y COL
//! [--seasonality-season N]` - split one column over time into its trend, its
//! repeating pattern and the rest. The same split as the chart's Seasonality
//! to table.

use std::path::PathBuf;

use octa::data::forecast::series_for;
use octa::data::seasonality::{decompose, decomposition_table};

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub struct Args {
    pub path: PathBuf,
    pub x: String,
    pub y: String,
    pub season: Option<usize>,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<()> {
    let table = super::read_table(&args.path)?;
    let (series, kind) = series_for(
        &table,
        table.column_index(&args.x)?,
        table.column_index(&args.y)?,
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    let d =
        decompose(&series.points, kind, args.season).map_err(|e| anyhow::anyhow!(e.message()))?;
    write_table(&decomposition_table(&d, kind), format)?;
    eprintln!(
        "season length: {}, the pattern explains {:.0}% of the movement around the trend",
        d.season,
        d.strength * 100.0
    );
    Ok(())
}
