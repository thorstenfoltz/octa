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

/// Options for the repair-aware read path ([`read_delimited_opts`]). All
/// fields default to "do nothing", so the default options reproduce the
/// standard read (modulo decoding the file into memory).
#[derive(Debug, Clone, Default)]
pub struct ReadOptions {
    /// Decode the file with `from_utf8_lossy` (replacing invalid byte
    /// sequences) instead of failing on non-UTF-8 input.
    pub lossy_utf8: bool,
    /// Force a specific delimiter. `None` uses the caller's default.
    pub delimiter: Option<u8>,
    /// Strip a leading UTF-8 BOM and stray control characters (everything
    /// except tab / CR / LF) before parsing.
    pub strip_bom_controls: bool,
    /// Keep every field of ragged rows instead of dropping extras. When set,
    /// the table is widened to the longest record and overflow columns get
    /// synthetic names, so no value is lost; short rows pad with Null.
    pub preserve_ragged: bool,
}

/// A diagnosis of why a delimited file looks malformed, plus the
/// [`ReadOptions`] that would repair it. Produced by [`analyze_delimited`].
#[derive(Debug, Clone)]
pub struct RepairPlan {
    /// Human-readable issues to show the user (ASCII only for egui glyphs).
    pub issues: Vec<String>,
    /// Suggested options that address the detected issues.
    pub options: ReadOptions,
}

fn read_delimited(path: &Path, delimiter: u8, format_name: &str) -> Result<DataTable> {
    let rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_path(path)?;
    build_table_from_reader(rdr, path, format_name, false)
}

/// Strip a leading UTF-8 BOM and any control characters other than the
/// whitespace that delimited text legitimately uses (tab, CR, LF).
fn strip_bom_and_controls(s: &str) -> String {
    let s = s.strip_prefix('\u{feff}').unwrap_or(s);
    s.chars()
        .filter(|&c| !c.is_control() || c == '\t' || c == '\n' || c == '\r')
        .collect()
}

/// Repair-aware delimited read. Reads the whole file into memory (so it can
/// re-decode / clean it), applies `opts`, and parses. Used only by the
/// opt-in malformed-file repair flow - the normal path stays on the streaming
/// [`read_delimited`] so large healthy files don't balloon memory.
pub fn read_delimited_opts(
    path: &Path,
    default_delimiter: u8,
    format_name: &str,
    opts: &ReadOptions,
) -> Result<DataTable> {
    let raw = std::fs::read(path)?;
    let mut content = if opts.lossy_utf8 {
        String::from_utf8_lossy(&raw).into_owned()
    } else {
        String::from_utf8(raw).map_err(|e| anyhow::anyhow!("invalid UTF-8: {e}"))?
    };
    if opts.strip_bom_controls {
        content = strip_bom_and_controls(&content);
    }
    let delimiter = opts.delimiter.unwrap_or(default_delimiter);
    let rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_reader(content.as_bytes());
    build_table_from_reader(rdr, path, format_name, opts.preserve_ragged)
}

