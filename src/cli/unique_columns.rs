//! `octa --unique-columns FILE [--max-combo N]` - print per-column
//! and per-combo uniqueness for FILE. Useful for spotting primary-key
//! candidates in undocumented data.

use std::path::PathBuf;

use octa::data::unique_columns::{UniqueAnalysis, find_unique_columns};
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::OutputFormat;
use super::output::write_table;

pub fn run(
    path: PathBuf,
    table: Option<String>,
    max_combo: usize,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let dt = read_one(&path, table.as_deref())?;
    let analysis = find_unique_columns(&dt, max_combo);
    let out_table = build_analysis_table(&analysis);
    write_table(&out_table, format)?;
    Ok(())
}

fn read_one(path: &std::path::Path, table: Option<&str>) -> anyhow::Result<DataTable> {
    octa::formats::read_table_auto(
        path,
        table,
        octa::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES,
    )
}

/// Flatten single-column + combo results into a five-column table:
/// `scope / columns / distinct_count / null_count / is_unique`.
/// `scope` is `single` or `combo`; `columns` is a `+`-joined name
/// list for combos and the column name for singles. `null_count` is
/// empty (string) for combos because nulls participate in the key
/// the same as any other value.
fn build_analysis_table(a: &UniqueAnalysis) -> DataTable {
    let columns = vec![
        ColumnInfo {
            name: "scope".to_string(),
            data_type: "Utf8".to_string(),
        },
        ColumnInfo {
            name: "columns".to_string(),
            data_type: "Utf8".to_string(),
        },
        ColumnInfo {
            name: "distinct_count".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "null_count".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "is_unique".to_string(),
            data_type: "Boolean".to_string(),
        },
    ];

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    for r in &a.single {
        rows.push(vec![
            CellValue::String("single".to_string()),
            CellValue::String(r.column.clone()),
            CellValue::Int(r.distinct_count as i64),
            CellValue::Int(r.null_count as i64),
            CellValue::Bool(r.is_unique),
        ]);
    }
    for c in &a.combos {
        rows.push(vec![
            CellValue::String("combo".to_string()),
            CellValue::String(c.columns.join(" + ")),
            CellValue::Int(c.distinct_count as i64),
            CellValue::Null,
            CellValue::Bool(c.is_unique),
        ]);
    }

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
