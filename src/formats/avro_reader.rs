use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use apache_avro::Reader as AvroFileReader;
use apache_avro::Writer as AvroFileWriter;
use apache_avro::types::Value as AvroValue;
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

        let mut table = DataTable {
            columns,
            rows,
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("Avro".to_string()),
            ..DataTable::empty()
        };
        crate::data::times::settle_interval_columns(&mut table);
        Ok(table)
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        let fields: Vec<apache_avro::schema::RecordField> = table
            .columns
            .iter()
            .enumerate()
            .map(|(col_idx, col)| {
                let schema = if is_time_column(table, col_idx) {
                    apache_avro::Schema::TimeMicros
                } else {
                    data_type_to_avro_schema(&col.data_type)
                };
                // Wrap in Union with Null to allow nulls
                let nullable = apache_avro::Schema::Union(
                    apache_avro::schema::UnionSchema::new(vec![apache_avro::Schema::Null, schema])
                        .unwrap(),
                );
                apache_avro::schema::RecordField::builder()
                    .name(col.name.clone())
                    .default(serde_json::Value::Null)
                    .schema(nullable)
                    .build()
            })
            .collect();

        // The builder fills the field lookup itself.
        let schema = apache_avro::Schema::Record(
            apache_avro::schema::RecordSchema::builder()
                .name(apache_avro::schema::Name::new("Row")?)
                .fields(fields)
                .build(),
        );

        let file = std::fs::File::create(path)?;
        let mut writer = AvroFileWriter::new(&schema, file)?;

        for row_idx in 0..table.row_count() {
            let mut record_fields: Vec<(String, AvroValue)> = Vec::new();
            for (col_idx, col) in table.columns.iter().enumerate() {
                let cell = table
                    .get(row_idx, col_idx)
                    .cloned()
                    .unwrap_or(CellValue::Null);
                let avro_val = match cell {
                    CellValue::Time(ns) if is_time_column(table, col_idx) => {
                        AvroValue::TimeMicros(ns / 1_000)
                    }
                    _ => cell_to_avro(&cell),
                };
                // Wrap in Union (index 0 = Null, index 1 = value)
                let union_val = match avro_val {
                    AvroValue::Null => AvroValue::Union(0, Box::new(AvroValue::Null)),
                    other => AvroValue::Union(1, Box::new(other)),
                };
                record_fields.push((col.name.clone(), union_val));
            }
            writer.append_value(AvroValue::Record(record_fields))?;
        }
        writer.flush()?;
        Ok(())
    }
}

/// A Time column whose every cell is a time (or null) is written as Avro
/// `time-micros`; anything else (text left by a failed parse) stays a string.
fn is_time_column(table: &DataTable, col: usize) -> bool {
    crate::data::times::TimeKind::of_type(&table.columns[col].data_type)
        == Some(crate::data::times::TimeKind::Time)
        && table.rows.iter().all(|r| {
            matches!(
                r.get(col),
                Some(CellValue::Time(_) | CellValue::Null) | None
            )
        })
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
        CellValue::Time(_) | CellValue::Duration(_) => AvroValue::String(cell.to_string()),
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
        apache_avro::Schema::TimeMillis => "Time32(ms)".to_string(),
        apache_avro::Schema::TimeMicros => "Time64(µs)".to_string(),
        apache_avro::Schema::Duration(_) => "Duration(ms)".to_string(),
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
            let (secs, nsecs) = crate::formats::parquet_reader::epoch_split(*ms, 1_000);
            match chrono::DateTime::from_timestamp(secs, nsecs) {
                Some(dt) => CellValue::DateTime(dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string()),
                None => CellValue::String(format!("ts_ms({})", ms)),
            }
        }
        AvroValue::TimestampMicros(us) => {
            let (secs, nsecs) = crate::formats::parquet_reader::epoch_split(*us, 1_000_000);
            match chrono::DateTime::from_timestamp(secs, nsecs) {
                Some(dt) => CellValue::DateTime(dt.format("%Y-%m-%d %H:%M:%S%.6f").to_string()),
                None => CellValue::String(format!("ts_us({})", us)),
            }
        }
        AvroValue::TimeMillis(ms) => CellValue::Time(i64::from(*ms) * 1_000_000),
        AvroValue::TimeMicros(us) => CellValue::Time(*us * 1_000),
        AvroValue::Duration(d) => {
            let months = u32::from(d.months());
            let days = i64::from(u32::from(d.days()));
            let ms = i64::from(u32::from(d.millis()));
            if months == 0 {
                CellValue::Duration((days * 86_400_000 + ms) * 1_000_000)
            } else {
                CellValue::String(format!("{months} months {days} days {ms} ms"))
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
