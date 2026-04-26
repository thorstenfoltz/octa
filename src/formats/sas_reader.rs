use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::{Context, Result};
use sas7bdat::{
    OffsetDateTime, SasReader,
    cell::CellValue as SasCell,
    dataset::{Variable, VariableKind},
};
use std::path::Path;

pub struct SasFormatReader;

impl FormatReader for SasFormatReader {
    fn name(&self) -> &str {
        "SAS"
    }

    fn extensions(&self) -> &[&str] {
        // sas7bcat is the companion catalog (label) file format; it can be
        // opened in Octa as a sanity check but it has no data rows on its own.
        &["sas7bdat"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let mut reader = SasReader::open(path)
            .with_context(|| format!("opening SAS file {}", path.display()))?;

        let columns: Vec<ColumnInfo> = reader
            .metadata()
            .variables
            .iter()
            .map(|v| ColumnInfo {
                name: variable_name(v),
                data_type: variable_type(v).to_string(),
            })
            .collect();

        let mut rows: Vec<Vec<CellValue>> = Vec::new();
        for row in reader.rows()? {
            let row = row?;
            let cells: Vec<CellValue> = row.iter().map(sas_cell_to_octa).collect();
            rows.push(cells);
        }

        Ok(DataTable {
            columns,
            rows,
            edits: std::collections::HashMap::new(),
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("SAS".to_string()),
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

fn variable_name(v: &Variable) -> String {
    let trimmed = v.name.trim_end();
    if trimmed.is_empty() {
        format!("col_{}", v.index + 1)
    } else {
        trimmed.to_string()
    }
}

fn variable_type(v: &Variable) -> &'static str {
    match v.kind {
        VariableKind::Character => "Utf8",
        VariableKind::Numeric => {
            // SAS numerics are 8-byte doubles; we keep them as Float64 unless
            // a date/datetime format is declared.
            if let Some(fmt) = &v.format {
                let name = fmt.name.to_ascii_uppercase();
                if name.starts_with("DATETIME") || name.starts_with("E8601DT") {
                    return "DateTime";
                }
                if name.starts_with("DATE")
                    || name.starts_with("YYMMDD")
                    || name.starts_with("MMDDYY")
                    || name.starts_with("DDMMYY")
                    || name.starts_with("E8601DA")
                {
                    return "Date";
                }
            }
            "Float64"
        }
    }
}

fn sas_cell_to_octa(value: &SasCell<'_>) -> CellValue {
    match value {
        SasCell::Float(f) => CellValue::Float(*f),
        SasCell::Int32(i) => CellValue::Int(i64::from(*i)),
        SasCell::Int64(i) => CellValue::Int(*i),
        SasCell::NumericString(s) => CellValue::String(s.as_ref().to_string()),
        SasCell::Str(s) => {
            let trimmed = s.trim_end();
            CellValue::String(trimmed.to_string())
        }
        SasCell::Bytes(b) => CellValue::Binary(b.as_ref().to_vec()),
        SasCell::DateTime(dt) => {
            CellValue::DateTime(format_offset_date_time(*dt, "%Y-%m-%d %H:%M:%S"))
        }
        SasCell::Date(d) => CellValue::Date(format_offset_date_time(*d, "%Y-%m-%d")),
        SasCell::Time(dur) => {
            // Render as HH:MM:SS since midnight; spill negative or >24h into a string.
            let total_secs = dur.whole_seconds();
            if (0..86_400).contains(&total_secs) {
                let h = total_secs / 3600;
                let m = (total_secs % 3600) / 60;
                let s = total_secs % 60;
                CellValue::String(format!("{h:02}:{m:02}:{s:02}"))
            } else {
                CellValue::String(format!("{total_secs}s"))
            }
        }
        SasCell::Missing(_) => CellValue::Null,
    }
}

fn format_offset_date_time(dt: OffsetDateTime, fmt: &str) -> String {
    // chrono is the project's standard formatter; convert via Unix timestamp.
    let secs = dt.unix_timestamp();
    let nsecs = dt.nanosecond();
    chrono::DateTime::from_timestamp(secs, nsecs)
        .map(|cd| cd.format(fmt).to_string())
        .unwrap_or_else(|| dt.to_string())
}
