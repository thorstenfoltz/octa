use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use arrow::array::*;
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

pub struct ParquetReader;

impl FormatReader for ParquetReader {
    fn name(&self) -> &str {
        "Parquet"
    }

    fn extensions(&self) -> &[&str] {
        &["parquet", "pq", "parq"]
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        write_parquet(path, table)
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let file = File::open(path)?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
        let schema = builder.schema().clone();

        // Get total row count from Parquet metadata without reading data
        let metadata = builder.metadata();
        let total_file_rows: usize = metadata
            .row_groups()
            .iter()
            .map(|rg| rg.num_rows() as usize)
            .sum();

        // Limit: load at most MAX_ROWS rows to avoid OOM
        const MAX_ROWS: usize = 1_000_000;
        let truncated = total_file_rows > MAX_ROWS;

        let reader = builder.with_batch_size(8192).build()?;

        // Build column info from Arrow schema
        let columns: Vec<ColumnInfo> = schema
            .fields()
            .iter()
            .map(|f| ColumnInfo {
                name: f.name().clone(),
                data_type: format!("{}", f.data_type()),
            })
            .collect();

        let mut rows: Vec<Vec<CellValue>> = Vec::new();
        let mut loaded = 0usize;

        'outer: for batch_result in reader {
            let batch = batch_result?;
            let num_rows = batch.num_rows();
            let num_cols = batch.num_columns();

            for row_idx in 0..num_rows {
                if loaded >= MAX_ROWS {
                    break 'outer;
                }
                let mut row = Vec::with_capacity(num_cols);
                for col_idx in 0..num_cols {
                    let array = batch.column(col_idx);
                    let value = arrow_value_to_cell(array, row_idx);
                    row.push(value);
                }
                rows.push(row);
                loaded += 1;
            }
        }

        Ok(DataTable {
            columns,
            rows,
            edits: std::collections::HashMap::new(),
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("Parquet".to_string()),
            structural_changes: false,
            total_rows: if truncated {
                Some(total_file_rows)
            } else {
                None
            },
            row_offset: 0,
            marks: std::collections::HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        })
    }
}

