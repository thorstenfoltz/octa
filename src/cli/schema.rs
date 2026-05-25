//! `octa --schema <FILE>` — print column metadata as a small table.
//!
//! Always reads the *whole* schema (cheap — it's just the column list,
//! no row data is touched for binary formats like Parquet). Text formats
//! do parse the header row to populate column count.

use std::path::PathBuf;

use octa::data::{CellValue, ColumnInfo, DataTable};

use super::OutputFormat;
use super::output::write_table;

pub fn run(path: PathBuf, format: OutputFormat) -> anyhow::Result<()> {
    let table = super::read_table(&path)?;
    let schema_table = build_schema_table(&table);
    write_table(&schema_table, format)?;
    Ok(())
}

/// Project a `DataTable`'s `columns` into a two-column table: `name`,
/// `type`. Bypasses the registry — we already have the schema in memory.
fn build_schema_table(source: &DataTable) -> DataTable {
    let columns = vec![
        ColumnInfo {
            name: "name".to_string(),
            data_type: "Utf8".to_string(),
        },
        ColumnInfo {
            name: "type".to_string(),
            data_type: "Utf8".to_string(),
        },
    ];
    let rows: Vec<Vec<CellValue>> = source
        .columns
        .iter()
        .map(|c| {
            vec![
                CellValue::String(c.name.clone()),
                CellValue::String(c.data_type.clone()),
            ]
        })
        .collect();
    DataTable {
        columns,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    }
}
