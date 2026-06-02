//! `octa --sample <FILE> [-n N] [--seed S]` - print a random N-row sample.
//!
//! Delegates to the pure `octa::data::sample::sample_table`, which samples
//! without replacement and preserves original row order. Output is
//! reproducible for a given `--seed`.

use std::path::PathBuf;

use octa::data::sample::sample_table;

use super::OutputFormat;
use super::output::write_table;

pub fn run(path: PathBuf, n: usize, seed: u64, format: OutputFormat) -> anyhow::Result<()> {
    let table = super::read_table(&path)?;
    let sampled = sample_table(&table, n, seed);
    write_table(&sampled, format)?;
    Ok(())
}
