//! `--drift-report FILE_A FILE_B`: has this dataset changed shape?
//!
//! Prints one row per measured difference through the normal formatter, and
//! puts the pass/fail summary plus the added and removed column names on
//! stderr so a piped table stays parseable, matching `--schema-drift`.
//!
//! Exits 1 only when `--fail-on` was given and a gate was breached. Without
//! thresholds the report is informational and always exits 0: a CI step opts
//! in to failing, it does not have to opt out.

use std::path::PathBuf;
use std::process::ExitCode;

use octa::data::drift::{DriftOptions, compare_profiles, parse_thresholds, report_table};

use super::OutputFormat;
use super::output::write_table;

pub fn run(
    path_a: PathBuf,
    path_b: PathBuf,
    fail_on: Option<String>,
    format: OutputFormat,
) -> anyhow::Result<ExitCode> {
    let opts = DriftOptions {
        thresholds: match &fail_on {
            Some(spec) => parse_thresholds(spec)?,
            None => Vec::new(),
        },
        ..Default::default()
    };

    let a = super::read_table(&path_a)?;
    let b = super::read_table(&path_b)?;
    let report = compare_profiles(&a, &b, &opts)?;

    write_table(&report_table(&report), format)?;

    eprintln!("rows: {} -> {}", report.rows_before, report.rows_after);
    if !report.added_columns.is_empty() {
        eprintln!("added columns: {}", report.added_columns.join(", "));
    }
    if !report.removed_columns.is_empty() {
        eprintln!("removed columns: {}", report.removed_columns.join(", "));
    }
    if report.failed {
        let breached: Vec<String> = report
            .rows
            .iter()
            .filter(|r| r.breached)
            .map(|r| {
                if r.column.is_empty() {
                    r.metric.clone()
                } else {
                    format!("{}.{}", r.column, r.metric)
                }
            })
            .collect();
        eprintln!("breached: {}", breached.join(", "));
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}
