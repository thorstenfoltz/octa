//! `octa --dedupe FILE [--dedupe-on COL[,...]] [--dedupe-keep first|last]`
//!
//! Removes duplicate rows from a tabular file. Without `--dedupe-on` the
//! whole row is the key; with it, only the named columns form the key.
//! `--dedupe-keep` (default `first`) chooses which occurrence survives.
//!
//! The result is written to stdout in whatever `--format` was requested.
//! A one-line summary (original rows, duplicates removed, output rows) goes
//! to stderr.

use std::path::PathBuf;

use octa::data::dedupe::{KeepWhich, dedupe_rows};

use super::OutputFormat;
use super::output::write_table;

pub fn run(
    path: PathBuf,
    dedupe_on: Option<String>,
    dedupe_keep: Option<String>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let table = super::read_table(&path)?;

    // Resolve column names to indices.
    let key_cols: Vec<usize> = match dedupe_on.as_deref() {
        Some(spec) if !spec.trim().is_empty() => {
            let names: Vec<&str> = spec.split(',').map(str::trim).collect();
            let mut indices = Vec::with_capacity(names.len());
            for name in names {
                let idx = table
                    .columns
                    .iter()
                    .position(|c| c.name == name)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "--dedupe-on: no column named \"{name}\" in {}",
                            path.display()
                        )
                    })?;
                indices.push(idx);
            }
            indices
        }
        // Absent or empty -> whole-row key.
        _ => vec![],
    };

    let keep = match dedupe_keep
        .as_deref()
        .unwrap_or("first")
        .to_ascii_lowercase()
        .as_str()
    {
        "first" => KeepWhich::First,
        "last" => KeepWhich::Last,
        other => anyhow::bail!("--dedupe-keep must be `first` or `last` (got \"{other}\")"),
    };

    let original_rows = table.row_count();
    let out = dedupe_rows(&table, &key_cols, keep);
    let removed = original_rows.saturating_sub(out.row_count());

    eprintln!(
        "dedupe: {} input row(s), {} duplicate(s) removed, {} output row(s)",
        original_rows,
        removed,
        out.row_count()
    );

    write_table(&out, format)
}
