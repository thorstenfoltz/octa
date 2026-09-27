//! `octa --shapes FILE --shapes-column COL` - what the column's values look
//! like: one row per shape (`A-99999`), with its count and one example, most
//! common first. Empty cells are counted on stderr.

use std::path::PathBuf;

use octa::data::shapes::shape_frequency;
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub struct Args {
    pub path: PathBuf,
    pub column: String,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<()> {
    let table = super::read_table(&args.path)?;
    let col = table
        .columns
        .iter()
        .position(|c| c.name == args.column)
        .ok_or_else(|| anyhow::anyhow!("no column named `{}`", args.column))?;
    let shapes = shape_frequency(&table, col);
    let mut out = DataTable::empty();
    out.columns = [("shape", "Utf8"), ("count", "Int64"), ("example", "Utf8")]
        .into_iter()
        .map(|(n, t)| ColumnInfo {
            name: n.into(),
            data_type: t.into(),
        })
        .collect();
    out.rows = shapes
        .shapes
        .iter()
        .map(|s| {
            vec![
                CellValue::String(s.shape.clone()),
                CellValue::Int(s.count as i64),
                CellValue::String(s.example.clone()),
            ]
        })
        .collect();
    write_table(&out, format)?;
    eprintln!(
        "{} shape(s), {} empty cell(s)",
        shapes.shapes.len(),
        shapes.empty
    );
    if let Some((loaded, total)) = table.partial_note() {
        eprintln!(
            "note: counted over the first {loaded} rows of {}",
            total.map_or("more".into(), |t| t.to_string())
        );
    }
    Ok(())
}
