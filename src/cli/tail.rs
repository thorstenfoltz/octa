//! `octa --tail <FILE> [-n N]` - print the last N rows of a file.
//!
//! Mirrors `--head` but slices from the end. Note: streaming readers load with
//! the standard `initial_load_rows` cap, so for very large files `--tail`
//! reflects the last N rows *within the loaded window*; raise the cap with
//! `--rows all` to tail the true end of a huge file.

use std::path::PathBuf;

use octa::data::DataTable;

use super::OutputFormat;
use super::output::write_table;

pub fn run(path: PathBuf, n: usize, format: OutputFormat) -> anyhow::Result<()> {
    let mut table = super::read_table(&path)?;
    keep_last(&mut table, n);
    write_table(&table, format)?;
    Ok(())
}

fn keep_last(table: &mut DataTable, n: usize) {
    let len = table.row_count();
    if len > n {
        table.rows.drain(0..len - n);
        // Once sliced to the tail, the "what's still available" marker loses
        // meaning (mirrors head::truncate_to).
        table.total_rows = None;
    }
}
