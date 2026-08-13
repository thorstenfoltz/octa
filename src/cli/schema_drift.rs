//! `--schema-drift DIR`: which files in a folder disagree about their columns.
//!
//! Exits 1 when they do, matching `--validate-schema`, so a CI step can gate
//! on it. The report table goes to stdout through the normal formatter; the
//! per-variant file lists and any skipped files go to stderr so a pipe stays
//! parseable.

use std::path::PathBuf;
use std::process::ExitCode;

use octa::data::schema_drift::{DriftOptions, analyse, collect_schemas, report_table};
use octa::formats::FormatRegistry;

use super::OutputFormat;
use super::output::write_table;

pub fn run(dir: PathBuf, opts: DriftOptions, format: OutputFormat) -> anyhow::Result<ExitCode> {
    if !dir.is_dir() {
        anyhow::bail!("{} is not a directory", dir.display());
    }
    let (files, skipped) = collect_schemas(&dir, opts.recursive, &FormatRegistry::new());
    if files.is_empty() {
        anyhow::bail!("no readable files in {}", dir.display());
    }

    let mut report = analyse(&files, &opts);
    report.skipped = skipped;

    write_table(&report_table(&report), format)?;

    for (i, v) in report.variants.iter().enumerate() {
        eprintln!("variant {}: {} file(s)", i + 1, v.files.len());
        for f in &v.files {
            eprintln!("  {f}");
        }
    }
    for (file, reason) in &report.skipped {
        eprintln!("skipped {file}: {reason}");
    }
    if report.has_drift {
        eprintln!("drifting columns: {}", report.drifting_columns.join(", "));
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}
