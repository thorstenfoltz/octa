//! `octa --union FILE [--union-file FILE ...] [--union-drop COL ...] [--union-cast COL=TYPE ...]`
//!
//! Stacks two or more tabular files into a single output table, reconciling
//! differing schemas. Columns present in only some sources are filled with
//! null; conflicting numeric types are widened (int + float -> float, any
//! other disagreement -> text). Use `--union-drop` to omit columns entirely
//! and `--union-cast COL=TYPE` to override the target Arrow type for a column.
//!
//! A one-line summary of the merged schema goes to stderr; the result table
//! goes to stdout in whatever `--format` was requested.

use std::path::PathBuf;

use octa::data::union::{plan_union, union_tables};

use super::OutputFormat;
use super::output::write_table;

pub fn run(
    files: Vec<PathBuf>,
    union_file: Vec<PathBuf>,
    drop: Vec<String>,
    cast: Vec<String>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let all_paths: Vec<PathBuf> = files.into_iter().chain(union_file).collect();
    if all_paths.len() < 2 {
        anyhow::bail!(
            "--union requires at least two input files (the positional file(s) plus one or more --union-file paths)"
        );
    }

    let tables: Vec<octa::data::DataTable> = all_paths
        .iter()
        .map(|p| super::read_table(p))
        .collect::<anyhow::Result<_>>()?;

    let schemas: Vec<&[octa::data::ColumnInfo]> =
        tables.iter().map(|t| t.columns.as_slice()).collect();
    let mut plan = plan_union(&schemas);

    // Apply --union-drop
    for col_name in &drop {
        if let Some(c) = plan.columns.iter_mut().find(|c| &c.name == col_name) {
            c.include = false;
        } else {
            eprintln!("warning: --union-drop column \"{col_name}\" not found in any source");
        }
    }

    // Apply --union-cast COL=TYPE
    for entry in &cast {
        let (col_name, target_type) = entry
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--union-cast expects COL=TYPE, got \"{entry}\""))?;
        if let Some(c) = plan.columns.iter_mut().find(|c| c.name == col_name) {
            c.target_type = target_type.to_string();
        } else {
            eprintln!("warning: --union-cast column \"{col_name}\" not found in any source");
        }
    }

    let refs: Vec<&octa::data::DataTable> = tables.iter().collect();
    let out = union_tables(&refs, &plan)?;

    let kept: Vec<_> = plan.columns.iter().filter(|c| c.include).collect();
    eprintln!(
        "union: {} source(s), {} output column(s), {} output row(s)",
        refs.len(),
        kept.len(),
        out.row_count()
    );

    write_table(&out, format)
}
