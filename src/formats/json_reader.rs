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
        // `.geojson` is intentionally NOT claimed here - `GeoJsonReader`
        // owns that extension because the map view depends on its
        // dedicated parser. `.json` files that happen to be GeoJSON still
        // open through this reader as a flat table.
        &["json"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let content = std::fs::read_to_string(path)?;
        let value: Value = serde_json::from_str(&protect_long_numbers(&content))?;
        json_to_table(value, path, "JSON")
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let json = table_to_json_marked(table);
        let content = restore_long_numbers(&serde_json::to_string_pretty(&json)?);
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
            .map(|l| serde_json::from_str(&protect_long_numbers(l)))
            .collect::<Result<_, _>>()?;
        json_to_table(Value::Array(values), path, "JSONL")
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let json = table_to_json_marked(table);
        let lines: Vec<String> = json
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|v| restore_long_numbers(&serde_json::to_string(v).unwrap_or_default()))
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
            // Only treat a top-level array as "the records list" when it
            // contains objects. An array of pure primitives (e.g. `tags`)
            // should stay anchored under its key and be flattened as one
            // column, not turned into rows of `"value"`.
            let mut found = None;
            for (_, v) in map {
                if let Value::Array(arr) = v
                    && !arr.is_empty()
                    && arr.iter().any(|e| matches!(e, Value::Object(_)))
                {
                    found = Some(arr.clone());
                    break;
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
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some(format_name.to_string()),
            ..DataTable::empty()
        });
    }

    // Collect all unique keys by flattening objects - preserve insertion order
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
                // A number a float would round, kept as its digits.
                CellValue::String(s) if long_number_text(s) => {
                    types.insert("exact");
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
        } else if types.contains("exact") {
            // Exact digits beside other numbers: a decimal as wide as they
            // need, or text where none holds them.
            let mut shape = crate::data::decimal::DecimalShape::default();
            rows.iter().for_each(|r| {
                shape.add(&r[col_idx]);
            });
            shape.type_name().unwrap_or_else(|| "Utf8".to_string())
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
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some(format_name.to_string()),
        ..DataTable::empty()
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
                flatten_value(&full_key, val, out);
            }
        }
        Value::Array(arr) if array_has_complex_element(arr) => {
            // An object inside the array means the user expects every leaf
            // path inside that object to become its own column, indexed by
            // position. Pure-primitive arrays fall through to the `_` arm and
            // ride along as a single JSON-string cell.
            for (i, item) in arr.iter().enumerate() {
                let child_key = if prefix.is_empty() {
                    format!("[{}]", i)
                } else {
                    format!("{}[{}]", prefix, i)
                };
                flatten_value(&child_key, item, out);
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

/// Whether an array contains at least one nested object or sub-array - the
/// signal we use to decide a JSON array is structured data worth expanding
/// into per-index columns rather than serialized into a single string.
fn array_has_complex_element(arr: &[Value]) -> bool {
    arr.iter()
        .any(|v| matches!(v, Value::Object(_) | Value::Array(_)))
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
        CellValue::Time(_) | CellValue::Duration(_) => Value::String(cell.to_string()),
        CellValue::Binary(b) => Value::String(format!("<{} bytes>", b.len())),
        CellValue::Nested(s) => {
            serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone()))
        }
    }
}

pub fn json_value_to_cell(value: &Value) -> CellValue {
    match value {
        // A number `protect_long_numbers` kept from the float: its digits.
        Value::String(s) if s.starts_with(LONG_NUMBER) => {
            CellValue::String(s[LONG_NUMBER.len()..].to_string())
        }
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
        Value::Array(_) | Value::Object(_) => CellValue::Nested(restore_long_numbers(
            &serde_json::to_string(value).unwrap_or_default(),
        )),
    }
}

/// Marks a number `protect_long_numbers` took out of the float's way. The NUL
/// cannot stand unescaped in JSON text, so no file holds the mark by chance.
const LONG_NUMBER: &str = "\u{0}octa-number:";

