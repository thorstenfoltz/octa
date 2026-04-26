use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::{Context, Result};
use dta::stata::dta::byte_order::ByteOrder;
use dta::stata::dta::dta_reader::DtaReader;
use dta::stata::dta::dta_writer::DtaWriter;
use dta::stata::dta::header::Header;
use dta::stata::dta::long_string_ref::LongStringRef;
use dta::stata::dta::long_string_table::LongStringTable;
use dta::stata::dta::release::Release;
use dta::stata::dta::schema::Schema;
use dta::stata::dta::value::Value as DtaValue;
use dta::stata::dta::variable::Variable;
use dta::stata::dta::variable_type::VariableType;
use dta::stata::missing_value::MissingValue;
use dta::stata::stata_byte::StataByte;
use dta::stata::stata_double::StataDouble;
use dta::stata::stata_float::StataFloat;
use dta::stata::stata_int::StataInt;
use dta::stata::stata_long::StataLong;
use std::path::Path;

pub struct StataReader;

impl FormatReader for StataReader {
    fn name(&self) -> &str {
        "Stata"
    }

    fn extensions(&self) -> &[&str] {
        &["dta"]
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        write_stata(path, table)
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let header_reader = DtaReader::new()
            .from_path(path)
            .with_context(|| format!("opening Stata file {}", path.display()))?;
        let schema_reader = header_reader.read_header()?;
        let mut characteristic_reader = schema_reader.read_schema()?;
        characteristic_reader.skip_to_end()?;

        let columns: Vec<ColumnInfo> = characteristic_reader
            .schema()
            .variables()
            .iter()
            .map(|v| ColumnInfo {
                name: v.name().to_string(),
                data_type: variable_type_string(v.variable_type()).to_string(),
            })
            .collect();

        let mut record_reader = characteristic_reader.into_record_reader()?;

        // First pass: collect values, but for `LongStringRef` we keep the
        // ref and resolve it after we walk the strL section below.
        #[derive(Clone)]
        enum Pending {
            Resolved(CellValue),
            LongRef(LongStringRef),
        }

        let mut pending_rows: Vec<Vec<Pending>> = Vec::new();
        while let Some(record) = record_reader.read_record()? {
            let mut row: Vec<Pending> = Vec::with_capacity(record.values().len());
            for value in record.values() {
                row.push(match value {
                    DtaValue::LongStringRef(r) => Pending::LongRef(*r),
                    other => Pending::Resolved(dta_value_to_cell(other)),
                });
            }
            pending_rows.push(row);
        }

        // strL pass: only present for v117+. read_remaining_into is a no-op
        // on older releases.
        let mut long_string_reader = record_reader.into_long_string_reader()?;
        let encoding = long_string_reader.encoding();
        let mut long_strings = LongStringTable::for_reading();
        long_string_reader.read_remaining_into(&mut long_strings)?;

        let rows: Vec<Vec<CellValue>> = pending_rows
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|cell| match cell {
                        Pending::Resolved(v) => v,
                        Pending::LongRef(key) => match long_strings.get(&key) {
                            Some(ls) => match ls.data_str(encoding) {
                                Some(s) => CellValue::String(s.into_owned()),
                                None => CellValue::Binary(ls.data().to_vec()),
                            },
                            None => CellValue::Null,
                        },
                    })
                    .collect()
            })
            .collect();

        Ok(DataTable {
            columns,
            rows,
            edits: std::collections::HashMap::new(),
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("Stata".to_string()),
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

fn variable_type_string(t: VariableType) -> &'static str {
    match t {
        VariableType::Byte => "Int8",
        VariableType::Int => "Int16",
        VariableType::Long => "Int32",
        VariableType::Float => "Float32",
        VariableType::Double => "Float64",
        VariableType::FixedString(_) | VariableType::LongString => "Utf8",
    }
}

/// Map an octa column data_type string to the Stata `VariableType` we
/// will write. For string-shaped columns (`Utf8`, `Date`, `DateTime`,
/// or any unknown type) the width is the maximum byte length found in
/// the column, clamped into `1..=2045` (the maximum FixedString width
/// the dta crate accepts; LongString would push us into v117+ strL
/// territory and is not currently supported here).
fn variable_type_for_column(table: &DataTable, col_idx: usize, data_type: &str) -> VariableType {
    match data_type {
        "Boolean" | "Int8" => VariableType::Byte,
        "Int16" => VariableType::Int,
        "Int32" => VariableType::Long,
        // Stata `Long` is i32; keep i64 columns intact by widening to Double.
        "Int64" => VariableType::Double,
        "Float32" => VariableType::Float,
        "Float64" => VariableType::Double,
        _ => {
            let mut max_len = 0usize;
            for r in 0..table.row_count() {
                let s = match table.get(r, col_idx) {
                    Some(CellValue::Null) | None => continue,
                    Some(CellValue::Binary(b)) => b.len(),
                    Some(other) => other.to_string().len(),
                };
                if s > max_len {
                    max_len = s;
                }
            }
            let width = max_len.clamp(1, 2045) as u16;
            VariableType::FixedString(width)
        }
    }
}

fn default_format_for(vt: VariableType) -> String {
    match vt {
        VariableType::Byte => "%8.0g".to_string(),
        VariableType::Int => "%8.0g".to_string(),
        VariableType::Long => "%12.0g".to_string(),
        VariableType::Float => "%9.0g".to_string(),
        VariableType::Double => "%10.0g".to_string(),
        VariableType::FixedString(n) => format!("%{n}s"),
        VariableType::LongString => "%9s".to_string(),
    }
}

/// Serialize a CellValue for a `FixedString` column. Non-string cells
/// (Int, Float, Date, etc.) are stringified via `Display` so the
/// written file at least preserves the visible text.
fn cell_to_stata_string(cell: &CellValue) -> String {
    match cell {
        CellValue::Null => String::new(),
        CellValue::String(s)
        | CellValue::Date(s)
        | CellValue::DateTime(s)
        | CellValue::Nested(s) => s.clone(),
        CellValue::Binary(b) => String::from_utf8_lossy(b).into_owned(),
        other => other.to_string(),
    }
}

fn cell_to_dta_value<'a>(cell: &CellValue, vt: VariableType, string_buf: &'a str) -> DtaValue<'a> {
    let to_float = |c: &CellValue| -> Option<f64> {
        match c {
            CellValue::Float(f) => Some(*f),
            CellValue::Int(i) => Some(*i as f64),
            CellValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            CellValue::String(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    };
    let int_value: Option<i64> = match cell {
        CellValue::Int(i) => Some(*i),
        CellValue::Bool(b) => Some(if *b { 1 } else { 0 }),
        CellValue::Float(f) if f.is_finite() && f.fract() == 0.0 => Some(*f as i64),
        _ => None,
    };

    match vt {
        VariableType::Byte => match int_value.and_then(|v| i8::try_from(v).ok()) {
            Some(v) => DtaValue::Byte(StataByte::Present(v)),
            None => DtaValue::Byte(StataByte::Missing(MissingValue::System)),
        },
        VariableType::Int => match int_value.and_then(|v| i16::try_from(v).ok()) {
            Some(v) => DtaValue::Int(StataInt::Present(v)),
            None => DtaValue::Int(StataInt::Missing(MissingValue::System)),
        },
        VariableType::Long => match int_value.and_then(|v| i32::try_from(v).ok()) {
            Some(v) => DtaValue::Long(StataLong::Present(v)),
            None => DtaValue::Long(StataLong::Missing(MissingValue::System)),
        },
        VariableType::Float => match to_float(cell) {
            Some(v) if v.is_finite() => DtaValue::Float(StataFloat::Present(v as f32)),
            _ => DtaValue::Float(StataFloat::Missing(MissingValue::System)),
        },
        VariableType::Double => match to_float(cell) {
            Some(v) if v.is_finite() => DtaValue::Double(StataDouble::Present(v)),
            _ => DtaValue::Double(StataDouble::Missing(MissingValue::System)),
        },
        VariableType::FixedString(width) => {
            // Truncate to byte width without splitting a UTF-8 boundary.
            let max = usize::from(width);
            let mut end = string_buf.len().min(max);
            while end > 0 && !string_buf.is_char_boundary(end) {
                end -= 1;
            }
            DtaValue::string(&string_buf[..end])
        }
        VariableType::LongString => DtaValue::string(string_buf),
    }
}

fn write_stata(path: &Path, table: &DataTable) -> Result<()> {
    let mut working = table.clone();
    working.apply_edits();

    // Resolve types up front so the row loop is just allocation + writes.
    let var_types: Vec<VariableType> = working
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| variable_type_for_column(&working, i, &c.data_type))
        .collect();

    let mut schema_builder = Schema::builder();
    for (col, vt) in working.columns.iter().zip(var_types.iter().copied()) {
        let fmt = default_format_for(vt);
        schema_builder = schema_builder.add_variable(Variable::builder(vt, &col.name).format(fmt));
    }
    let schema = schema_builder
        .build()
        .with_context(|| "building Stata schema")?;

    let header = Header::builder(Release::V118, ByteOrder::LittleEndian).build();

    let mut record_writer = DtaWriter::new()
        .from_path(path)
        .with_context(|| format!("opening Stata file {} for write", path.display()))?
        .write_header(header)
        .with_context(|| "writing Stata header")?
        .write_schema(schema)
        .with_context(|| "writing Stata schema")?
        .into_record_writer()
        .with_context(|| "starting Stata record writer")?;

    for r in 0..working.row_count() {
        // Pre-build owned strings so `Value::string(&s)` references stay valid
        // until `write_record` returns.
        let str_buffers: Vec<String> = (0..var_types.len())
            .map(|c| match var_types[c] {
                VariableType::FixedString(_) | VariableType::LongString => {
                    let cell = working.get(r, c).cloned().unwrap_or(CellValue::Null);
                    cell_to_stata_string(&cell)
                }
                _ => String::new(),
            })
            .collect();

        let values: Vec<DtaValue> = (0..var_types.len())
            .map(|c| {
                let cell = working.get(r, c).cloned().unwrap_or(CellValue::Null);
                cell_to_dta_value(&cell, var_types[c], &str_buffers[c])
            })
            .collect();

        record_writer
            .write_record(&values)
            .with_context(|| format!("writing Stata row {}", r))?;
    }

    record_writer
        .into_long_string_writer()
        .with_context(|| "transitioning to Stata long-string writer")?
        .into_value_label_writer()
        .with_context(|| "transitioning to Stata value-label writer")?
        .finish()
        .with_context(|| "finalizing Stata file")?;

    Ok(())
}

fn dta_value_to_cell(value: &DtaValue<'_>) -> CellValue {
    match value {
        DtaValue::Byte(v) => v
            .present()
            .map(|n| CellValue::Int(i64::from(n)))
            .unwrap_or(CellValue::Null),
        DtaValue::Int(v) => v
            .present()
            .map(|n| CellValue::Int(i64::from(n)))
            .unwrap_or(CellValue::Null),
        DtaValue::Long(v) => v
            .present()
            .map(|n| CellValue::Int(i64::from(n)))
            .unwrap_or(CellValue::Null),
        DtaValue::Float(v) => v
            .present()
            .map(|n| CellValue::Float(f64::from(n)))
            .unwrap_or(CellValue::Null),
        DtaValue::Double(v) => v.present().map(CellValue::Float).unwrap_or(CellValue::Null),
        DtaValue::String(s) => CellValue::String(s.as_ref().to_string()),
        // Resolved by the caller — should never reach here.
        DtaValue::LongStringRef(_) => CellValue::Null,
    }
}
