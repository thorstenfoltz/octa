use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Arc;

// `ambers` returns data as `arrow` v57 RecordBatch. We import that arrow
// version under an alias to coexist with the project's main arrow v54.
use arrow57::array::{
    Array, ArrayRef, BinaryViewArray, BooleanArray, Date32Array, DurationMicrosecondArray,
    Float64Array, Int8Array, Int16Array, Int32Array, Int64Array, RecordBatch, StringArray,
    StringViewArray, TimestampMicrosecondArray,
};
use arrow57::datatypes::{
    DataType as Arrow57DataType, Field as Arrow57Field, Schema as Arrow57Schema,
    TimeUnit as Arrow57TimeUnit,
};

pub struct SpssReader;

impl FormatReader for SpssReader {
    fn name(&self) -> &str {
        "SPSS"
    }

    fn extensions(&self) -> &[&str] {
        &["sav", "zsav"]
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        write_sav(path, table)
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let (batch, _meta) = ambers::read_sav(path)
            .with_context(|| format!("opening SPSS file {}", path.display()))?;

        let schema = batch.schema();
        let columns: Vec<ColumnInfo> = schema
            .fields()
            .iter()
            .map(|f| ColumnInfo {
                name: f.name().clone(),
                data_type: arrow57_type_string(f.data_type()).to_string(),
            })
            .collect();

        let num_rows = batch.num_rows();
        let num_cols = batch.num_columns();
        let mut rows: Vec<Vec<CellValue>> = Vec::with_capacity(num_rows);
        for row_idx in 0..num_rows {
            let mut row = Vec::with_capacity(num_cols);
            for col_idx in 0..num_cols {
                let array = batch.column(col_idx);
                row.push(arrow57_value_to_cell(array.as_ref(), row_idx));
            }
            rows.push(row);
        }

        Ok(DataTable {
            columns,
            rows,
            edits: std::collections::HashMap::new(),
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("SPSS".to_string()),
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: std::collections::HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        })
    }
}

/// Map an octa column data_type string to the arrow57 `DataType` we
/// expose to `ambers`. Unknown / non-mappable types fall through to
/// `Utf8` so we always write something parseable.
fn arrow57_type_for_column(data_type: &str) -> Arrow57DataType {
    match data_type {
        "Boolean" => Arrow57DataType::Boolean,
        "Int8" => Arrow57DataType::Int8,
        "Int16" => Arrow57DataType::Int16,
        "Int32" => Arrow57DataType::Int32,
        "Int64" => Arrow57DataType::Int64,
        // SPSS only stores numerics as Float64 internally; collapsing
        // Float32 here keeps the writer simple and matches what
        // `SpssMetadata::from_arrow_schema` expects.
        "Float32" | "Float64" => Arrow57DataType::Float64,
        "Date" | "Date32" => Arrow57DataType::Date32,
        "DateTime" | "Timestamp(Microsecond, None)" => {
            Arrow57DataType::Timestamp(Arrow57TimeUnit::Microsecond, None)
        }
        _ => Arrow57DataType::Utf8,
    }
}

