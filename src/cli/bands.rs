//! `octa --bands FILE --bands-column COL [--bands-by width|count|edges]
//! [--bands-count N] [--bands-edges LIST] [--bands-back middle|lower|upper]`:
//! add a column of number bands, or of numbers read back from bands, and
//! print the table. Never modifies FILE. Same engine as the GUI's Number
//! bands and the MCP tool `number_bands`.

use std::path::PathBuf;

use octa::data::bands::{BandSpec, add_band_column};

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub struct Args {
    pub path: PathBuf,
    pub column: String,
    pub spec: BandSpec,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<()> {
    let mut table = super::read_table(&args.path)?;
    let col = table.column_index(&args.column)?;
    let (_, unread) = add_band_column(&mut table, col, &args.spec)
        .map_err(|e| anyhow::anyhow!("--bands: {}", octa::i18n::t(e.key())))?;
    table.apply_edits();
    write_table(&table, format)?;
    if unread > 0 {
        eprintln!("{unread} labels held no number and stayed empty");
    }
    Ok(())
}
