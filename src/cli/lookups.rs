//! `octa --lookups FILE [--lookups-min 0.95]` - columns that always follow
//! another column (a customer's name and city following its ID). One row per
//! key and following column, with how consistently it follows.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use octa::data::lookups::{DEFAULT_MIN_CONSISTENCY, find_lookups, findings_table};

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub struct Args {
    pub path: PathBuf,
    pub min: Option<f64>,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<()> {
    let min = args.min.unwrap_or(DEFAULT_MIN_CONSISTENCY);
    anyhow::ensure!(
        (0.0..=1.0).contains(&min),
        "--lookups-min must be between 0 and 1"
    );
    let table = super::read_table(&args.path)?;
    let findings = find_lookups(&table, min, &AtomicBool::new(false));
    write_table(&findings_table(&table, &findings), format)?;
    eprintln!("{} key column(s) decide other columns", findings.len());
    if let Some((loaded, total)) = table.partial_note() {
        eprintln!(
            "note: scanned the first {loaded} rows of {}",
            total.map_or("more".into(), |t| t.to_string())
        );
    }
    Ok(())
}
