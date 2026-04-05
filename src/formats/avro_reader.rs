use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use apache_avro::types::Value as AvroValue;
use apache_avro::Reader as AvroFileReader;
use apache_avro::Writer as AvroFileWriter;
use std::path::Path;

pub struct AvroReader;

impl FormatReader for AvroReader {
    fn name(&self) -> &str {
        "Avro"
    }

    fn extensions(&self) -> &[&str] {
        &["avro"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let file = std::fs::File::open(path)?;
        let reader = AvroFileReader::new(file)?;
        let schema = reader.writer_schema().clone();

        // Extract column info from schema
        let columns: Vec<ColumnInfo> = match &schema {
            apache_avro::Schema::Record(record) => record
                .fields
                .iter()
                .map(|f| ColumnInfo {
                    name: f.name.clone(),
                    data_type: avro_type_string(&f.schema),
                })
                .collect(),
            _ => vec![ColumnInfo {
                name: "value".to_string(),
                data_type: "Utf8".to_string(),
            }],
        };

        let mut rows = Vec::new();
        for value in reader {
            let value = value?;
            match &value {
                AvroValue::Record(fields) => {
                    let row: Vec<CellValue> = columns
                        .iter()
                        .map(|col| {
                            fields
                                .iter()
                                .find(|(name, _)| name == &col.name)
                                .map(|(_, v)| avro_to_cell(v))
                                .unwrap_or(CellValue::Null)
                        })
                        .collect();
                    rows.push(row);
                }
                _ => {
                    rows.push(vec![avro_to_cell(&value)]);
                }
            }
        }

        Ok(DataTable {
            columns,
            rows,
            edits: std::collections::HashMap::new(),
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("Avro".to_string()),
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: std::collections::HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        })
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let fields: Vec<apache_avro::schema::RecordField> = table
            .columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let schema = data_type_to_avro_schema(&col.data_type);
                // Wrap in Union with Null to allow nulls
                let nullable = apache_avro::Schema::Union(
                    apache_avro::schema::UnionSchema::new(vec![apache_avro::Schema::Null, schema])
                        .unwrap(),
                );
                apache_avro::schema::RecordField {
                    name: col.name.clone(),
                    doc: None,
                    aliases: None,
                    default: Some(serde_json::Value::Null),
                    schema: nullable,
                    order: apache_avro::schema::RecordFieldOrder::Ascending,
                    position: i,
                    custom_attributes: Default::default(),
                }
            })
            .collect();

        let schema = apache_avro::Schema::Record(apache_avro::schema::RecordSchema {
            name: apache_avro::schema::Name {
                name: "Row".to_string(),
                namespace: None,
            },
            aliases: None,
            doc: None,
            fields,
            lookup: {
                let mut map = std::collections::BTreeMap::new();
                for (i, col) in table.columns.iter().enumerate() {
                    map.insert(col.name.clone(), i);
                }
                map
            },
            attributes: Default::default(),
        });

        let file = std::fs::File::create(path)?;
        let mut writer = AvroFileWriter::new(&schema, file);

        for row_idx in 0..table.row_count() {
            let mut record_fields: Vec<(String, AvroValue)> = Vec::new();
            for (col_idx, col) in table.columns.iter().enumerate() {
                let cell = table
                    .get(row_idx, col_idx)
                    .cloned()
                    .unwrap_or(CellValue::Null);
                let avro_val = cell_to_avro(&cell);
                // Wrap in Union (index 0 = Null, index 1 = value)
                let union_val = match avro_val {
                    AvroValue::Null => AvroValue::Union(0, Box::new(AvroValue::Null)),
                    other => AvroValue::Union(1, Box::new(other)),
                };
                record_fields.push((col.name.clone(), union_val));
            }
            writer.append(AvroValue::Record(record_fields))?;
        }
        writer.flush()?;
        Ok(())
    }
}

fn data_type_to_avro_schema(data_type: &str) -> apache_avro::Schema {
    match data_type {
        "Boolean" => apache_avro::Schema::Boolean,
        "Int32" => apache_avro::Schema::Int,
        "Int64" => apache_avro::Schema::Long,
        "Float32" => apache_avro::Schema::Float,
        "Float64" => apache_avro::Schema::Double,
        "Binary" => apache_avro::Schema::Bytes,
        _ => apache_avro::Schema::String,
    }
}

