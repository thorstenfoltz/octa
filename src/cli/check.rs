//! `--check FILE --rules FILE`: does this data still satisfy its rules?
//!
//! A CI gate over the same `octa::data::validation` engine the GUI paints red
//! cells with, so a rule that passes here is a rule the user can reproduce by
//! opening the file. Exits 1 on any violation **and** on any rule that could
//! not run, because a rules file whose columns have been renamed would
//! otherwise report a clean run over checks it silently skipped.

use std::path::PathBuf;
use std::process::ExitCode;

use octa::data::validation::rules_file::{load, resolve};
use octa::data::validation::{ValidationRule, violations};
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::OutputFormat;
use super::output::write_table;

/// Offending values named per rule, so the report says what is wrong and not
/// just how much.
const MAX_SAMPLES: usize = 3;

/// The on-disk kind name, for the report's `rule` column.
fn rule_label(rule: &ValidationRule) -> String {
    let named = octa::data::validation::rules_file::to_named(std::slice::from_ref(rule), &[]);
    named
        .rule
        .first()
        .map(|r| r.kind.clone())
        .unwrap_or_default()
}

pub fn run(path: PathBuf, rules: PathBuf, format: OutputFormat) -> anyhow::Result<ExitCode> {
    let table = super::read_table(&path)?;
    let file = load(&rules)?;
    let (resolved, unknown) = resolve(&file, &table.columns);

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let mut failed = !unknown.is_empty();

    // One pass per rule: `violations` answers for a whole rule set at once, so
    // attributing a failing cell back to its rule means asking one rule at a
    // time. N scans of a table is fine for a gate and leaves the engine alone.
    for rule in &resolved {
        let hits = violations(&table, std::slice::from_ref(rule));
        if hits.is_empty() {
            continue;
        }
        failed = true;
        let mut coords: Vec<(usize, usize)> = hits.into_iter().collect();
        coords.sort_unstable();
        let samples: Vec<String> = coords
            .iter()
            .take(MAX_SAMPLES)
            .filter_map(|(r, c)| table.get(*r, *c).map(|v| v.to_string()))
            .collect();
        let column = rule
            .column
            .and_then(|i| table.columns.get(i))
            .map(|c| c.name.clone())
            .unwrap_or_default();
        rows.push(vec![
            CellValue::String(rule_label(rule)),
            CellValue::String(column),
            CellValue::Int(coords.len() as i64),
            CellValue::String(samples.join(", ")),
        ]);
    }

    // A rule that could not be resolved is reported in the same table rather
    // than only on stderr: a machine reading the report must see it too.
    for problem in &unknown {
        rows.push(vec![
            CellValue::String(problem.clone()),
            CellValue::String(String::new()),
            CellValue::String(String::new()),
            CellValue::String(String::new()),
        ]);
    }

    let mut report = DataTable::empty();
    report.columns = ["rule", "column", "failures", "samples"]
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    report.rows = rows;
    write_table(&report, format)?;

    if failed {
        eprintln!(
            "{} of {} rule(s) failed, {} could not run",
            report.row_count() - unknown.len(),
            resolved.len(),
            unknown.len()
        );
        return Ok(ExitCode::from(1));
    }
    eprintln!("all {} rule(s) passed", resolved.len());
    Ok(ExitCode::SUCCESS)
}