/// Convert an Arrow array value at a given index to a CellValue.
pub fn arrow_value_to_cell(array: &dyn Array, idx: usize) -> CellValue {
    if array.is_null(idx) {
        return CellValue::Null;
    }

    match array.data_type() {
        DataType::Boolean => {
            let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            CellValue::Bool(arr.value(idx))
        }
        DataType::Int8 => {
            let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
            CellValue::Int(arr.value(idx) as i64)
        }
        DataType::Int16 => {
            let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
            CellValue::Int(arr.value(idx) as i64)
        }
        DataType::Int32 => {
            let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
            CellValue::Int(arr.value(idx) as i64)
        }
        DataType::Int64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
            CellValue::Int(arr.value(idx))
        }
        DataType::UInt8 => {
            let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
            CellValue::Int(arr.value(idx) as i64)
        }
        DataType::UInt16 => {
            let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
            CellValue::Int(arr.value(idx) as i64)
        }
        DataType::UInt32 => {
            let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
            CellValue::Int(arr.value(idx) as i64)
        }
        DataType::UInt64 => {
            let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
            CellValue::Int(arr.value(idx) as i64)
        }
        DataType::Float16 => {
            let arr = array.as_any().downcast_ref::<Float16Array>().unwrap();
            CellValue::Float(arr.value(idx).to_f64())
        }
        DataType::Float32 => {
            let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
            CellValue::Float(arr.value(idx) as f64)
        }
        DataType::Float64 => {
            let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
            CellValue::Float(arr.value(idx))
        }
        DataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
            CellValue::String(arr.value(idx).to_string())
        }
        DataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
            CellValue::String(arr.value(idx).to_string())
        }
        DataType::Binary => {
            let arr = array.as_any().downcast_ref::<BinaryArray>().unwrap();
            CellValue::Binary(arr.value(idx).to_vec())
        }
        DataType::LargeBinary => {
            let arr = array.as_any().downcast_ref::<LargeBinaryArray>().unwrap();
            CellValue::Binary(arr.value(idx).to_vec())
        }
        DataType::Date32 => {
            let arr = array.as_any().downcast_ref::<Date32Array>().unwrap();
            let days = arr.value(idx);
            let date = chrono::NaiveDate::from_num_days_from_ce_opt(days + 719_163);
            match date {
                Some(d) => CellValue::Date(d.format("%Y-%m-%d").to_string()),
                None => CellValue::String(format!("date32({})", days)),
            }
        }
        DataType::Date64 => {
            let arr = array.as_any().downcast_ref::<Date64Array>().unwrap();
            let ms = arr.value(idx);
            let secs = ms / 1000;
            let nsecs = ((ms % 1000) * 1_000_000) as u32;
            match chrono::DateTime::from_timestamp(secs, nsecs) {
                Some(dt) => CellValue::Date(dt.format("%Y-%m-%d").to_string()),
                None => CellValue::String(format!("date64({})", ms)),
            }
        }
        DataType::Timestamp(unit, _tz) => {
            let (secs, nsecs) = match unit {
                TimeUnit::Second => {
                    let arr = array
                        .as_any()
                        .downcast_ref::<TimestampSecondArray>()
                        .unwrap();
                    (arr.value(idx), 0u32)
                }
                TimeUnit::Millisecond => {
                    let arr = array
                        .as_any()
                        .downcast_ref::<TimestampMillisecondArray>()
                        .unwrap();
                    let v = arr.value(idx);
                    (v / 1000, ((v % 1000) * 1_000_000) as u32)
                }
                TimeUnit::Microsecond => {
                    let arr = array
                        .as_any()
                        .downcast_ref::<TimestampMicrosecondArray>()
                        .unwrap();
                    let v = arr.value(idx);
                    (v / 1_000_000, ((v % 1_000_000) * 1000) as u32)
                }
                TimeUnit::Nanosecond => {
                    let arr = array
                        .as_any()
                        .downcast_ref::<TimestampNanosecondArray>()
                        .unwrap();
                    let v = arr.value(idx);
                    (v / 1_000_000_000, (v % 1_000_000_000) as u32)
                }
            };
            match chrono::DateTime::from_timestamp(secs, nsecs) {
                Some(dt) => CellValue::DateTime(dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string()),
                None => CellValue::String(format!("timestamp({})", secs)),
            }
        }
        DataType::Decimal128(_, scale) => {
            let arr = array.as_any().downcast_ref::<Decimal128Array>().unwrap();
            let raw = arr.value(idx);
            let scale = *scale as u32;
            if scale == 0 {
                CellValue::Int(raw as i64)
            } else {
                let divisor = 10f64.powi(scale as i32);
                CellValue::Float(raw as f64 / divisor)
            }
        }
        DataType::Decimal256(_, scale) => {
            let arr = array.as_any().downcast_ref::<Decimal256Array>().unwrap();
            let raw = arr.value(idx);
            CellValue::String(format!("{}e-{}", raw, scale))
        }
        // Nested / semi-structured types: serialize to JSON-like string
        DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _) => {
            CellValue::Nested(format!("{:?}", array.slice(idx, 1)))
        }
        DataType::Struct(_) => CellValue::Nested(format!("{:?}", array.slice(idx, 1))),
        DataType::Map(_, _) => CellValue::Nested(format!("{:?}", array.slice(idx, 1))),
        _ => {
            // Fallback: use Debug representation
            CellValue::String(format!("{:?}", array.slice(idx, 1)))
        }
    }
}

/// Map a DataTable data_type string back to an Arrow DataType.
pub fn data_type_from_string(s: &str) -> DataType {
    match s.to_lowercase().as_str() {
        "boolean" | "bool" => DataType::Boolean,
        "int8" => DataType::Int8,
        "int16" => DataType::Int16,
        "int32" => DataType::Int32,
        "int64" | "int" => DataType::Int64,
        "uint8" => DataType::UInt8,
        "uint16" => DataType::UInt16,
        "uint32" => DataType::UInt32,
        "uint64" => DataType::UInt64,
        "float16" => DataType::Float16,
        "float32" => DataType::Float32,
        "float64" | "float" | "double" => DataType::Float64,
        "utf8" | "string" => DataType::Utf8,
        "largeutf8" | "largestring" => DataType::LargeUtf8,
        "binary" => DataType::Binary,
        "largebinary" => DataType::LargeBinary,
        "date32" | "date" => DataType::Date32,
        "date64" => DataType::Date64,
        s if s.starts_with("timestamp") => DataType::Timestamp(TimeUnit::Microsecond, None),
        "datetime" => DataType::Timestamp(TimeUnit::Microsecond, None),
        _ => DataType::Utf8, // fallback: store as string
    }
}

