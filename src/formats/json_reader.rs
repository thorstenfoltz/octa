use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

pub struct JsonReader;

impl FormatReader for JsonReader {
    fn name(&self) -> &str {
        "JSON"
    }

    fn extensions(&self) -> &[&str] {
        &["json", "geojson"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let content = std::fs::read_to_string(path)?;
        let value: Value = serde_json::from_str(&content)?;
        json_to_table(value, path, "JSON")
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let json = table_to_json_array(table);
        let content = serde_json::to_string_pretty(&json)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

pub struct JsonlReader;

impl FormatReader for JsonlReader {
    fn name(&self) -> &str {
        "JSON Lines"
    }

    fn extensions(&self) -> &[&str] {
        &["jsonl", "ndjson"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let content = std::fs::read_to_string(path)?;
        let values: Vec<Value> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?;
        json_to_table(Value::Array(values), path, "JSONL")
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let json = table_to_json_array(table);
        let lines: Vec<String> = json
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|v| serde_json::to_string(v).unwrap_or_default())
            .collect();
        std::fs::write(path, lines.join("\n"))?;
        Ok(())
    }
}

/// Convert a JSON value to a DataTable. Public so TOML/YAML readers can reuse it.
/// Preserves original key order from the source data.
pub fn json_to_table(value: Value, path: &Path, format_name: &str) -> Result<DataTable> {
    let records = match value {
        Value::Array(arr) => arr,
        Value::Object(ref map) => {
            // Try to find the first array field in the object
            let mut found = None;
            for (_, v) in map {
                if let Value::Array(arr) = v {
                    if !arr.is_empty() {
                        found = Some(arr.clone());
                        break;
                    }
                }
            }
            found.unwrap_or_else(|| vec![value.clone()])
        }
        _ => vec![value],
    };

    if records.is_empty() {
        return Ok(DataTable {
            columns: Vec::new(),
            rows: Vec::new(),
            edits: std::collections::HashMap::new(),
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some(format_name.to_string()),
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: std::collections::HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        });
    }

    // Collect all unique keys by flattening objects — preserve insertion order
    let mut all_keys: Vec<String> = Vec::new();
    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut flat_records: Vec<Vec<(String, Value)>> = Vec::new();

    for record in &records {
        let mut flat = Vec::new();
        flatten_value("", record, &mut flat);
        for (key, _) in &flat {
            if seen_keys.insert(key.clone()) {
                all_keys.push(key.clone());
            }
        }
        flat_records.push(flat);
    }

    let columns: Vec<ColumnInfo> = all_keys
        .iter()
        .map(|name| ColumnInfo {
            name: name.clone(),
            data_type: "Utf8".to_string(),
        })
        .collect();

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    for flat in &flat_records {
        let row: Vec<CellValue> = all_keys
            .iter()
            .map(|key| {
                flat.iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| json_value_to_cell(v))
                    .unwrap_or(CellValue::Null)
            })
            .collect();
        rows.push(row);
    }

    // Refine column types
    let mut final_columns = columns;
    for (col_idx, col) in final_columns.iter_mut().enumerate() {
        let mut types = HashSet::new();
        for row in &rows {
            match &row[col_idx] {
                CellValue::Null => {}
                CellValue::Int(_) => {
                    types.insert("int");
                }
                CellValue::Float(_) => {
                    types.insert("float");
                }
                CellValue::Bool(_) => {
                    types.insert("bool");
                }
                CellValue::Nested(_) => {
                    types.insert("nested");
                }
                _ => {
                    types.insert("string");
                }
            }
        }
        col.data_type = if types.contains("string") || types.contains("nested") {
            "Utf8".to_string()
        } else if types.contains("float") {
            "Float64".to_string()
        } else if types.contains("int") && !types.contains("float") {
            "Int64".to_string()
        } else if types.contains("int") {
            "Float64".to_string()
        } else if types.contains("bool") {
            "Boolean".to_string()
        } else {
            "Utf8".to_string()
        };
    }

    Ok(DataTable {
        columns: final_columns,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some(format_name.to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    })
}

pub fn flatten_value(prefix: &str, value: &Value, out: &mut Vec<(String, Value)>) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let full_key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                match val {
                    Value::Object(_) => flatten_value(&full_key, val, out),
                    _ => {
                        out.push((full_key, val.clone()));
                    }
                }
            }
        }
        _ => {
            let key = if prefix.is_empty() {
                "value".to_string()
            } else {
                prefix.to_string()
            };
            out.push((key, value.clone()));
        }
    }
}

/// Convert a DataTable back to a JSON array of objects.
pub fn table_to_json_array(table: &DataTable) -> Value {
    let mut records = Vec::new();
    for row_idx in 0..table.row_count() {
        let mut obj = serde_json::Map::new();
        for (col_idx, col) in table.columns.iter().enumerate() {
            let val = table
                .get(row_idx, col_idx)
                .cloned()
                .unwrap_or(CellValue::Null);
            obj.insert(col.name.clone(), cell_to_json_value(&val));
        }
        records.push(Value::Object(obj));
    }
    Value::Array(records)
}

fn cell_to_json_value(cell: &CellValue) -> Value {
    match cell {
        CellValue::Null => Value::Null,
        CellValue::Bool(b) => Value::Bool(*b),
        CellValue::Int(i) => serde_json::json!(*i),
        CellValue::Float(f) => serde_json::json!(*f),
        CellValue::String(s) => Value::String(s.clone()),
        CellValue::Date(s) => Value::String(s.clone()),
        CellValue::DateTime(s) => Value::String(s.clone()),
        CellValue::Binary(b) => Value::String(format!("<{} bytes>", b.len())),
        CellValue::Nested(s) => {
            serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone()))
        }
    }
}

pub fn json_value_to_cell(value: &Value) -> CellValue {
    match value {
        Value::Null => CellValue::Null,
        Value::Bool(b) => CellValue::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CellValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                CellValue::Float(f)
            } else {
                CellValue::String(n.to_string())
            }
        }
        Value::String(s) => CellValue::String(s.clone()),
        Value::Array(_) | Value::Object(_) => {
            CellValue::Nested(serde_json::to_string(value).unwrap_or_default())
        }
    }
}
