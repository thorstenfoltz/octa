//! `octa --impute COL=STRATEGY [--impute COL=STRATEGY ...] FILE`
//!
//! Fills missing/empty cells in one or more columns of a tabular file.
//! Each `--impute` flag takes a `COL=STRATEGY` pair:
//!
//! * `mean`        - arithmetic mean of non-null values (numeric columns).
//! * `median`      - median of non-null values (numeric columns).
//! * `mode`        - most frequent non-null value.
//! * `ffill`       - forward-fill from the previous non-null row.
//! * `bfill`       - backward-fill from the next non-null row.
//! * `const:VALUE` - fill with the literal text VALUE.
//!
//! The result table (with all specified columns imputed) is written to stdout
//! in whatever `--format` was requested. A summary line per column goes to
//! stderr.

use std::path::PathBuf;

use octa::data::impute::{ImputeStrategy, impute_column};

use super::OutputFormat;
use super::output::write_table;

/// Parse one `COL=STRATEGY` spec from the command line.
fn parse_spec(spec: &str) -> anyhow::Result<(String, ImputeStrategy)> {
    let (col, raw_strategy) = spec.split_once('=').ok_or_else(|| {
        anyhow::anyhow!("--impute expects COL=STRATEGY (e.g. `price=mean`), got \"{spec}\"")
    })?;
    let col = col.trim().to_string();
    if col.is_empty() {
        anyhow::bail!("--impute: column name is empty in \"{spec}\"");
    }
    let strategy = parse_strategy(raw_strategy.trim())?;
    Ok((col, strategy))
}

fn parse_strategy(s: &str) -> anyhow::Result<ImputeStrategy> {
    if let Some(value) = s.strip_prefix("const:") {
        return Ok(ImputeStrategy::Constant(value.to_string()));
    }
    match s.to_ascii_lowercase().as_str() {
        "mean" => Ok(ImputeStrategy::Mean),
        "median" => Ok(ImputeStrategy::Median),
        "mode" => Ok(ImputeStrategy::Mode),
        "ffill" => Ok(ImputeStrategy::ForwardFill),
        "bfill" => Ok(ImputeStrategy::BackwardFill),
        other => anyhow::bail!(
            "unknown impute strategy \"{other}\"; expected mean, median, mode, ffill, bfill, \
             or const:VALUE"
        ),
    }
}

pub fn run(path: PathBuf, impute_specs: Vec<String>, format: OutputFormat) -> anyhow::Result<()> {
    let mut table = super::read_table(&path)?;

    for spec in &impute_specs {
        let (col_name, strategy) = parse_spec(spec)?;
        let col_idx = table
            .columns
            .iter()
            .position(|c| c.name == col_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "--impute: no column named \"{col_name}\" in {}",
                    path.display()
                )
            })?;

        let imputed = impute_column(&table, col_idx, &strategy)?;

        // Replace the column's cells in the mutable table.
        for (r, cell) in imputed.into_iter().enumerate() {
            if r < table.rows.len() && col_idx < table.rows[r].len() {
                table.rows[r][col_idx] = cell;
            }
        }
        eprintln!(
            "impute: column \"{col_name}\" filled with strategy {:?}",
            strategy
        );
    }

    write_table(&table, format)
}
