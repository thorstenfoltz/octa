//! `octa --compare-schemas FILE_A FILE_B` - diff the column schemas of
//! two files and print the result to stdout.
//!
//! Reads only the schemas (no row data) through the format registry,
//! then delegates to the pure `octa::data::compare_schemas` function.
//! Output goes through `cli::output::write_table` so the user's
//! `-f / --format {tsv|json|csv}` choice carries through.

use std::path::PathBuf;

use octa::data::compare_schemas::{SchemaDiff, compare_schemas};
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::OutputFormat;
use super::output::write_table;

pub fn run(
    path_a: PathBuf,
    path_b: PathBuf,
    table_a: Option<String>,
    table_b: Option<String>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let dt_a = read_one(&path_a, table_a.as_deref())?;
    let dt_b = read_one(&path_b, table_b.as_deref())?;
    let diff = compare_schemas(&dt_a.columns, &dt_b.columns);
    let table = build_diff_table(&diff);
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

/// Flatten the diff into a single four-column table that the shared
/// CLI writer can render. Rows are emitted in this order: `common`,
/// `only_in_a`, `only_in_b`, `type_mismatches`.
fn build_diff_table(diff: &SchemaDiff) -> DataTable {
    let columns = vec![
        ColumnInfo {
            name: "status".to_string(),
            data_type: "Utf8".to_string(),
        },
        ColumnInfo {
            name: "column".to_string(),
            data_type: "Utf8".to_string(),
        },
        ColumnInfo {
            name: "type_a".to_string(),
            data_type: "Utf8".to_string(),
        },
        ColumnInfo {
            name: "type_b".to_string(),
            data_type: "Utf8".to_string(),
        },
    ];

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let push = |rows: &mut Vec<Vec<CellValue>>, status: &str, name: &str, a: &str, b: &str| {
        rows.push(vec![
            CellValue::String(status.to_string()),
            CellValue::String(name.to_string()),
            CellValue::String(a.to_string()),
            CellValue::String(b.to_string()),
        ]);
    };

    for col in &diff.common {
        push(
            &mut rows,
            "common",
            &col.name,
            &col.data_type,
            &col.data_type,
        );
    }
    for col in &diff.only_in_a {
        push(&mut rows, "only_in_a", &col.name, &col.data_type, "");
    }
    for col in &diff.only_in_b {
        push(&mut rows, "only_in_b", &col.name, "", &col.data_type);
    }
    for m in &diff.type_mismatches {
        push(&mut rows, "type_mismatch", &m.name, &m.type_a, &m.type_b);
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