fn cell_to_avro(cell: &CellValue) -> AvroValue {
    match cell {
        CellValue::Null => AvroValue::Null,
        CellValue::Bool(b) => AvroValue::Boolean(*b),
        CellValue::Int(i) => AvroValue::Long(*i),
        CellValue::Float(f) => AvroValue::Double(*f),
        CellValue::String(s) => AvroValue::String(s.clone()),
        CellValue::Date(s) => AvroValue::String(s.clone()),
        CellValue::DateTime(s) => AvroValue::String(s.clone()),
        CellValue::Binary(b) => AvroValue::Bytes(b.clone()),
        CellValue::Nested(s) => AvroValue::String(s.clone()),
    }
}

fn avro_type_string(schema: &apache_avro::Schema) -> String {
    match schema {
        apache_avro::Schema::Null => "Null".to_string(),
        apache_avro::Schema::Boolean => "Boolean".to_string(),
        apache_avro::Schema::Int => "Int32".to_string(),
        apache_avro::Schema::Long => "Int64".to_string(),
        apache_avro::Schema::Float => "Float32".to_string(),
        apache_avro::Schema::Double => "Float64".to_string(),
        apache_avro::Schema::String => "Utf8".to_string(),
        apache_avro::Schema::Bytes => "Binary".to_string(),
        apache_avro::Schema::Union(u) => {
            let non_null: Vec<_> = u
                .variants()
                .iter()
                .filter(|s| !matches!(s, apache_avro::Schema::Null))
                .collect();
            if non_null.len() == 1 {
                avro_type_string(non_null[0])
            } else {
                "Utf8".to_string()
            }
        }
        _ => "Utf8".to_string(),
    }
}

fn avro_to_cell(value: &AvroValue) -> CellValue {
    match value {
        AvroValue::Null => CellValue::Null,
        AvroValue::Boolean(b) => CellValue::Bool(*b),
        AvroValue::Int(i) => CellValue::Int(*i as i64),
        AvroValue::Long(i) => CellValue::Int(*i),
        AvroValue::Float(f) => CellValue::Float(*f as f64),
        AvroValue::Double(f) => CellValue::Float(*f),
        AvroValue::String(s) => CellValue::String(s.clone()),
        AvroValue::Bytes(b) => CellValue::Binary(b.clone()),
        AvroValue::Union(_, inner) => avro_to_cell(inner),
        AvroValue::Date(days) => {
            let date = chrono::NaiveDate::from_num_days_from_ce_opt(*days + 719_163);
            match date {
                Some(d) => CellValue::Date(d.format("%Y-%m-%d").to_string()),
                None => CellValue::String(format!("date({})", days)),
            }
        }
        AvroValue::TimestampMillis(ms) => {
            let secs = ms / 1000;
            let nsecs = ((ms % 1000) * 1_000_000) as u32;
            match chrono::DateTime::from_timestamp(secs, nsecs) {
                Some(dt) => CellValue::DateTime(dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string()),
                None => CellValue::String(format!("ts_ms({})", ms)),
            }
        }
        AvroValue::TimestampMicros(us) => {
            let secs = us / 1_000_000;
            let nsecs = ((us % 1_000_000) * 1000) as u32;
            match chrono::DateTime::from_timestamp(secs, nsecs) {
                Some(dt) => CellValue::DateTime(dt.format("%Y-%m-%d %H:%M:%S%.6f").to_string()),
                None => CellValue::String(format!("ts_us({})", us)),
            }
        }
        AvroValue::Record(fields) => {
            let json: serde_json::Map<String, serde_json::Value> = fields
                .iter()
                .map(|(k, v)| (k.clone(), avro_value_to_json(v)))
                .collect();
            CellValue::Nested(serde_json::to_string(&json).unwrap_or_default())
        }
        AvroValue::Array(arr) => {
            let json: Vec<serde_json::Value> = arr.iter().map(avro_value_to_json).collect();
            CellValue::Nested(serde_json::to_string(&json).unwrap_or_default())
        }
        _ => CellValue::String(format!("{:?}", value)),
    }
}

fn avro_value_to_json(value: &AvroValue) -> serde_json::Value {
    match value {
        AvroValue::Null => serde_json::Value::Null,
        AvroValue::Boolean(b) => serde_json::Value::Bool(*b),
        AvroValue::Int(i) => serde_json::json!(*i),
        AvroValue::Long(i) => serde_json::json!(*i),
        AvroValue::Float(f) => serde_json::json!(*f),
        AvroValue::Double(f) => serde_json::json!(*f),
        AvroValue::String(s) => serde_json::Value::String(s.clone()),
        AvroValue::Union(_, inner) => avro_value_to_json(inner),
        _ => serde_json::Value::String(format!("{:?}", value)),
    }
}
