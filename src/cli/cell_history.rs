//! `octa --cell-history FILE --history-column COL (--history-key K
//! --history-value V | --history-row N)` - the commits that changed one
//! cell, newest first. Follows the row by key, so a re-sorted file is not
//! reported as every row changing.

use std::path::PathBuf;

use octa::data::cell_history::{cell_history, history_table, resolve_row};
use octa::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES;

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub struct Args {
    pub path: PathBuf,
    pub column: String,
    pub keys: Vec<String>,
    pub values: Vec<String>,
    pub row: Option<usize>,
    pub depth: usize,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<()> {
    let loaded = octa::git::history::versions_with_working_copy(
        &args.path,
        args.depth,
        DEFAULT_MAX_DECOMPRESSED_BYTES,
    )?;
    let row = resolve_row(&loaded.versions, args.keys, args.values, args.row)?;
    let h = cell_history(&loaded.versions, &row, &args.column);
    write_table(&history_table(&h), format)?;
    for c in &h.positional {
        eprintln!(
            "note: key not unique or missing in {}, matched by position there",
            c.sha
        );
    }
    for (c, e) in &loaded.unreadable {
        eprintln!("note: could not read {}: {e}", c.sha);
    }
    if loaded.more {
        eprintln!("note: older commits exist; raise --history-depth to see them");
    }
    Ok(())
}
