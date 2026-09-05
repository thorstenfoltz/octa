//! `octa --check-references PARENT --parent-column COL --child-file CHILD
//! --child-column COL` - list child rows whose key has no parent.
//!
//! **Exits 1 when orphans exist**, so it works as a CI gate the way
//! `--validate-schema` and `--schema-drift` do. The report goes to stdout and
//! the summary to stderr, so a pipe stays parseable.

use std::path::PathBuf;
use std::process::ExitCode;

use octa::data::DataTable;
use octa::data::referential::{check, report_table};

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub struct Args {
    pub parent: PathBuf,
    pub parent_column: String,
    pub child: Option<PathBuf>,
    pub child_column: String,
    pub table_a: Option<String>,
    pub table_b: Option<String>,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<ExitCode> {
    let parent = read_one(&args.parent, args.table_a.as_deref())?;
    // No child file means both columns live in the parent file, which is the
    // self-referencing case (a `manager_id` pointing at `id`).
    let child = match &args.child {
        Some(p) => read_one(p, args.table_b.as_deref())?,
        None => parent.clone(),
    };
    let pcol = column_index(&parent, &args.parent_column)?;
    let ccol = column_index(&child, &args.child_column)?;

    let report = check(&parent, pcol, &child, ccol);
    write_table(&report_table(&args.child_column, &report), format)?;
    eprintln!("{}", report.sentence());
    if report.null_keys > 0 {
        eprintln!(
            "note: {} child row(s) have no key at all, which is not an orphan",
            report.null_keys
        );
    }
    Ok(if report.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn read_one(path: &std::path::Path, table: Option<&str>) -> anyhow::Result<DataTable> {
    octa::formats::read_table_auto(
        path,
        table,
        octa::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES,
    )
}

fn column_index(t: &DataTable, name: &str) -> anyhow::Result<usize> {
    t.columns
        .iter()
        .position(|c| c.name == name)
        .ok_or_else(|| anyhow::anyhow!("no column named `{name}`"))
}