/// Write a DataTable to a Parquet file.
fn write_parquet(path: &Path, table: &DataTable) -> Result<()> {
    let fields: Vec<Field> = table
        .columns
        .iter()
        .map(|col| Field::new(&col.name, data_type_from_string(&col.data_type), true))
        .collect();
    let schema = Arc::new(Schema::new(fields));

    let file = File::create(path)?;
    let mut writer = ArrowWriter::try_new(file, schema.clone(), None)?;

    // Build Arrow arrays column by column
    let num_rows = table.row_count();
    let mut arrays: Vec<Arc<dyn Array>> = Vec::with_capacity(table.col_count());

    for col_idx in 0..table.col_count() {
        let arrow_type = data_type_from_string(&table.columns[col_idx].data_type);
        let array = build_arrow_array(&arrow_type, table, col_idx, num_rows);
        arrays.push(array);
    }

    let batch = arrow::record_batch::RecordBatch::try_new(schema, arrays)?;
    writer.write(&batch)?;
    writer.close()?;

    Ok(())
}

/// Build an Arrow array from a column of the DataTable.
pub fn build_arrow_array(
    arrow_type: &DataType,
    table: &DataTable,
    col_idx: usize,
    num_rows: usize,
) -> Arc<dyn Array> {
    match arrow_type {
        DataType::Boolean => {
            let mut builder = BooleanBuilder::with_capacity(num_rows);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::Bool(v)) => builder.append_value(*v),
                    Some(CellValue::Null) | None => builder.append_null(),
                    Some(v) => {
                        let s = v.to_string().to_lowercase();
                        match s.as_str() {
                            "true" | "1" | "yes" => builder.append_value(true),
                            "false" | "0" | "no" => builder.append_value(false),
                            _ => builder.append_null(),
                        }
                    }
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => {
            let mut builder = Int64Builder::with_capacity(num_rows);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::Int(v)) => builder.append_value(*v),
                    Some(CellValue::Float(v)) => builder.append_value(*v as i64),
                    Some(CellValue::Null) | None => builder.append_null(),
                    Some(v) => match v.to_string().parse::<i64>() {
                        Ok(i) => builder.append_value(i),
                        Err(_) => builder.append_null(),
                    },
                }
            }
            Arc::new(builder.finish())
        }
        DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
            let mut builder = UInt64Builder::with_capacity(num_rows);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::Int(v)) => builder.append_value(*v as u64),
                    Some(CellValue::Float(v)) => builder.append_value(*v as u64),
                    Some(CellValue::Null) | None => builder.append_null(),
                    Some(v) => match v.to_string().parse::<u64>() {
                        Ok(i) => builder.append_value(i),
                        Err(_) => builder.append_null(),
                    },
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Float16 | DataType::Float32 | DataType::Float64 => {
            let mut builder = Float64Builder::with_capacity(num_rows);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::Float(v)) => builder.append_value(*v),
                    Some(CellValue::Int(v)) => builder.append_value(*v as f64),
                    Some(CellValue::Null) | None => builder.append_null(),
                    Some(v) => match v.to_string().parse::<f64>() {
                        Ok(f) => builder.append_value(f),
                        Err(_) => builder.append_null(),
                    },
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Date32 => {
            let mut builder = Date32Builder::with_capacity(num_rows);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::Date(s)) => {
                        if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                            let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
                            let days = (d - epoch).num_days() as i32;
                            builder.append_value(days);
                        } else {
                            builder.append_null();
                        }
                    }
                    Some(CellValue::Null) | None => builder.append_null(),
                    _ => builder.append_null(),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::Timestamp(_, _) => {
            let mut builder = TimestampMicrosecondBuilder::with_capacity(num_rows);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::DateTime(s)) => {
                        if let Ok(dt) =
                            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.3f")
                        {
                            builder.append_value(dt.and_utc().timestamp_micros());
                        } else if let Ok(dt) =
                            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                        {
                            builder.append_value(dt.and_utc().timestamp_micros());
                        } else {
                            builder.append_null();
                        }
                    }
                    Some(CellValue::Null) | None => builder.append_null(),
                    _ => builder.append_null(),
                }
            }
            Arc::new(builder.finish())
        }
        DataType::LargeUtf8 | DataType::LargeBinary => {
            let mut builder = LargeStringBuilder::with_capacity(num_rows, num_rows * 32);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::Null) | None => builder.append_null(),
                    Some(v) => builder.append_value(v.to_string()),
                }
            }
            Arc::new(builder.finish())
        }
        _ => {
            // Default: write as Utf8 string
            let mut builder = StringBuilder::with_capacity(num_rows, num_rows * 32);
            for row in 0..num_rows {
                match table.get(row, col_idx) {
                    Some(CellValue::Null) | None => builder.append_null(),
                    Some(v) => builder.append_value(v.to_string()),
                }
            }
            Arc::new(builder.finish())
        }
    }
}
