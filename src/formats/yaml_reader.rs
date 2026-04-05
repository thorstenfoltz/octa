use crate::data::DataTable;
use crate::formats::FormatReader;
use anyhow::Result;
use std::path::Path;

pub struct YamlReader;

impl FormatReader for YamlReader {
    fn name(&self) -> &str {
        "YAML"
    }

    fn extensions(&self) -> &[&str] {
        &["yaml", "yml"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let content = std::fs::read_to_string(path)?;
        let value: serde_yaml::Value = serde_yaml::from_str(&content)?;
        let json_value = yaml_to_json(&value);
        crate::formats::json_reader::json_to_table(json_value, path, "YAML")
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let json = crate::formats::json_reader::table_to_json_array(table);
        let content = serde_yaml::to_string(&json)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

pub fn yaml_to_json(value: &serde_yaml::Value) -> serde_json::Value {
    match value {
        serde_yaml::Value::Null => serde_json::Value::Null,
        serde_yaml::Value::Bool(b) => serde_json::Value::Bool(*b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::json!(i)
            } else if let Some(f) = n.as_f64() {
                serde_json::json!(f)
            } else {
                serde_json::Value::String(n.to_string())
            }
        }
        serde_yaml::Value::String(s) => serde_json::Value::String(s.clone()),
        serde_yaml::Value::Sequence(seq) => {
            serde_json::Value::Array(seq.iter().map(yaml_to_json).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| {
                    let key = match k {
                        serde_yaml::Value::String(s) => s.clone(),
                        _ => format!("{:?}", k),
                    };
                    (key, yaml_to_json(v))
                })
                .collect();
            serde_json::Value::Object(obj)
        }
        serde_yaml::Value::Tagged(tagged) => yaml_to_json(&tagged.value),
    }
}
