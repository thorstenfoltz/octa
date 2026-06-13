use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use serde_json::Value;
use std::path::Path;

/// Reader for Jupyter Notebook files (.ipynb).
/// Each cell becomes a row with columns: cell_number, cell_type, source, and outputs.
pub struct JupyterReader;

impl FormatReader for JupyterReader {
    fn name(&self) -> &str {
        "Jupyter Notebook"
    }

    fn extensions(&self) -> &[&str] {
        &["ipynb"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let content = std::fs::read_to_string(path)?;
        let notebook: Value = serde_json::from_str(&content)?;
        parse_notebook(&notebook, path)
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        write_notebook(path, table)
    }
}

fn parse_notebook(notebook: &Value, path: &Path) -> Result<DataTable> {
    let cells = notebook
        .get("cells")
        .and_then(|c| c.as_array())
        .ok_or_else(|| anyhow::anyhow!("Invalid notebook: missing 'cells' array"))?;

    let columns = vec![
        ColumnInfo {
            name: "Cell".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "Type".to_string(),
            data_type: "Utf8".to_string(),
        },
        ColumnInfo {
            name: "Source".to_string(),
            data_type: "Utf8".to_string(),
        },
        ColumnInfo {
            name: "Output".to_string(),
            data_type: "Utf8".to_string(),
        },
    ];

    let mut rows = Vec::new();

    for (idx, cell) in cells.iter().enumerate() {
        let cell_type = cell
            .get("cell_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let source = extract_multiline(cell.get("source"));
        let output = extract_outputs(cell.get("outputs"));

        rows.push(vec![
            CellValue::Int((idx + 1) as i64),
            CellValue::String(cell_type.to_string()),
            CellValue::String(source),
            CellValue::String(output),
        ]);
    }

    let mut table = DataTable::empty();
    table.columns = columns;
    table.rows = rows;
    table.source_path = Some(path.to_string_lossy().to_string());
    table.format_name = Some("Jupyter Notebook".to_string());
    Ok(table)
}

/// Extract text from a notebook multiline field (string or array of strings).
fn extract_multiline(value: Option<&Value>) -> String {
    match value {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(""),
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Extract text output from a cell's outputs array.
fn extract_outputs(value: Option<&Value>) -> String {
    let outputs = match value {
        Some(Value::Array(arr)) => arr,
        _ => return String::new(),
    };

    let mut parts = Vec::new();
    for output in outputs {
        let output_type = output
            .get("output_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match output_type {
            "stream" => {
                let text = extract_multiline(output.get("text"));
                if !text.is_empty() {
                    parts.push(text);
                }
            }
            "execute_result" | "display_data" => {
                // Prefer text/plain from the data dict
                if let Some(data) = output.get("data")
                    && let Some(text) = data.get("text/plain")
                {
                    let t = extract_multiline(Some(text));
                    if !t.is_empty() {
                        parts.push(t);
                    }
                }
            }
            "error" => {
                let ename = output
                    .get("ename")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Error");
                let evalue = output.get("evalue").and_then(|v| v.as_str()).unwrap_or("");
                parts.push(format!("{}: {}", ename, evalue));
            }
            _ => {}
        }
    }

    parts.join("\n")
}

/// Encode a cell's source text as the notebook multiline form: one
/// `"line\n"` string per line, with no trailing newline on the last line
/// unless the source itself ended with one.
fn source_to_array(source: &str) -> Value {
    let mut lines: Vec<Value> = source
        .lines()
        .map(|l| Value::String(format!("{}\n", l)))
        .collect();
    if lines.is_empty() {
        return Value::Array(vec![]);
    }
    if !source.ends_with('\n')
        && let Some(Value::String(s)) = lines.last_mut()
    {
        s.pop(); // drop the trailing \n we added to the final line
    }
    Value::Array(lines)
}

/// Build a fresh cell object from scratch (used for tables not backed by an
/// original notebook, and for rows appended past the original cell count).
/// Code cells get an empty `outputs` + null `execution_count`.
fn fresh_cell(cell_type: &str, source: &str) -> Value {
    let mut cell_obj = serde_json::Map::new();
    cell_obj.insert(
        "cell_type".to_string(),
        Value::String(cell_type.to_string()),
    );
    cell_obj.insert(
        "metadata".to_string(),
        Value::Object(serde_json::Map::new()),
    );
    cell_obj.insert("source".to_string(), source_to_array(source));
    if cell_type == "code" {
        cell_obj.insert("execution_count".to_string(), Value::Null);
        cell_obj.insert("outputs".to_string(), Value::Array(vec![]));
    }
    Value::Object(cell_obj)
}

/// Read the `cell_type` (col 1) and `source` (col 2) for one table row.
fn row_type_and_source(table: &DataTable, row: usize) -> (String, String) {
    let cell_type = match table.get(row, 1) {
        Some(CellValue::String(s)) => s.clone(),
        _ => "code".to_string(),
    };
    let source = match table.get(row, 2) {
        Some(CellValue::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    (cell_type, source)
}

/// Write a DataTable back to a Jupyter Notebook (.ipynb) file.
///
/// When the table came from an `.ipynb` (its `source_path` still parses as a
/// notebook), the original notebook is reused as the template: only each
/// cell's `source` (and `cell_type`) is overwritten from the table, so cell
/// **outputs**, `execution_count`, per-cell `metadata`, and the top-level
/// `nbformat` / `metadata` survive the round trip. An edited cell keeps its
/// prior (now-stale) output, matching Jupyter's behaviour when a cell is
/// changed but not re-run. Tables not backed by a notebook (built from
/// scratch, parse-in-new-tab) fall through to a from-scratch emit.
fn write_notebook(path: &Path, table: &DataTable) -> Result<()> {
    let original = table
        .source_path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(|v| v.get("cells").map(Value::is_array).unwrap_or(false));

    let notebook = match original {
        Some(orig) => merge_into_original(orig, table),
        None => fresh_notebook(table),
    };

    let content = serde_json::to_string_pretty(&notebook)?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Overwrite each original cell's source/type from the table, preserving
/// everything else; append fresh cells for extra rows and truncate to the
/// table's row count for deletions.
fn merge_into_original(mut notebook: Value, table: &DataTable) -> Value {
    let cells = notebook
        .get("cells")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut out_cells: Vec<Value> = Vec::with_capacity(table.row_count());

    for row in 0..table.row_count() {
        let (cell_type, source) = row_type_and_source(table, row);
        match cells.get(row) {
            Some(Value::Object(orig)) => {
                let mut cell = orig.clone();
                cell.insert("cell_type".to_string(), Value::String(cell_type.clone()));
                cell.insert("source".to_string(), source_to_array(&source));
                // Reconcile output-related keys with the (possibly changed)
                // cell type: markdown cells must not carry code-only keys, and
                // code cells need them present.
                if cell_type == "code" {
                    cell.entry("outputs")
                        .or_insert_with(|| Value::Array(vec![]));
                    cell.entry("execution_count").or_insert(Value::Null);
                } else {
                    cell.remove("outputs");
                    cell.remove("execution_count");
                }
                out_cells.push(Value::Object(cell));
            }
            // Original entry missing or non-object: emit a fresh cell.
            _ => out_cells.push(fresh_cell(&cell_type, &source)),
        }
    }

    if let Some(obj) = notebook.as_object_mut() {
        obj.insert("cells".to_string(), Value::Array(out_cells));
    }
    notebook
}

/// Emit a complete notebook from scratch (no original to preserve).
fn fresh_notebook(table: &DataTable) -> Value {
    let cells: Vec<Value> = (0..table.row_count())
        .map(|row| {
            let (cell_type, source) = row_type_and_source(table, row);
            fresh_cell(&cell_type, &source)
        })
        .collect();

    serde_json::json!({
        "nbformat": 4,
        "nbformat_minor": 5,
        "metadata": {
            "kernelspec": {
                "display_name": "Python 3",
                "language": "python",
                "name": "python3"
            },
            "language_info": {
                "name": "python",
                "version": "3.10.0"
            }
        },
        "cells": cells
    })
}

#[cfg(test)]
#[path = "jupyter_reader_tests.rs"]
mod tests;
