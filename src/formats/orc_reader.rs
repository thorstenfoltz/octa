use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use std::path::Path;

// orc-rust uses arrow v58 internally; we import it as `arrow58` to avoid
// conflicting with the project's main arrow v54 dependency.
use arrow58::array::*;
use arrow58::datatypes::{DataType, Field, Schema, TimeUnit};

pub struct OrcReader;

impl FormatReader for OrcReader {
    fn name(&self) -> &str {
        "ORC"
    }

    fn extensions(&self) -> &[&str] {
        &["orc"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        use orc_rust::ArrowReaderBuilder;

        let file = std::fs::File::open(path)?;
        let builder = ArrowReaderBuilder::try_new(file)?;
        let schema = builder.schema();

        let columns: Vec<ColumnInfo> = schema
            .fields()
            .iter()
            .map(|f| ColumnInfo {
                name: f.name().clone(),
                data_type: arrow_type_to_string(f.data_type()),
            })
            .collect();

        let reader = builder.build();
        let mut rows = Vec::new();

        for batch_result in reader {
            let batch = batch_result?;
            let num_rows = batch.num_rows();
            let num_cols = batch.num_columns();
            for row_idx in 0..num_rows {
                let mut row = Vec::with_capacity(num_cols);
                for col_idx in 0..num_cols {
                    let array = batch.column(col_idx);
                    row.push(arrow_value_to_cell(array.as_ref(), row_idx));
                }
                rows.push(row);
            }
        }

        Ok(DataTable {
            columns,
            rows,
            edits: std::collections::HashMap::new(),
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("ORC".to_string()),
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: std::collections::HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        })
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        use orc_rust::ArrowWriterBuilder;

        let schema = build_orc_schema(table);
        let file = std::fs::File::create(path)?;
        let mut writer = ArrowWriterBuilder::new(file, schema.clone()).try_build()?;

        let batch = build_record_batch(table, &schema)?;
        writer.write(&batch)?;
        writer.close()?;
        Ok(())
    }
}

fn arrow_type_to_string(dt: &DataType) -> String {
    match dt {
        DataType::Boolean => "Boolean".to_string(),
        DataType::Int8 => "Int8".to_string(),
        DataType::Int16 => "Int16".to_string(),
        DataType::Int32 => "Int32".to_string(),
        DataType::Int64 => "Int64".to_string(),
        DataType::UInt8 => "UInt8".to_string(),
        DataType::UInt16 => "UInt16".to_string(),
        DataType::UInt32 => "UInt32".to_string(),
        DataType::UInt64 => "UInt64".to_string(),
        DataType::Float16 => "Float16".to_string(),
        DataType::Float32 => "Float32".to_string(),
        DataType::Float64 => "Float64".to_string(),
        DataType::Utf8 | DataType::LargeUtf8 => "Utf8".to_string(),
        DataType::Binary | DataType::LargeBinary => "Binary".to_string(),
        DataType::Date32 | DataType::Date64 => "Date32".to_string(),
        DataType::Timestamp(_, _) => "Timestamp".to_string(),
        DataType::Decimal128(_, _) | DataType::Decimal256(_, _) => "Utf8".to_string(),
        _ => "Utf8".to_string(),
    }
}

fn arrow_value_to_cell(array: &dyn Array, row: usize) -> CellValue {
    if array.is_null(row) {
        return CellValue::Null;
    }

    match array.data_type() {
        DataType::Boolean => {
            let a = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            CellValue::Bool(a.value(row))
        }
        DataType::Int8 => {
            let a = array.as_any().downcast_ref::<Int8Array>().unwrap();
            CellValue::Int(a.value(row) as i64)
        }
        DataType::Int16 => {
            let a = array.as_any().downcast_ref::<Int16Array>().unwrap();
            CellValue::Int(a.value(row) as i64)
        }
        DataType::Int32 => {
            let a = array.as_any().downcast_ref::<Int32Array>().unwrap();
            CellValue::Int(a.value(row) as i64)
        }
        DataType::Int64 => {
            let a = array.as_any().downcast_ref::<Int64Array>().unwrap();
            CellValue::Int(a.value(row))
        }
        DataType::UInt8 => {
            let a = array.as_any().downcast_ref::<UInt8Array>().unwrap();
            CellValue::Int(a.value(row) as i64)
        }
        DataType::UInt16 => {
            let a = array.as_any().downcast_ref::<UInt16Array>().unwrap();
            CellValue::Int(a.value(row) as i64)
        }
        DataType::UInt32 => {
            let a = array.as_any().downcast_ref::<UInt32Array>().unwrap();
            CellValue::Int(a.value(row) as i64)
        }
        DataType::UInt64 => {
            let a = array.as_any().downcast_ref::<UInt64Array>().unwrap();
            CellValue::Int(a.value(row) as i64)
        }
        DataType::Float32 => {
            let a = array.as_any().downcast_ref::<Float32Array>().unwrap();
            CellValue::Float(a.value(row) as f64)
        }
        DataType::Float64 => {
            let a = array.as_any().downcast_ref::<Float64Array>().unwrap();
            CellValue::Float(a.value(row))
        }
        DataType::Utf8 => {
            let a = array.as_any().downcast_ref::<StringArray>().unwrap();
            CellValue::String(a.value(row).to_string())
        }
        DataType::LargeUtf8 => {
            let a = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
            CellValue::String(a.value(row).to_string())
        }
        DataType::Binary => {
            let a = array.as_any().downcast_ref::<BinaryArray>().unwrap();
            CellValue::Binary(a.value(row).to_vec())
        }
        DataType::LargeBinary => {
            let a = array.as_any().downcast_ref::<LargeBinaryArray>().unwrap();
            CellValue::Binary(a.value(row).to_vec())
        }
        DataType::Date32 => {
            let a = array.as_any().downcast_ref::<Date32Array>().unwrap();
            let days = a.value(row);
            let date = chrono::NaiveDate::from_num_days_from_ce_opt(days + 719_163);
            match date {
                Some(d) => CellValue::Date(d.format("%Y-%m-%d").to_string()),
                None => CellValue::String(format!("{days}")),
            }
        }
        DataType::Date64 => {
            let a = array.as_any().downcast_ref::<Date64Array>().unwrap();
            let ms = a.value(row);
            let secs = ms / 1000;
            let nsecs = ((ms % 1000) * 1_000_000) as u32;
            match chrono::DateTime::from_timestamp(secs, nsecs) {
                Some(dt) => CellValue::Date(dt.format("%Y-%m-%d").to_string()),
                None => CellValue::String(format!("{ms}")),
            }
        }
        DataType::Timestamp(unit, _) => {
            let (secs, nsecs) = match unit {
                TimeUnit::Second => {
                    let a = array
                        .as_any()
                        .downcast_ref::<TimestampSecondArray>()
                        .unwrap();
                    (a.value(row), 0u32)
                }
                TimeUnit::Millisecond => {
                    let a = array
                        .as_any()
                        .downcast_ref::<TimestampMillisecondArray>()
                        .unwrap();
                    let ms = a.value(row);
                    (ms / 1000, ((ms % 1000) * 1_000_000) as u32)
                }
                TimeUnit::Microsecond => {
                    let a = array
                        .as_any()
                        .downcast_ref::<TimestampMicrosecondArray>()
                        .unwrap();
                    let us = a.value(row);
                    (us / 1_000_000, ((us % 1_000_000) * 1000) as u32)
                }
                TimeUnit::Nanosecond => {
                    let a = array
                        .as_any()
                        .downcast_ref::<TimestampNanosecondArray>()
                        .unwrap();
                    let ns = a.value(row);
                    (ns / 1_000_000_000, (ns % 1_000_000_000) as u32)
                }
            };
            match chrono::DateTime::from_timestamp(secs, nsecs) {
                Some(dt) => CellValue::DateTime(dt.format("%Y-%m-%d %H:%M:%S%.9f").to_string()),
                None => CellValue::String(format!("{secs}")),
            }
        }
        DataType::Decimal128(_, scale) => {
            let a = array.as_any().downcast_ref::<Decimal128Array>().unwrap();
            let v = a.value(row);
            let scale = *scale as u32;
            if scale == 0 {
                CellValue::String(v.to_string())
            } else {
                let divisor = 10i128.pow(scale);
                let int_part = v / divisor;
                let frac_part = (v % divisor).unsigned_abs();
                CellValue::String(format!(
                    "{int_part}.{frac_part:0>width$}",
                    width = scale as usize
                ))
            }
        }
        _ => {
            let formatted = arrow58::util::display::array_value_to_string(array, row);
            match formatted {
                Ok(s) => CellValue::String(s),
                Err(_) => CellValue::Null,
            }
        }
    }
}

fn build_orc_schema(table: &DataTable) -> arrow58::datatypes::SchemaRef {
    let fields: Vec<Field> = table
        .columns
        .iter()
        .map(|col| {
            let dt = match col.data_type.as_str() {
                "Boolean" => DataType::Boolean,
                "Int8" => DataType::Int8,
                "Int16" => DataType::Int16,
                "Int32" => DataType::Int32,
                "Int64" => DataType::Int64,
                "UInt8" => DataType::UInt8,
                "UInt16" => DataType::UInt16,
                "UInt32" => DataType::UInt32,
                "UInt64" => DataType::UInt64,
                "Float32" => DataType::Float32,
                "Float64" => DataType::Float64,
                "Binary" => DataType::Binary,
                "Date32" => DataType::Date32,
                _ => DataType::Utf8,
            };
            Field::new(&col.name, dt, true)
        })
        .collect();

    std::sync::Arc::new(Schema::new(fields))
}

fn build_record_batch(
    table: &DataTable,
    schema: &arrow58::datatypes::SchemaRef,
) -> Result<arrow58::record_batch::RecordBatch> {
    let mut arrays: Vec<ArrayRef> = Vec::new();

    for (col_idx, field) in schema.fields().iter().enumerate() {
        let array: ArrayRef = match field.data_type() {
            DataType::Boolean => {
                let values: Vec<Option<bool>> = (0..table.row_count())
                    .map(|row| match table.get(row, col_idx) {
                        Some(CellValue::Bool(b)) => Some(*b),
                        Some(CellValue::Null) | None => None,
                        Some(v) => v.to_string().parse().ok(),
                    })
                    .collect();
                std::sync::Arc::new(BooleanArray::from(values))
            }
            DataType::Int32 => {
                let values: Vec<Option<i32>> = (0..table.row_count())
                    .map(|row| cell_to_int(table.get(row, col_idx)).map(|v| v as i32))
                    .collect();
                std::sync::Arc::new(Int32Array::from(values))
            }
            DataType::Int64 => {
                let values: Vec<Option<i64>> = (0..table.row_count())
                    .map(|row| cell_to_int(table.get(row, col_idx)))
                    .collect();
                std::sync::Arc::new(Int64Array::from(values))
            }
            DataType::Float32 => {
                let values: Vec<Option<f32>> = (0..table.row_count())
                    .map(|row| cell_to_float(table.get(row, col_idx)).map(|v| v as f32))
                    .collect();
                std::sync::Arc::new(Float32Array::from(values))
            }
            DataType::Float64 => {
                let values: Vec<Option<f64>> = (0..table.row_count())
                    .map(|row| cell_to_float(table.get(row, col_idx)))
                    .collect();
                std::sync::Arc::new(Float64Array::from(values))
            }
            DataType::Binary => {
                let values: Vec<Option<Vec<u8>>> = (0..table.row_count())
                    .map(|row| match table.get(row, col_idx) {
                        Some(CellValue::Binary(b)) => Some(b.clone()),
                        Some(CellValue::Null) | None => None,
                        Some(v) => Some(v.to_string().into_bytes()),
                    })
                    .collect();
                let refs: Vec<Option<&[u8]>> = values.iter().map(|v| v.as_deref()).collect();
                std::sync::Arc::new(BinaryArray::from(refs))
            }
            DataType::Date32 => {
                let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
                let values: Vec<Option<i32>> = (0..table.row_count())
                    .map(|row| match table.get(row, col_idx) {
                        Some(CellValue::Date(s)) => {
                            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                                .ok()
                                .map(|d| d.signed_duration_since(epoch).num_days() as i32)
                        }
                        Some(CellValue::Null) | None => None,
                        _ => None,
                    })
                    .collect();
                std::sync::Arc::new(Date32Array::from(values))
            }
            _ => {
                let values: Vec<Option<String>> = (0..table.row_count())
                    .map(|row| match table.get(row, col_idx) {
                        Some(CellValue::Null) | None => None,
                        Some(v) => Some(v.to_string()),
                    })
                    .collect();
                let refs: Vec<Option<&str>> = values
                    .iter()
                    .map(|v: &Option<String>| v.as_deref())
                    .collect();
                std::sync::Arc::new(StringArray::from(refs))
            }
        };
        arrays.push(array);
    }

    Ok(arrow58::record_batch::RecordBatch::try_new(
        schema.clone(),
        arrays,
    )?)
}

fn cell_to_int(cell: Option<&CellValue>) -> Option<i64> {
    match cell {
        Some(CellValue::Int(i)) => Some(*i),
        Some(CellValue::Float(f)) => Some(*f as i64),
        Some(CellValue::Null) | None => None,
        Some(v) => v.to_string().parse().ok(),
    }
}

fn cell_to_float(cell: Option<&CellValue>) -> Option<f64> {
    match cell {
        Some(CellValue::Float(f)) => Some(*f),
        Some(CellValue::Int(i)) => Some(*i as f64),
        Some(CellValue::Null) | None => None,
        Some(v) => v.to_string().parse().ok(),
    }
}