/// The same mark as JSON writes it inside a string.
const LONG_NUMBER_JSON: &str = "\"\\u0000octa-number:";

/// The digits of a number `protect_long_numbers` marked, for a view that
/// shows the JSON value itself (the tree).
pub fn long_number(s: &str) -> Option<&str> {
    s.strip_prefix(LONG_NUMBER)
}

/// A typed number as the JSON value that keeps it: marked when a float would
/// round it (see [`protect_long_numbers`]), `None` for anything else.
pub fn mark_long_number(text: &str) -> Option<Value> {
    (text.parse::<i64>().is_err() && long_number_text(text))
        .then(|| Value::String(format!("{LONG_NUMBER}{text}")))
}

/// Whether a cell's text is the digits of a number a float would round, as
/// `json_value_to_cell` leaves them.
fn long_number_text(s: &str) -> bool {
    crate::data::decimal::number_cell(s).is_some_and(|c| matches!(c, CellValue::String(_)))
}

/// `content` with every number a float would round (past about 15 digits,
/// an integer past i64) turned into a marked string, so `serde_json`, which
/// reads every number as a float or an integer, cannot round it. Numbers with
/// an exponent stay numbers. Everything else is untouched, byte for byte.
pub fn protect_long_numbers(content: &str) -> std::borrow::Cow<'_, str> {
    let bytes = content.as_bytes();
    let mut out: Option<String> = None;
    let mut copied = 0;
    let (mut i, mut in_string) = (0, false);
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            match b {
                b'\\' => i += 1,
                b'"' => in_string = false,
                _ => {}
            }
            i += 1;
            continue;
        }
        if b == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if b == b'-' || b.is_ascii_digit() {
            let start = i;
            while i < bytes.len()
                && matches!(bytes[i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
            {
                i += 1;
            }
            let token = &content[start..i];
            if token.parse::<i64>().is_err() && long_number_text(token) {
                let out = out.get_or_insert_with(|| String::with_capacity(content.len() + 64));
                out.push_str(&content[copied..start]);
                out.push_str(LONG_NUMBER_JSON);
                out.push_str(token);
                out.push('"');
                copied = i;
            }
            continue;
        }
        i += 1;
    }
    match out {
        Some(mut out) => {
            out.push_str(&content[copied..]);
            std::borrow::Cow::Owned(out)
        }
        None => std::borrow::Cow::Borrowed(content),
    }
}

/// Undo the marks in JSON text: each marked string goes back to the bare
/// number it was, so a nested value or a written file holds the digits.
pub fn restore_long_numbers(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let mut rest = json;
    while let Some(at) = rest.find(LONG_NUMBER_JSON) {
        out.push_str(&rest[..at]);
        let after = &rest[at + LONG_NUMBER_JSON.len()..];
        let end = after.find('"').unwrap_or(after.len());
        out.push_str(&after[..end]);
        rest = after.get(end + 1..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

/// [`table_to_json_array`] with a decimal column's exact digits marked, for a
/// writer that runs the text through [`restore_long_numbers`] and so writes
/// them as numbers, every digit.
pub fn table_to_json_marked(table: &DataTable) -> Value {
    let mut json = table_to_json_array(table);
    let decimal: Vec<bool> = table
        .columns
        .iter()
        .map(|c| crate::data::decimal::decimal_parts(&c.data_type).is_some())
        .collect();
    for record in json.as_array_mut().into_iter().flatten() {
        let Some(obj) = record.as_object_mut() else {
            continue;
        };
        for (col, _) in table.columns.iter().zip(&decimal).filter(|(_, d)| **d) {
            if let Some(Value::String(s)) = obj.get_mut(&col.name)
                && crate::data::decimal::Exact::parse(s).is_some()
            {
                *s = format!("{LONG_NUMBER}{s}");
            }
        }
    }
    json
}