/// Build a [`DataTable`] from any CSV reader. Shared by the streaming
/// ([`read_delimited`]) and repair ([`read_delimited_opts`]) paths so the
/// header / row / type-refinement logic lives in one place.
fn build_table_from_reader<R: std::io::Read>(
    mut rdr: csv::Reader<R>,
    path: &Path,
    format_name: &str,
    preserve_ragged: bool,
) -> Result<DataTable> {
    let headers: Vec<String> = rdr.headers()?.iter().map(|h| h.to_string()).collect();
    let header_count = headers.len();
    let max_rows = super::initial_load_rows();

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let mut truncated = false;
    // Column count: normally the header width. When preserving ragged rows we
    // first learn the widest record so overflow fields keep their own columns
    // instead of being dropped. Only the repair path sets `preserve_ragged`,
    // so the common streaming path keeps its row-by-row, lower-memory shape.
    let col_count = if preserve_ragged {
        let mut records: Vec<csv::StringRecord> = Vec::new();
        for result in rdr.records() {
            if records.len() >= max_rows {
                truncated = true;
                break;
            }
            records.push(result?);
        }
        let col_count = records
            .iter()
            .map(|r| r.len())
            .max()
            .unwrap_or(header_count)
            .max(header_count);
        for record in &records {
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
        col_count
    } else {
        for result in rdr.records() {
            if rows.len() >= max_rows {
                truncated = true;
                break;
            }
            let record = result?;
            let mut row: Vec<CellValue> = (0..header_count)
                .map(|i| {
                    record
                        .get(i)
                        .map(infer_cell_value)
                        .unwrap_or(CellValue::Null)
                })
                .collect();
            row.resize(header_count, CellValue::Null);
            rows.push(row);
        }
        header_count
    };

    // Build columns, adding synthetic names for any overflow beyond the header.
    let mut columns: Vec<ColumnInfo> = headers
        .iter()
        .map(|h| ColumnInfo {
            name: h.clone(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    while columns.len() < col_count {
        let name = overflow_column_name(&columns, columns.len());
        columns.push(ColumnInfo {
            name,
            data_type: "Utf8".to_string(),
        });
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

/// Inspect a delimited file for signs of malformation and, if any are found,
/// return a [`RepairPlan`] describing the issues and the [`ReadOptions`] that
/// would fix them. Returns `None` when the file looks healthy.
///
/// `default_delimiter` is the delimiter the normal reader would use (for CSV
/// that is the auto-detected one, for TSV it is the tab). Only a bounded
/// prefix is sampled so the check stays cheap even on large files.
pub fn analyze_delimited(path: &Path, default_delimiter: u8) -> Option<RepairPlan> {
    const SAMPLE_BYTES: usize = 256 * 1024;

    let raw = read_prefix(path, SAMPLE_BYTES)?;
    if raw.is_empty() {
        return None;
    }

    let mut issues: Vec<String> = Vec::new();
    let mut options = ReadOptions {
        delimiter: Some(default_delimiter),
        ..Default::default()
    };

    // Encoding: flag only a *genuine* invalid byte sequence, not a multi-byte
    // codepoint clipped by the sample boundary (error_len() is None for the
    // truncated-at-end case).
    let valid_prefix: &str = match std::str::from_utf8(&raw) {
        Ok(s) => s,
        Err(e) => {
            if e.error_len().is_some() {
                issues.push("Invalid character encoding (not valid UTF-8)".to_string());
                options.lossy_utf8 = true;
            }
            std::str::from_utf8(&raw[..e.valid_up_to()]).unwrap_or("")
        }
    };

    // Leading byte-order mark.
    if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        issues.push("Leading byte-order mark (BOM)".to_string());
        options.strip_bom_controls = true;
    }

    // Stray control characters (anything other than tab / CR / LF).
    if valid_prefix
        .chars()
        .any(|c| c.is_control() && c != '\t' && c != '\n' && c != '\r')
    {
        issues.push("Stray control characters".to_string());
        options.strip_bom_controls = true;
    }

    // Delimiter mismatch: a different, consistent delimiter fits better than
    // the one the reader would use (mainly relevant to TSV).
    if let Some(detected) = detect_delimiter(path)
        && detected != default_delimiter
    {
        issues.push(format!(
            "Delimiter looks like '{}', not '{}'",
            describe_delim(detected),
            describe_delim(default_delimiter)
        ));
        options.delimiter = Some(detected);
    }

    // Ragged rows: record field counts disagree with the header under the
    // chosen delimiter.
    let delim = options.delimiter.unwrap_or(default_delimiter);
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .has_headers(true)
        .flexible(true)
        .from_reader(valid_prefix.as_bytes());
    let header_len = rdr.headers().map(|h| h.len()).unwrap_or(0);
    if header_len > 0 {
        let ragged = rdr
            .records()
            .take(200)
            .filter_map(|r| r.ok())
            .any(|r| r.len() != header_len);
        if ragged {
            issues.push("Rows have inconsistent column counts".to_string());
            // Repair by widening the table so extra fields keep their own
            // columns rather than being silently dropped.
            options.preserve_ragged = true;
        }
    }

    if issues.is_empty() {
        None
    } else {
        Some(RepairPlan { issues, options })
    }
}

/// Read up to `max` bytes from the start of a file (best effort).
fn read_prefix(path: &Path, max: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    f.take(max as u64).read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Pick a name for an overflow column at `idx` (0-based) that does not collide
/// with any already-present column. Base name is 1-based ("column_4" for the
/// fourth column); a numeric suffix is appended on collision.
fn overflow_column_name(existing: &[ColumnInfo], idx: usize) -> String {
    let taken = |name: &str| existing.iter().any(|c| c.name == name);
    let base = format!("column_{}", idx + 1);
    if !taken(&base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Human label for a delimiter byte (ASCII only).
fn describe_delim(d: u8) -> String {
    match d {
        b'\t' => "tab".to_string(),
        b' ' => "space".to_string(),
        other => (other as char).to_string(),
    }
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
    if !batch_buf.is_empty()
        && let Ok(mut buf) = buffer.lock()
    {
        buf.append(&mut batch_buf);
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
