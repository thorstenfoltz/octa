use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use anyhow::Result;
use std::path::Path;

pub struct CsvReader;

impl FormatReader for CsvReader {
    fn name(&self) -> &str {
        "CSV"
    }

    fn extensions(&self) -> &[&str] {
        &["csv"]
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let delimiter = detect_delimiter(path).unwrap_or(b',');
        read_delimited(path, delimiter, "CSV")
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        write_delimited(path, b',', table)
    }
}

pub struct TsvReader;

impl FormatReader for TsvReader {
    fn name(&self) -> &str {
        "TSV"
    }

    fn extensions(&self) -> &[&str] {
        &["tsv", "tab"]
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        read_delimited(path, b'\t', "TSV")
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        write_delimited(path, b'\t', table)
    }
}

/// Auto-detect the delimiter used in a CSV file by checking consistency of
/// candidate delimiters across the first few lines.
pub fn detect_delimiter(path: &Path) -> Option<u8> {
    let content = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = content.lines().take(20).collect();
    if lines.is_empty() {
        return None;
    }

    let candidates: &[u8] = b",;|\t";
    let mut best: Option<(u8, usize)> = None; // (delimiter, count_per_line)

    for &delim in candidates {
        let delim_char = delim as char;
        let counts: Vec<usize> = lines
            .iter()
            .map(|l| l.matches(delim_char).count())
            .collect();

        // Skip if header has zero occurrences
        if counts[0] == 0 {
            continue;
        }

        // Check consistency: all lines should have roughly the same count
        let header_count = counts[0];
        let consistent = counts.iter().all(|&c| c == header_count || c == 0);

        if consistent && (best.is_none() || header_count > best.unwrap().1) {
            best = Some((delim, header_count));
        }
    }

    best.map(|(d, _)| d)
}

const MAX_ROWS: usize = 1_000_000;

pub fn infer_cell_value(s: &str) -> CellValue {
    if s.is_empty() {
        return CellValue::Null;
    }
    match s.to_lowercase().as_str() {
        "true" => return CellValue::Bool(true),
        "false" => return CellValue::Bool(false),
        _ => {}
    }
    if let Ok(i) = s.parse::<i64>() {
        return CellValue::Int(i);
    }
    if let Ok(f) = s.parse::<f64>() {
        return CellValue::Float(f);
    }
    if chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok() {
        return CellValue::Date(s.to_string());
    }
    if chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").is_ok()
        || chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").is_ok()
        || chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f").is_ok()
        || chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").is_ok()
    {
        return CellValue::DateTime(s.to_string());
    }
    // Timezone-aware timestamps (RFC3339, ISO8601 with offset)
    if chrono::DateTime::parse_from_rfc3339(s).is_ok()
        || chrono::DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%z").is_ok()
        || chrono::DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f%z").is_ok()
        || chrono::DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%z").is_ok()
        || chrono::DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f%z").is_ok()
    {
        return CellValue::DateTime(s.to_string());
    }
    CellValue::String(s.to_string())
}

fn read_delimited(path: &Path, delimiter: u8, format_name: &str) -> Result<DataTable> {
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_path(path)?;

    let headers: Vec<String> = rdr.headers()?.iter().map(|h| h.to_string()).collect();
    let columns: Vec<ColumnInfo> = headers
        .iter()
        .map(|h| ColumnInfo {
            name: h.clone(),
            data_type: "Utf8".to_string(),
        })
        .collect();

    let col_count = columns.len();
    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let mut truncated = false;
    for result in rdr.records() {
        if rows.len() >= MAX_ROWS {
            truncated = true;
            break;
        }
        let record = result?;
        let mut row: Vec<CellValue> = (0..col_count)
            .map(|i| {
                record
                    .get(i)
                    .map(infer_cell_value)
                    .unwrap_or(CellValue::Null)
            })
            .collect();
        row.resize(col_count, CellValue::Null);
        rows.push(row);
    }

    // If truncated, signal that more rows are available without reading the rest
    let total_rows = if truncated {
        Some(usize::MAX) // sentinel: unknown total, more rows available
    } else {
        None
    };

    // Refine column types based on actual data
    let mut refined_columns = columns;
    for (col_idx, col) in refined_columns.iter_mut().enumerate() {
        let mut has_int = false;
        let mut has_float = false;
        let mut has_bool = false;
        let mut has_date = false;
        let mut has_datetime = false;
        let mut has_string = false;

        for row in &rows {
            match row.get(col_idx) {
                Some(CellValue::Int(_)) => has_int = true,
                Some(CellValue::Float(_)) => has_float = true,
                Some(CellValue::Bool(_)) => has_bool = true,
                Some(CellValue::Date(_)) => has_date = true,
                Some(CellValue::DateTime(_)) => has_datetime = true,
                Some(CellValue::String(_)) => has_string = true,
                _ => {}
            }
        }

        col.data_type = if has_string {
            "Utf8".to_string()
        } else if has_datetime {
            "Timestamp(Microsecond, None)".to_string()
        } else if has_date {
            "Date32".to_string()
        } else if has_float {
            "Float64".to_string()
        } else if has_int {
            "Int64".to_string()
        } else if has_bool {
            "Boolean".to_string()
        } else {
            "Utf8".to_string()
        };
    }

    Ok(DataTable {
        columns: refined_columns,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: Some(path.to_string_lossy().to_string()),
        format_name: Some(format_name.to_string()),
        structural_changes: false,
        total_rows,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    })
}

/// Load a chunk of CSV/TSV rows in the background.
/// Skips `skip_rows` data records, then reads up to `max_rows` records.
/// Pushes rows into `buffer` in batches. Sets `done` to true when finished.
#[allow(clippy::too_many_arguments)]
pub fn load_csv_rows_chunk(
    path: &Path,
    delimiter: u8,
    skip_rows: usize,
    max_rows: usize,
    num_cols: usize,
    buffer: std::sync::Arc<std::sync::Mutex<Vec<Vec<CellValue>>>>,
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    exhausted: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_path(path)?;

    let flush_threshold = 50_000;
    let mut batch_buf = Vec::with_capacity(flush_threshold);
    let mut skipped = 0usize;
    let mut loaded = 0usize;

    for result in rdr.records() {
        let record = result?;
        if skipped < skip_rows {
            skipped += 1;
            continue;
        }
        if loaded >= max_rows {
            break;
        }
        let mut row: Vec<CellValue> = (0..num_cols)
            .map(|i| {
                record
                    .get(i)
                    .map(infer_cell_value)
                    .unwrap_or(CellValue::Null)
            })
            .collect();
        row.resize(num_cols, CellValue::Null);
        batch_buf.push(row);
        loaded += 1;

        if batch_buf.len() >= flush_threshold {
            if let Ok(mut buf) = buffer.lock() {
                buf.append(&mut batch_buf);
            }
            batch_buf = Vec::with_capacity(flush_threshold);
        }
    }

    // Flush remaining
    if !batch_buf.is_empty() {
        if let Ok(mut buf) = buffer.lock() {
            buf.append(&mut batch_buf);
        }
    }

    if loaded < max_rows {
        exhausted.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

pub fn write_delimited(path: &Path, delimiter: u8, table: &DataTable) -> Result<()> {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_path(path)?;

    let headers: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
    wtr.write_record(&headers)?;

    for row_idx in 0..table.row_count() {
        let record: Vec<String> = (0..table.col_count())
            .map(|col_idx| {
                table
                    .get(row_idx, col_idx)
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            })
            .collect();
        wtr.write_record(&record)?;
    }

    wtr.flush()?;
    Ok(())
}
