//! `octa --compare-distributions FILE --dist-column COL [--dist-file-b FILE_B]
//! [--dist-column-b COL]` - do two columns come from the same population?
//!
//! Thin wrapper over `octa::data::distribution_compare`, printing the shared
//! result table so the terminal, the GUI tab and the MCP response all say the
//! same thing.

use std::path::PathBuf;

use octa::data::distribution_compare::{compare_columns, result_table};

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub struct Args {
    pub path: PathBuf,
    pub column: String,
    pub path_b: Option<PathBuf>,
    pub column_b: Option<String>,
    pub table: Option<String>,
    pub table_b: Option<String>,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<()> {
    let first = super::read_one(&args.path, args.table.as_deref())?;
    // No second file means both columns live in the first one, which is the
    // "did these two fields drift apart" case.
    let second = match &args.path_b {
        Some(p) => super::read_one(p, args.table_b.as_deref())?,
        None => first.clone(),
    };
    let name_b = args.column_b.clone().unwrap_or_else(|| args.column.clone());
    let ia = first.column_index(&args.column)?;
    let ib = second.column_index(&name_b)?;

    let outcome = compare_columns(&first, ia, &second, ib);
    let table = result_table(&args.column, &name_b, &outcome);
    write_table(&table, format)?;
    Ok(())
}
