//! `octa --join [FILE ...] [--join-file FILE ...] --join-on COL[,COL,...] [--join-type TYPE]`
//!
//! Joins two or more tabular files left-to-right on shared key column(s).
//! The positional file(s) and any `--join-file` paths together form the
//! ordered source list; they are assigned names `t0`, `t1`, ... and joined
//! via a DuckDB `USING (keys)` expression. Duplicate key columns are
//! collapsed into one in the output.
//!
//! A one-line summary goes to stderr; the result table goes to stdout in
//! whatever `--format` was requested.

use std::path::PathBuf;

use octa::data::join::{JoinType, join_tables};

use super::OutputFormat;
use super::output::write_table;

pub fn run(
    files: Vec<PathBuf>,
    join_file: Vec<PathBuf>,
    join_on: Vec<String>,
    join_type: Option<String>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let all_paths: Vec<PathBuf> = files.into_iter().chain(join_file).collect();
    if all_paths.len() < 2 {
        anyhow::bail!(
            "--join requires at least two input files (the positional file(s) plus one or more \
             --join-file paths)"
        );
    }
    if join_on.is_empty() {
        anyhow::bail!("--join requires --join-on COL[,COL,...] with at least one key column");
    }

    let tables: Vec<octa::data::DataTable> = all_paths
        .iter()
        .map(|p| super::read_table(p))
        .collect::<anyhow::Result<_>>()?;

    // Build owned names first so the borrows below outlive this scope.
    let names: Vec<String> = (0..tables.len()).map(|i| format!("t{i}")).collect();
    let named: Vec<(&str, &octa::data::DataTable)> = names
        .iter()
        .map(String::as_str)
        .zip(tables.iter())
        .collect();

    let how = parse_join_type(join_type.as_deref())?;
    let out = join_tables(&named, &join_on, how)?;

    eprintln!(
        "join: {} source(s), {} key(s), {} output column(s), {} output row(s)",
        named.len(),
        join_on.len(),
        out.col_count(),
        out.row_count()
    );

    write_table(&out, format)
}

fn parse_join_type(s: Option<&str>) -> anyhow::Result<JoinType> {
    match s.unwrap_or("left").to_ascii_lowercase().as_str() {
        "left" => Ok(JoinType::Left),
        "inner" => Ok(JoinType::Inner),
        "right" => Ok(JoinType::Right),
        "full" => Ok(JoinType::Full),
        other => {
            anyhow::bail!("--join-type must be one of: left, inner, right, full (got \"{other}\")")
        }
    }
}
