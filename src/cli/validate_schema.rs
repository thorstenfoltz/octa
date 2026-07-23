//! `octa --validate-schema FILE --schema SCHEMA_FILE.json` - check a
//! file's column schema against an expected JSON Schema. Exit code is
//! 0 when the schemas match, 1 when they differ. CI-pipeable.
//!
//! Diff details land on stdout in the user's chosen `-f / --format`,
//! mirroring `--compare-schemas`. Parser warnings (JSON Schema types
//! the parser couldn't recognise) go to stderr so they don't pollute
//! a piped result.

use std::path::PathBuf;
use std::process::ExitCode;

use octa::data::validate_schema::{ValidationReport, validate_against_json_schema};
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::OutputFormat;
use super::output::write_table;

/// Run validation. Returns `Ok(ExitCode::SUCCESS)` on a clean match,
/// `Ok(ExitCode::FAILURE)` on any diff (so the caller in
/// [`super::dispatch`] doesn't have to translate). Read errors return
/// `Err(...)` and dispatch maps them to FAILURE.
pub fn run(
    path: PathBuf,
    schema_file: PathBuf,
    table: Option<String>,
    format: OutputFormat,
) -> anyhow::Result<ExitCode> {
    let dt = read_one(&path, table.as_deref())?;
    let schema_text = std::fs::read_to_string(&schema_file)
        .map_err(|e| anyhow::anyhow!("read --schema {}: {e}", schema_file.display()))?;
    let report = validate_against_json_schema(&dt.columns, &schema_text)?;

    let result_table = build_report_table(&report);
    write_table(&result_table, format)?;

    if !report.unparsed_types.is_empty() {
        eprintln!(
            "note: {} JSON Schema type(s) were not recognised and defaulted to Utf8:",
            report.unparsed_types.len()
        );
        for t in &report.unparsed_types {
            eprintln!("  - {t}");
        }
    }

    Ok(if report.matches {
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

/// Flatten the validation report into the same `status / column /
/// type_a / type_b` shape `compare_schemas` uses, with a header row
/// stating the overall match result via the column-zero `status`
/// column. Empty when the schemas match.
fn build_report_table(report: &ValidationReport) -> DataTable {
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
            name: "actual_type".to_string(),
            data_type: "Utf8".to_string(),
        },
        ColumnInfo {
            name: "expected_type".to_string(),
            data_type: "Utf8".to_string(),
        },
    ];

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let push =
        |rows: &mut Vec<Vec<CellValue>>, status: &str, name: &str, actual: &str, expected: &str| {
            rows.push(vec![
                CellValue::String(status.to_string()),
                CellValue::String(name.to_string()),
                CellValue::String(actual.to_string()),
                CellValue::String(expected.to_string()),
            ]);
        };

    for col in &report.diff.only_in_a {
        push(&mut rows, "unexpected", &col.name, &col.data_type, "");
    }
    for col in &report.diff.only_in_b {
        push(&mut rows, "missing", &col.name, "", &col.data_type);
    }
    for m in &report.diff.type_mismatches {
        push(&mut rows, "type_mismatch", &m.name, &m.type_a, &m.type_b);
    }
    // No row for `common` - they're not findings. Mirrors how lint
    // tools surface only the issues, not every healthy column.

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
