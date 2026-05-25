//! `octa --head <FILE> [-n N]` — print the first N rows of a file.
//!
//! For streaming readers (Parquet/CSV/TSV) the file loads with the standard
//! `initial_load_rows` cap; `--head -n` then slices the head off that. So
//! `octa --head huge.parquet -n 10` is fast because the reader itself stops
//! early via the cap; the slice is just a vector truncate.

use std::path::PathBuf;

use octa::data::DataTable;

use super::OutputFormat;
use super::output::write_table;

pub fn run(path: PathBuf, n: usize, format: OutputFormat) -> anyhow::Result<()> {
    let mut table = super::read_table(&path)?;
    truncate_to(&mut table, n);
    write_table(&table, format)?;
    Ok(())
}

fn truncate_to(table: &mut DataTable, n: usize) {
    if table.row_count() > n {
        table.rows.truncate(n);
        // total_rows is the "what's still available beyond what's loaded"
        // marker; once we've sliced to head, the value loses meaning.
        // Drop it so downstream code doesn't show a misleading "+N more".
        table.total_rows = None;
    }
}
