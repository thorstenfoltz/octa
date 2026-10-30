use crate::data::times::{self, TimeKind};
use crate::data::{CellValue, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use std::path::Path;

pub struct TomlReader;

impl FormatReader for TomlReader {
    fn name(&self) -> &str {
        "TOML"
    }

    fn extensions(&self) -> &[&str] {
        &["toml"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let content = std::fs::read_to_string(path)?;
        // toml 1.x split parsing into "single value" (`FromStr`) vs whole
        // document (`from_str`). Files like `[[items]]` are documents, so
        // route through the document parser.
        let value: toml::Value = toml::from_str(&content)?;
        let json_value = toml_to_json(&value);
        let mut table = crate::formats::json_reader::json_to_table(json_value, path, "TOML")?;
        // A column whose every cell is the text of a TOML local time is a
        // Time column. ponytail: matched by text, so a quoted string equal to
        // a local time elsewhere in the file would count too; that is rare.
        let mut local = std::collections::HashSet::new();
        collect_local_times(&value, &mut local);
        // The writer turns a null cell into "" (TOML has no null), so an empty
        // string counts as a gap, like Null. At least one real local time is
        // still required, so a column of only gaps stays text.
        for col in 0..table.columns.len() {
            let mut has_time = false;
            let all_local = table
                .rows
                .iter()
                .filter_map(|r| r.get(col))
                .all(|c| match c {
                    CellValue::String(s) if local.contains(s) => {
                        has_time = true;
                        true
                    }
                    CellValue::String(s) => s.is_empty(),
                    CellValue::Null => true,
                    _ => false,
                })
                && has_time;
            if all_local {
                times::apply_kind(&mut table, col, TimeKind::Time);
            }
        }
        Ok(table)
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let json = crate::formats::json_reader::table_to_json_array(table);
        let mut toml_value = json_to_toml(&json);
        // Time columns go out as TOML local times, not strings.
        if let toml::Value::Array(rows) = &mut toml_value {
            for (i, col) in table.columns.iter().enumerate() {
                if TimeKind::of_type(&col.data_type) != Some(TimeKind::Time) {
                    continue;
                }
                for (r, row) in rows.iter_mut().enumerate() {
                    if let (Some(CellValue::Time(_)), toml::Value::Table(t)) =
                        (table.get(r, i), row)
                        && let Some(toml::Value::String(s)) = t.get(&col.name)
                        && let Ok(dt) = s.parse::<toml::value::Datetime>()
                    {
                        t.insert(col.name.clone(), toml::Value::Datetime(dt));
                    }
                }
            }
        }
        // A TOML document is a table, so the rows go under a `rows` key (the
        // reader finds the array of tables again). A bare top-level array was
        // refused by the serializer, so every TOML save failed before.
        let mut doc = toml::map::Map::new();
        doc.insert("rows".to_string(), toml_value);
        let content = toml::to_string_pretty(&toml::Value::Table(doc))?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

pub fn json_to_toml(value: &serde_json::Value) -> toml::Value {
    match value {
        serde_json::Value::Null => toml::Value::String(String::new()),
        serde_json::Value::Bool(b) => toml::Value::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                toml::Value::Float(f)
            } else {
                toml::Value::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => toml::Value::String(s.clone()),
        serde_json::Value::Array(arr) => toml::Value::Array(arr.iter().map(json_to_toml).collect()),
        serde_json::Value::Object(map) => {
            let table: toml::map::Map<String, toml::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_toml(v)))
                .collect();
            toml::Value::Table(table)
        }
    }
}

pub fn toml_to_json(value: &toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::json!(*i),
        toml::Value::Float(f) => serde_json::json!(*f),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
        toml::Value::Array(arr) => serde_json::Value::Array(arr.iter().map(toml_to_json).collect()),
        toml::Value::Table(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), toml_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
    }
}

/// The text of every TOML local time (no date, no offset) in `value`.
fn collect_local_times(value: &toml::Value, out: &mut std::collections::HashSet<String>) {
    match value {
        toml::Value::Datetime(dt) if dt.date.is_none() && dt.offset.is_none() => {
            out.insert(dt.to_string());
        }
        toml::Value::Array(a) => a.iter().for_each(|v| collect_local_times(v, out)),
        toml::Value::Table(m) => m.values().for_each(|v| collect_local_times(v, out)),
        _ => {}
    }
}
