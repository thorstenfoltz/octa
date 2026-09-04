//! `octa --compare-distributions FILE --dist-column COL [--dist-file-b FILE_B]
//! [--dist-column-b COL]` - do two columns come from the same population?
//!
//! Thin wrapper over `octa::data::distribution_compare`, printing the shared
//! result table so the terminal, the GUI tab and the MCP response all say the
//! same thing.

use std::path::PathBuf;

use octa::data::DataTable;
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
    let first = read_one(&args.path, args.table.as_deref())?;
    // No second file means both columns live in the first one, which is the
    // "did these two fields drift apart" case.
    let second = match &args.path_b {
        Some(p) => read_one(p, args.table_b.as_deref())?,
        None => first.clone(),
    };
    let name_b = args.column_b.clone().unwrap_or_else(|| args.column.clone());
    let ia = column_index(&first, &args.column)?;
    let ib = column_index(&second, &name_b)?;

    let outcome = compare_columns(&first, ia, &second, ib);
    let table = result_table(&args.column, &name_b, &outcome);
    write_table(&table, format)?;
    Ok(())
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
