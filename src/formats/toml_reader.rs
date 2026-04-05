use crate::data::DataTable;
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
        let value: toml::Value = content.parse()?;
        let json_value = toml_to_json(&value);
        crate::formats::json_reader::json_to_table(json_value, path, "TOML")
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let json = crate::formats::json_reader::table_to_json_array(table);
        let toml_value = json_to_toml(&json);
        let content = toml::to_string_pretty(&toml_value)?;
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
