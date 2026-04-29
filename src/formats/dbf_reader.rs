use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::{Context, Result, anyhow};
use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use dbase::{
    FieldInfo, FieldName, FieldType, FieldValue, Reader, TableWriter, TableWriterBuilder,
    WritableRecord,
};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::path::Path;

pub struct DbfReader;

impl FormatReader for DbfReader {
    fn name(&self) -> &str {
        "DBF"
    }

    fn extensions(&self) -> &[&str] {
        &["dbf"]
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let mut reader = Reader::from_path(path)
            .with_context(|| format!("opening DBF file {}", path.display()))?;

        let columns: Vec<ColumnInfo> = reader
            .fields()
            .iter()
            .map(|info| ColumnInfo {
                name: info.name().to_string(),
                data_type: dbf_type_string(info).to_string(),
            })
            .collect();

        let mut rows: Vec<Vec<CellValue>> = Vec::new();
        for record_result in reader.iter_records() {
            let record = record_result.with_context(|| "reading DBF record")?;
            let mut row: Vec<CellValue> = Vec::with_capacity(columns.len());
            for col in &columns {
                let value = record
                    .get(&col.name)
                    .map(field_value_to_cell)
                    .unwrap_or(CellValue::Null);
                row.push(value);
            }
            rows.push(row);
        }

        Ok(DataTable {
            columns,
            rows,
            edits: HashMap::new(),
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("DBF".to_string()),
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        })
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        write_dbf(path, table)
    }
}

fn dbf_type_string(info: &FieldInfo) -> &'static str {
    match info.field_type() {
        FieldType::Logical => "Boolean",
        FieldType::Integer => "Int32",
        // dBASE's Numeric stores both integer and decimal values as ASCII; we
        // treat them as Float64 since the decimal-places metadata isn't on the
        // public FieldInfo API in dbase 0.7.
        FieldType::Numeric | FieldType::Float | FieldType::Currency | FieldType::Double => {
            "Float64"
        }
        FieldType::Date => "Date",
        FieldType::DateTime => "DateTime",
        FieldType::Character | FieldType::Memo => "Utf8",
    }
}

fn field_value_to_cell(value: &FieldValue) -> CellValue {
    match value {
        FieldValue::Character(Some(s)) => CellValue::String(s.trim_end().to_string()),
        FieldValue::Character(None) => CellValue::Null,
        FieldValue::Numeric(Some(n)) => CellValue::Float(*n),
        FieldValue::Numeric(None) => CellValue::Null,
        FieldValue::Logical(Some(b)) => CellValue::Bool(*b),
        FieldValue::Logical(None) => CellValue::Null,
        FieldValue::Date(Some(d)) => {
            CellValue::Date(format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()))
        }
        FieldValue::Date(None) => CellValue::Null,
        FieldValue::Float(Some(f)) => CellValue::Float(f64::from(*f)),
        FieldValue::Float(None) => CellValue::Null,
        FieldValue::Integer(i) => CellValue::Int(i64::from(*i)),
        FieldValue::Currency(c) => CellValue::Float(*c),
        FieldValue::DateTime(dt) => {
            let d = dt.date();
            let t = dt.time();
            CellValue::DateTime(format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                d.year(),
                d.month(),
                d.day(),
                t.hours(),
                t.minutes(),
                t.seconds()
            ))
        }
        FieldValue::Double(d) => CellValue::Float(*d),
        FieldValue::Memo(s) => CellValue::String(s.clone()),
    }
}

/// One row pre-converted into `(name, FieldValue)` pairs in declaration order.
/// Implements `WritableRecord` so we can hand it to the dbase TableWriter.
struct WriteRow {
    fields: Vec<FieldValue>,
}

impl WritableRecord for WriteRow {
    fn write_using<W: std::io::Write>(
        &self,
        field_writer: &mut dbase::FieldWriter<'_, W>,
    ) -> Result<(), dbase::FieldIOError> {
        let mut idx = 0;
        while field_writer.next_field_name().is_some() {
            field_writer.write_next_field_value(&self.fields[idx])?;
            idx += 1;
        }
        Ok(())
    }
}

fn write_dbf(path: &Path, table: &DataTable) -> Result<()> {
    let writer = build_writer_for(path, &table.columns)?;
    write_rows(writer, table)
}

