//! `octa --forecast FILE --forecast-x COL --forecast-y COL
//! [--forecast-periods N] [--forecast-season N]` - Holt-Winters forecast of
//! one column over time, with 80% and 95% ranges. The same model as the
//! chart's Forecast.

use std::path::PathBuf;

use octa::data::forecast::{forecast, forecast_table, series_for};
use octa::data::timeline::column_named;

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub struct Args {
    pub path: PathBuf,
    pub x: String,
    pub y: String,
    pub periods: usize,
    pub season: Option<usize>,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<()> {
    let table = super::read_table(&args.path)?;
    let (series, kind) = series_for(
        &table,
        column_named(&table, &args.x)?,
        column_named(&table, &args.y)?,
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    let f = forecast(&series.points, kind, args.periods, args.season)
        .map_err(|e| anyhow::anyhow!(octa::i18n::t(e.i18n_key())))?;
    write_table(&forecast_table(&f, kind), format)?;
    eprintln!(
        "season length: {}",
        if f.season == 0 {
            "none".to_string()
        } else {
            f.season.to_string()
        }
    );
    Ok(())
}