fn cell_as_f64(cell: &CellValue) -> Option<f64> {
    match cell {
        CellValue::Float(f) => Some(*f),
        CellValue::Int(i) => Some(*i as f64),
        CellValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        CellValue::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn cell_as_i64(cell: &CellValue) -> Option<i64> {
    match cell {
        CellValue::Int(i) => Some(*i),
        CellValue::Bool(b) => Some(if *b { 1 } else { 0 }),
        CellValue::Float(f) if f.is_finite() && f.fract() == 0.0 => Some(*f as i64),
        CellValue::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// Convert a `CellValue::Date` string ("YYYY-MM-DD") to days since the
/// Unix epoch, matching the inverse of the read path.
fn date_string_to_days(s: &str) -> Option<i32> {
    use chrono::Datelike;
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .map(|d| d.num_days_from_ce() - 719_163)
}

/// Convert a `CellValue::DateTime` string to microseconds since the
/// Unix epoch. Accepts the same formats the read path produces.
fn datetime_string_to_micros(s: &str) -> Option<i64> {
    let dt = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.6f")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f"))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f"))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .ok()?;
    Some(dt.and_utc().timestamp_micros())
}

fn build_array(table: &DataTable, col_idx: usize, dt: &Arrow57DataType) -> ArrayRef {
    let n = table.row_count();
    let cell = |r: usize| table.get(r, col_idx).cloned().unwrap_or(CellValue::Null);
    match dt {
        Arrow57DataType::Boolean => {
            let v: Vec<Option<bool>> = (0..n)
                .map(|r| match cell(r) {
                    CellValue::Bool(b) => Some(b),
                    CellValue::Int(i) => Some(i != 0),
                    CellValue::Null => None,
                    other => cell_as_f64(&other).map(|f| f != 0.0),
                })
                .collect();
            Arc::new(BooleanArray::from(v)) as ArrayRef
        }
        Arrow57DataType::Int8 => {
            let v: Vec<Option<i8>> = (0..n)
                .map(|r| cell_as_i64(&cell(r)).and_then(|i| i8::try_from(i).ok()))
                .collect();
            Arc::new(Int8Array::from(v)) as ArrayRef
        }
        Arrow57DataType::Int16 => {
            let v: Vec<Option<i16>> = (0..n)
                .map(|r| cell_as_i64(&cell(r)).and_then(|i| i16::try_from(i).ok()))
                .collect();
            Arc::new(Int16Array::from(v)) as ArrayRef
        }
        Arrow57DataType::Int32 => {
            let v: Vec<Option<i32>> = (0..n)
                .map(|r| cell_as_i64(&cell(r)).and_then(|i| i32::try_from(i).ok()))
                .collect();
            Arc::new(Int32Array::from(v)) as ArrayRef
        }
        Arrow57DataType::Int64 => {
            let v: Vec<Option<i64>> = (0..n).map(|r| cell_as_i64(&cell(r))).collect();
            Arc::new(Int64Array::from(v)) as ArrayRef
        }
        Arrow57DataType::Float64 => {
            let v: Vec<Option<f64>> = (0..n).map(|r| cell_as_f64(&cell(r))).collect();
            Arc::new(Float64Array::from(v)) as ArrayRef
        }
        Arrow57DataType::Date32 => {
            let v: Vec<Option<i32>> = (0..n)
                .map(|r| match cell(r) {
                    CellValue::Date(s) | CellValue::String(s) | CellValue::DateTime(s) => {
                        date_string_to_days(&s)
                    }
                    _ => None,
                })
                .collect();
            Arc::new(Date32Array::from(v)) as ArrayRef
        }
        Arrow57DataType::Timestamp(Arrow57TimeUnit::Microsecond, _) => {
            let v: Vec<Option<i64>> = (0..n)
                .map(|r| match cell(r) {
                    CellValue::DateTime(s) | CellValue::String(s) => datetime_string_to_micros(&s),
                    CellValue::Date(s) => {
                        date_string_to_days(&s).map(|d| (d as i64) * 86_400_000_000)
                    }
                    _ => None,
                })
                .collect();
            Arc::new(TimestampMicrosecondArray::from(v)) as ArrayRef
        }
        _ => {
            let v: Vec<Option<String>> = (0..n)
                .map(|r| match cell(r) {
                    CellValue::Null => None,
                    other => Some(other.to_string()),
                })
                .collect();
            Arc::new(StringArray::from(v)) as ArrayRef
        }
    }
}

fn write_sav(path: &Path, table: &DataTable) -> Result<()> {
    let mut working = table.clone();
    working.apply_edits();

    let fields: Vec<Arrow57Field> = working
        .columns
        .iter()
        .map(|c| Arrow57Field::new(c.name.clone(), arrow57_type_for_column(&c.data_type), true))
        .collect();
    let schema = Arc::new(Arrow57Schema::new(fields));

    let arrays: Vec<ArrayRef> = working
        .columns
        .iter()
        .enumerate()
        .map(|(i, _)| build_array(&working, i, schema.field(i).data_type()))
        .collect();

    let batch = RecordBatch::try_new(schema.clone(), arrays)
        .with_context(|| "building SPSS RecordBatch")?;
    let metadata = ambers::metadata::SpssMetadata::from_arrow_schema(schema.as_ref());

    let compression = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .as_deref()
    {
        Some("zsav") => ambers::Compression::Zlib,
        // Bytecode is the default SPSS compression for `.sav`; matches what
        // `read_sav` accepts and produces smaller files than `Compression::None`.
        _ => ambers::Compression::Bytecode,
    };

    ambers::write_sav(path, &batch, &metadata, compression, None)
        .with_context(|| format!("writing SPSS file {}", path.display()))?;
    Ok(())
}

fn arrow57_type_string(dt: &Arrow57DataType) -> &'static str {
    match dt {
        Arrow57DataType::Boolean => "Boolean",
        Arrow57DataType::Int8 => "Int8",
        Arrow57DataType::Int16 => "Int16",
        Arrow57DataType::Int32 => "Int32",
        Arrow57DataType::Int64 => "Int64",
        Arrow57DataType::Float32 => "Float32",
        Arrow57DataType::Float64 => "Float64",
        Arrow57DataType::Utf8 | Arrow57DataType::Utf8View | Arrow57DataType::LargeUtf8 => "Utf8",
        Arrow57DataType::Date32 | Arrow57DataType::Date64 => "Date",
        Arrow57DataType::Timestamp(_, _) => "DateTime",
        Arrow57DataType::Duration(_) => "Utf8",
        _ => "Utf8",
    }
}

fn arrow57_value_to_cell(array: &dyn Array, row: usize) -> CellValue {
    if array.is_null(row) {
        return CellValue::Null;
    }
    match array.data_type() {
        Arrow57DataType::Float64 => {
            let a = array.as_any().downcast_ref::<Float64Array>().unwrap();
            CellValue::Float(a.value(row))
        }
        Arrow57DataType::Utf8 => {
            let a = array.as_any().downcast_ref::<StringArray>().unwrap();
            CellValue::String(a.value(row).trim_end().to_string())
        }
        Arrow57DataType::Utf8View => {
            let a = array.as_any().downcast_ref::<StringViewArray>().unwrap();
            CellValue::String(a.value(row).trim_end().to_string())
        }
        Arrow57DataType::BinaryView => {
            let a = array.as_any().downcast_ref::<BinaryViewArray>().unwrap();
            CellValue::Binary(a.value(row).to_vec())
        }
        Arrow57DataType::Date32 => {
            let a = array.as_any().downcast_ref::<Date32Array>().unwrap();
            let days = a.value(row);
            chrono::NaiveDate::from_num_days_from_ce_opt(days + 719_163)
                .map(|d| CellValue::Date(d.format("%Y-%m-%d").to_string()))
                .unwrap_or(CellValue::Null)
        }
        Arrow57DataType::Timestamp(unit, _) => {
            let us = match unit {
                Arrow57TimeUnit::Microsecond => array
                    .as_any()
                    .downcast_ref::<TimestampMicrosecondArray>()
                    .unwrap()
                    .value(row),
                _ => return CellValue::String(format!("ts({:?})", unit)),
            };
            let secs = us.div_euclid(1_000_000);
            let nsecs = (us.rem_euclid(1_000_000) * 1_000) as u32;
            chrono::DateTime::from_timestamp(secs, nsecs)
                .map(|dt| CellValue::DateTime(dt.format("%Y-%m-%d %H:%M:%S%.6f").to_string()))
                .unwrap_or(CellValue::Null)
        }
        Arrow57DataType::Duration(unit) => match unit {
            Arrow57TimeUnit::Microsecond => {
                let a = array
                    .as_any()
                    .downcast_ref::<DurationMicrosecondArray>()
                    .unwrap();
                let us = a.value(row);
                let total_secs = us.div_euclid(1_000_000);
                let frac_us = us.rem_euclid(1_000_000);
                let h = total_secs / 3600;
                let m = (total_secs % 3600) / 60;
                let s = total_secs % 60;
                if frac_us == 0 {
                    CellValue::String(format!("{h:02}:{m:02}:{s:02}"))
                } else {
                    CellValue::String(format!("{h:02}:{m:02}:{s:02}.{:06}", frac_us))
                }
            }
            _ => CellValue::String(format!("dur({:?})", unit)),
        },
        _ => CellValue::String(format!("{:?}", array.data_type())),
    }
}
