use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::{Context, Result};
use sas7bdat::{CellValue as SasCell, ColumnMeta, Dataset, LogicalType};
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
        let dataset = Dataset::open(path)
            .map_err(sas_err)
            .with_context(|| format!("opening SAS file {}", path.display()))?;

        let columns: Vec<ColumnInfo> = dataset
            .columns()
            .iter()
            .map(|c| ColumnInfo {
                name: column_name(c),
                data_type: logical_type_name(c.logical_type).to_string(),
            })
            .collect();

        let mut rows: Vec<Vec<CellValue>> = Vec::new();
        dataset
            .visit_rows(|row| {
                rows.push(row.iter().map(sas_cell_to_octa).collect());
                Ok(std::ops::ControlFlow::Continue(()))
            })
            .map_err(sas_err)
            .with_context(|| format!("reading rows from SAS file {}", path.display()))?;

        let mut table = DataTable {
            columns,
            rows,
            source_path: Some(path.to_string_lossy().to_string()),
            format_name: Some("SAS".to_string()),
            ..DataTable::empty()
        };
        narrow_time_columns(&mut table);
        Ok(table)
    }
}

/// `sas7bdat::Error` implements `Display` but not `std::error::Error`, so it
/// cannot cross an `anyhow` boundary on its own. Flatten it to a message here;
/// the caller adds the `.context()` naming the file.
fn sas_err(e: sas7bdat::Error) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

fn column_name(c: &ColumnMeta) -> String {
    let trimmed = c.name.trim_end();
    if trimmed.is_empty() {
        format!("col_{}", c.index + 1)
    } else {
        trimmed.to_string()
    }
}

/// The reader used to classify date columns itself by matching SAS format-name
/// prefixes (DATETIME/YYMMDD/MMDDYY/...). `LogicalType` now carries that
/// decision, made from the format *and* the column's internal flags, so the
/// hand-rolled prefix list is gone along with the formats it silently missed.
fn logical_type_name(t: LogicalType) -> &'static str {
    match t {
        LogicalType::Integer => "Int64",
        LogicalType::Float => "Float64",
        LogicalType::String => "Utf8",
        LogicalType::Date => "Date",
        LogicalType::DateTime => "DateTime",
        // Read as seconds; `narrow_time_columns` turns a column that stays
        // within one day into a Time column.
        LogicalType::Time => "Duration(s)",
        LogicalType::Bytes => "Binary",
    }
}

fn sas_cell_to_octa(value: &SasCell<'_>) -> CellValue {
    match value {
        SasCell::Null => CellValue::Null,
        SasCell::Int32(i) => CellValue::Int(i64::from(*i)),
        SasCell::Int64(i) => CellValue::Int(*i),
        SasCell::Float64(f) => CellValue::Float(*f),
        SasCell::Str(s) => CellValue::String(s.trim_end().to_string()),
        SasCell::Bytes(b) => CellValue::Binary(b.to_vec()),
        // `unix_days` / `unix_seconds` do the 1960 -> 1970 epoch shift for us
        // (SasDate::DAYS_SAS_TO_UNIX = 3653, SasDateTime::SECONDS_SAS_TO_UNIX
        // = 315_619_200), so chrono only has to format an ordinary timestamp.
        SasCell::Date(d) => CellValue::Date(format_unix_secs(
            i64::from(d.unix_days()) * 86_400,
            "%Y-%m-%d",
        )),
        SasCell::DateTime(dt) => {
            CellValue::DateTime(format_unix_secs(dt.unix_seconds(), "%Y-%m-%d %H:%M:%S"))
        }
        SasCell::Time(t) => CellValue::Duration(
            i64::from(t.seconds_since_midnight) * crate::data::times::NANOS_PER_SEC,
        ),
    }
}

/// A SAS TIME column is a clock reading unless a value runs past a day (or
/// below zero), which makes it a duration.
fn narrow_time_columns(table: &mut DataTable) {
    for col in 0..table.columns.len() {
        if crate::data::times::TimeKind::of_type(&table.columns[col].data_type)
            == Some(crate::data::times::TimeKind::Duration)
        {
            crate::data::times::narrow_to_time(table, col);
        }
    }
}

fn format_unix_secs(secs: i64, fmt: &str) -> String {
    // chrono is the project's standard formatter.
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|cd| cd.format(fmt).to_string())
        .unwrap_or_else(|| secs.to_string())
}

#[cfg(test)]
#[path = "sas_reader_tests.rs"]
mod tests;