fn build_writer_for(
    path: &Path,
    columns: &[ColumnInfo],
) -> Result<TableWriter<std::io::BufWriter<std::fs::File>>> {
    let mut builder = TableWriterBuilder::new();
    for col in columns {
        let field_name = FieldName::try_from(col.name.as_str())
            .map_err(|e| anyhow!("invalid DBF field name '{}': {:?}", col.name, e))?;
        builder = match col.data_type.as_str() {
            "Boolean" => builder.add_logical_field(field_name),
            "Int8" | "Int16" | "Int32" | "UInt8" | "UInt16" | "UInt32" => {
                builder.add_integer_field(field_name)
            }
            // i64/u64 don't fit in DBF Integer (i32) — store as wide Numeric.
            "Int64" | "UInt64" => builder.add_numeric_field(field_name, 20, 0),
            "Float32" | "Float64" => builder.add_numeric_field(field_name, 20, 8),
            "Date" => builder.add_date_field(field_name),
            "DateTime" => builder.add_datetime_field(field_name),
            "Binary" => {
                return Err(anyhow!(
                    "DBF cannot store Binary columns (column '{}')",
                    col.name
                ));
            }
            // Utf8, Nested, anything else — write as character string.
            // 254 is the max single-field length in dBase (length is u8).
            _ => builder.add_character_field(field_name, 254),
        };
    }
    builder
        .build_with_file_dest(path)
        .with_context(|| format!("creating DBF file {}", path.display()))
}

fn write_rows(
    mut writer: TableWriter<std::io::BufWriter<std::fs::File>>,
    table: &DataTable,
) -> Result<()> {
    for (row_idx, row) in table.rows.iter().enumerate() {
        let mut fields: Vec<FieldValue> = Vec::with_capacity(table.columns.len());
        for (col_idx, col) in table.columns.iter().enumerate() {
            let cell = table
                .edits
                .get(&(row_idx, col_idx))
                .or_else(|| row.get(col_idx))
                .cloned()
                .unwrap_or(CellValue::Null);
            fields.push(cell_to_field_value(&cell, &col.data_type));
        }
        writer
            .write_record(&WriteRow { fields })
            .map_err(|e| anyhow!("writing DBF row {}: {}", row_idx, e))?;
    }
    Ok(())
}

fn cell_to_field_value(cell: &CellValue, data_type: &str) -> FieldValue {
    let is_int_field = matches!(
        data_type,
        "Int8" | "Int16" | "Int32" | "UInt8" | "UInt16" | "UInt32"
    );
    match cell {
        CellValue::Null => match data_type {
            "Boolean" => FieldValue::Logical(None),
            "Date" => FieldValue::Date(None),
            "DateTime" => FieldValue::DateTime(default_datetime()),
            t if int_field(t) => FieldValue::Integer(0),
            "Int64" | "UInt64" | "Float32" | "Float64" => FieldValue::Numeric(None),
            _ => FieldValue::Character(None),
        },
        CellValue::Bool(b) => FieldValue::Logical(Some(*b)),
        CellValue::Int(i) if is_int_field => {
            FieldValue::Integer((*i).clamp(i32::MIN as i64, i32::MAX as i64) as i32)
        }
        CellValue::Int(i) => FieldValue::Numeric(Some(*i as f64)),
        CellValue::Float(f) if is_int_field => {
            FieldValue::Integer(f.clamp(i32::MIN as f64, i32::MAX as f64) as i32)
        }
        CellValue::Float(f) => FieldValue::Numeric(Some(*f)),
        CellValue::String(s) => FieldValue::Character(Some(s.clone())),
        CellValue::Date(s) => parse_date_field(s),
        CellValue::DateTime(s) => parse_datetime_field(s),
        CellValue::Binary(_) => FieldValue::Character(None),
        CellValue::Nested(s) => FieldValue::Character(Some(s.clone())),
    }
}

fn int_field(t: &str) -> bool {
    matches!(
        t,
        "Int8" | "Int16" | "Int32" | "UInt8" | "UInt16" | "UInt32"
    )
}

fn parse_date_field(s: &str) -> FieldValue {
    let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") else {
        return FieldValue::Date(None);
    };
    match make_dbase_date(&d) {
        Some(date) => FieldValue::Date(Some(date)),
        None => FieldValue::Date(None),
    }
}

fn parse_datetime_field(s: &str) -> FieldValue {
    let parsed = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"));
    let Ok(ndt) = parsed else {
        return FieldValue::DateTime(default_datetime());
    };
    let Some(date) = make_dbase_date(&ndt.date()) else {
        return FieldValue::DateTime(default_datetime());
    };
    let Some(time) = make_dbase_time(ndt.hour(), ndt.minute(), ndt.second()) else {
        return FieldValue::DateTime(default_datetime());
    };
    FieldValue::DateTime(dbase::DateTime::new(date, time))
}

/// `dbase::Date::new` panics on out-of-range, so we validate first.
fn make_dbase_date(d: &NaiveDate) -> Option<dbase::Date> {
    let year = d.year();
    let month = d.month();
    let day = d.day();
    if !(0..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(dbase::Date::new(day, month, year as u32))
}

fn make_dbase_time(hour: u32, minute: u32, second: u32) -> Option<dbase::Time> {
    if hour > 24 || minute > 60 || second > 60 {
        return None;
    }
    Some(dbase::Time::new(hour, minute, second))
}

fn default_datetime() -> dbase::DateTime {
    let date = dbase::Date::new(1, 1, 1970);
    let time = dbase::Time::new(0, 0, 0);
    dbase::DateTime::new(date, time)
}
