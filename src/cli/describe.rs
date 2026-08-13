//! `octa --describe FILE [--table NAME] [--sample-rows N]` - one-shot
//! orientation snapshot of a tabular file. Delegates to
//! `octa::data::describe::describe_file`.
//!
//! Output goes through the shared CLI formatter (`-f`); the layout is
//! a vertical key/value table: each row is `field = value` for the
//! high-level fields, then the column schema, then the sample rows.
//! TSV / CSV / JSON all carry the same field set so downstream tools
//! can grep / jq the result.

use std::path::PathBuf;

use octa::data::describe::{FileDescription, describe_file};
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::OutputFormat;
use super::output::write_table;

pub fn run(
    path: PathBuf,
    table: Option<String>,
    sample_rows: Option<usize>,
    deep: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let d = describe_file(&path, table.as_deref(), sample_rows)?;
    let internals = if deep {
        Some(octa::data::file_internals::inspect(&path)?)
    } else {
        None
    };
    match format {
        OutputFormat::Json => print_json(&d, internals.as_ref())?,
        _ => {
            write_table(&build_overview_table(&d), format)?;
            if let Some(i) = &internals {
                // Facts as key/value lines, then the per-chunk table. Hints go
                // to stderr so a piped `--describe --deep` stays parseable.
                println!();
                for (k, v) in &i.facts {
                    println!("{k}\t{v}");
                }
                for hint in &i.hints {
                    eprintln!("hint: {hint}");
                }
                if i.chunks.row_count() > 0 {
                    println!();
                    write_table(&i.chunks, format)?;
                }
            }
        }
    }
    Ok(())
}

/// JSON output: hand-rolled to mirror the MCP tool's response shape
/// so a `octa --describe -f json` matches what the MCP server returns
/// (minus the `cell_truncated` flag, which is MCP-specific).
fn print_json(
    d: &FileDescription,
    internals: Option<&octa::data::file_internals::FileInternals>,
) -> anyhow::Result<()> {
    use serde_json::{Map, Value};
    let mut out = Map::new();
    out.insert("path".to_string(), Value::String(d.path.clone()));
    out.insert(
        "format_name".to_string(),
        d.format_name
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null),
    );
    out.insert(
        "file_size_bytes".to_string(),
        d.file_size_bytes.map(Value::from).unwrap_or(Value::Null),
    );
    out.insert(
        "table".to_string(),
        d.table
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null),
    );
    out.insert("row_count".to_string(), Value::from(d.row_count));
    out.insert(
        "initial_load_capped".to_string(),
        Value::Bool(d.initial_load_capped),
    );
    let cols: Vec<Value> = d
        .columns
        .iter()
        .map(|c| {
            let mut m = Map::new();
            m.insert("name".to_string(), Value::String(c.name.clone()));
            m.insert("type".to_string(), Value::String(c.data_type.clone()));
            Value::Object(m)
        })
        .collect();
    out.insert("columns".to_string(), Value::Array(cols));
    let samples: Vec<Value> = d
        .sample_rows
        .iter()
        .map(|row| {
            let arr: Vec<Value> = row.iter().map(|c| Value::String(c.to_string())).collect();
            Value::Array(arr)
        })
        .collect();
    out.insert("sample_rows".to_string(), Value::Array(samples));
    if let Some(i) = internals {
        // Same `internals` shape the MCP tool returns, so `-f json` and the
        // agent-facing response stay interchangeable.
        let facts: Map<String, Value> = i
            .facts
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        let mut obj = Map::new();
        obj.insert("facts".to_string(), Value::Object(facts));
        obj.insert(
            "hints".to_string(),
            Value::Array(i.hints.iter().map(|h| Value::String(h.clone())).collect()),
        );
        out.insert("internals".to_string(), Value::Object(obj));
    }
    println!("{}", serde_json::to_string_pretty(&Value::Object(out))?);
    Ok(())
}

/// Build a two-column key/value table that TSV / CSV can render. The
/// columns are `field` / `value`. Sample rows are appended as a
/// comma-joined `sample_row[N]` row each so the user can eyeball them.
fn build_overview_table(d: &FileDescription) -> DataTable {
    let columns = vec![
        ColumnInfo {
            name: "field".to_string(),
            data_type: "Utf8".to_string(),
        },
        ColumnInfo {
            name: "value".to_string(),
            data_type: "Utf8".to_string(),
        },
    ];
    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let mut push = |k: &str, v: String| {
        rows.push(vec![CellValue::String(k.to_string()), CellValue::String(v)]);
    };
    push("path", d.path.clone());
    push("format_name", d.format_name.clone().unwrap_or_default());
    push(
        "file_size_bytes",
        d.file_size_bytes.map(|n| n.to_string()).unwrap_or_default(),
    );
    push("table", d.table.clone().unwrap_or_default());
    push("row_count", d.row_count.to_string());
    push("initial_load_capped", d.initial_load_capped.to_string());
    push("column_count", d.columns.len().to_string());
    for col in &d.columns {
        push(&format!("column[{}]", col.name), col.data_type.clone());
    }
    for (i, row) in d.sample_rows.iter().enumerate() {
        let joined: Vec<String> = row.iter().map(|c| c.to_string()).collect();
        push(&format!("sample_row[{i}]"), joined.join(", "));
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
