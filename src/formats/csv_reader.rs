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

    fn write_file_with_options(
        &self,
        path: &Path,
        table: &DataTable,
        opts: &crate::formats::write_options::WriteOptions,
    ) -> Result<()> {
        write_delimited_with(path, table, &opts.csv, b',')
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

    fn write_file_with_options(
        &self,
        path: &Path,
        table: &DataTable,
        opts: &crate::formats::write_options::WriteOptions,
    ) -> Result<()> {
        write_delimited_with(path, table, &opts.csv, b'\t')
    }
}

/// Auto-detect the delimiter of a CSV file from its first lines. Reads at
/// most 1 MB, so a multi-gigabyte file costs what a small one does.
pub fn detect_delimiter(path: &Path) -> Option<u8> {
    const SNIFF_BYTES: usize = 1 << 20;
    let raw = read_prefix(path, SNIFF_BYTES)?;
    let mut text = String::from_utf8_lossy(&raw).into_owned();
    // A full buffer most likely cut the last line short, and a short line
    // would break the consistency check below: leave it out.
    if raw.len() == SNIFF_BYTES
        && let Some(cut) = text.rfind('\n')
    {
        text.truncate(cut);
    }
    detect_delimiter_in(&text)
}

/// The delimiter the first 20 lines agree on.
///
/// Per candidate, `C` is its most common non-zero count per line. From the
/// first line holding `C` on, every line must hold `C` or none; lines
/// **before** it may hold fewer, which is what a title block above the real
/// header looks like (`Report generated 03.10.2026`). At least two lines
/// must hold `C` (a lone title line with a comma is not a table), unless the
/// text is a single line. Among the candidates that pass, the one whose run
/// starts earliest wins: the header is the first structured line, while
/// decimal commas only start in the data below it. Then the larger `C`,
/// then the order `,;|\t`.
pub fn detect_delimiter_in(content: &str) -> Option<u8> {
    let lines: Vec<&str> = content.lines().take(20).collect();
    let non_empty = lines.iter().filter(|l| !l.trim().is_empty()).count();
    // (first line of the run, count per line, delimiter)
    let mut best: Option<(usize, usize, u8)> = None;
    for &delim in b",;|\t" {
        let counts: Vec<usize> = lines
            .iter()
            .map(|l| l.matches(delim as char).count())
            .collect();
        let Some(c) = most_common_nonzero(&counts) else {
            continue;
        };
        let Some(start) = counts.iter().position(|&n| n == c) else {
            continue;
        };
        let consistent = counts[..start].iter().all(|&n| n < c)
            && counts[start..].iter().all(|&n| n == c || n == 0);
        let agreeing = counts.iter().filter(|&&n| n == c).count();
        if !consistent || (agreeing < 2 && non_empty > 1) {
            continue;
        }
        let better = match best {
            None => true,
            Some((s, bc, _)) => start < s || (start == s && c > bc),
        };
        if better {
            best = Some((start, c, delim));
        }
    }
    best.map(|(_, _, d)| d)
}

/// The most frequent non-zero value in `counts`; a tie goes to the larger.
fn most_common_nonzero(counts: &[usize]) -> Option<usize> {
    let mut freq: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    for &n in counts.iter().filter(|&&n| n > 0) {
        *freq.entry(n).or_default() += 1;
    }
    freq.into_iter()
        .max_by_key(|&(n, f)| (f, n))
        .map(|(n, _)| n)
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

/// How many records the width check looks at.
const BODY_SAMPLE: usize = 50;

/// The table width: the header's, unless most sampled records have more
/// fields (a title line sat where the header should be); then the most
/// common record width. A few long rows keep the header width, as before.
fn body_width(header: usize, record_lens: &[usize]) -> usize {
    let wider = record_lens.iter().filter(|&&n| n > header).count();
    if wider * 2 > record_lens.len() {
        header.max(most_common_nonzero(record_lens).unwrap_or(0))
    } else {
        header
    }
}

fn record_to_row(record: &csv::StringRecord, width: usize) -> Vec<CellValue> {
    (0..width)
        .map(|i| {
            record
                .get(i)
                .map(infer_cell_value)
                .unwrap_or(CellValue::Null)
        })
        .collect()
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
        // Peek at the first records: a title line above the real header
        // makes the header narrower than the data, and sizing rows to it
        // would cut every later field off.
        let mut records = rdr.records();
        let mut head: Vec<csv::StringRecord> = Vec::new();
        for result in records.by_ref().take(BODY_SAMPLE) {
            head.push(result?);
        }
        let lens: Vec<usize> = head.iter().map(|r| r.len()).collect();
        let width = body_width(header_count, &lens);
        for record in &head {
            if rows.len() >= max_rows {
                truncated = true;
                break;
            }
            rows.push(record_to_row(record, width));
        }
        if !truncated {
            for result in records {
                if rows.len() >= max_rows {
                    truncated = true;
                    break;
                }
                rows.push(record_to_row(&result?, width));
            }
        }
        width
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
    // Overflow columns empty in every row are a trailing delimiter on each
    // data line (`1;2;3;`), not data: drop them so such files read as before.
    while columns.len() > header_count
        && rows
            .iter()
            .all(|r| matches!(r.last(), None | Some(CellValue::Null)))
    {
        columns.pop();
        for row in &mut rows {
            row.pop();
        }
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
        col.data_type =
            crate::data::column_type_from_cells(rows.iter().filter_map(|r| r.get(col_idx)))
                .to_string();
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
        formulas: std::collections::HashMap::new(),
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

    // A leading UTF-8 byte order mark is NOT a defect: spreadsheet programs
    // want it, Octa itself writes it on request, and the reader drops it from
    // the first header. Flagging it had every such file, Octa's own output
    // included, ask for a repair on every open.

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
/// Which slice of the file a chunk load should read.
///
/// Three bare `usize`s in a row is the shape where a call site can transpose
/// two and still compile: `skip_rows` and `max_rows` swapped would silently
/// read the wrong window of the file, and `num_cols` in the wrong slot would
/// reshape every row. Naming them at the call site is the point.
pub struct ChunkSpec {
    pub skip_rows: usize,
    pub max_rows: usize,
    pub num_cols: usize,
}

/// Where a background chunk load reports to. Always created and moved
/// together, so it travels as one argument.
pub struct ChunkSink {
    pub buffer: std::sync::Arc<std::sync::Mutex<Vec<Vec<CellValue>>>>,
    pub done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub exhausted: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Where the last chunk stopped: records read, and the byte position of
    /// the next one. A chunk starting at that record count seeks there
    /// instead of parsing the file from the start again, which made every
    /// chunk slower than the one before. Kept per tab, across chunks.
    pub resume: ChunkResume,
}

/// See [`ChunkSink::resume`].
pub type ChunkResume = std::sync::Arc<std::sync::Mutex<Option<(usize, csv::Position)>>>;

pub fn load_csv_rows_chunk(
    path: &Path,
    delimiter: u8,
    spec: ChunkSpec,
    sink: ChunkSink,
) -> Result<()> {
    let ChunkSpec {
        skip_rows,
        max_rows,
        num_cols,
    } = spec;
    let ChunkSink {
        buffer,
        done,
        exhausted,
        resume,
    } = sink;
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_path(path)?;

    let mut record = csv::StringRecord::new();
    let resume_at = resume
        .lock()
        .ok()
        .and_then(|r| r.clone())
        .filter(|(records, _)| skip_rows > 0 && *records == skip_rows);
    if let Some((_, position)) = resume_at {
        rdr.seek(position)?;
    } else {
        // A reused record, so skipping allocates nothing per row.
        for _ in 0..skip_rows {
            if !rdr.read_record(&mut record)? {
                break;
            }
        }
    }

    let flush_threshold = 50_000;
    let mut batch_buf = Vec::with_capacity(flush_threshold);
    let mut loaded = 0usize;

    // The limit is checked before reading, so the position after the loop is
    // the start of the first record this chunk did not take.
    while loaded < max_rows && rdr.read_record(&mut record)? {
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

    if let Ok(mut r) = resume.lock() {
        *r = Some((skip_rows + loaded, rdr.position().clone()));
    }
    if loaded < max_rows {
        exhausted.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

pub fn write_delimited(path: &Path, delimiter: u8, table: &DataTable) -> Result<()> {
    let opts = crate::formats::write_options::CsvOptions {
        delimiter,
        ..Default::default()
    };
    write_delimited_with(path, table, &opts, delimiter)
}

/// Options-aware delimited writer. `write_delimited` delegates here with the
/// defaults, so the two cannot drift.
///
/// `format_delimiter` is the separator the format itself implies (a tab for
/// TSV, the tab's own delimiter for CSV). It wins in two cases: whenever
/// `opts.delimiter` is still the default comma, and *always* for a TSV, whose
/// definition is the tab. Without that second rule a saved delimiter of `;`
/// wrote semicolons into a file named `.tsv`, which no TSV reader will parse
/// as the user intended. Someone wanting `;` wants a `.csv`.
pub fn write_delimited_with(
    path: &Path,
    table: &DataTable,
    opts: &crate::formats::write_options::CsvOptions,
    format_delimiter: u8,
) -> Result<()> {
    // Through a sibling temp + rename: a half-written CSV must not replace the
    // user's file (see `formats::write_atomically`).
    crate::formats::write_slices(
        path,
        &crate::formats::SliceFormat::Delimited {
            opts: opts.clone(),
            delimiter: format_delimiter,
        },
        &table.columns,
        [Ok(table)],
    )
}

/// The delimited half of [`write_slices`](crate::formats::write_slices): the
/// header, then the rows of each slice, straight into `path` (the caller owns
/// the temp and the rename).
pub(crate) fn write_delimited_slices<T: std::borrow::Borrow<DataTable>>(
    path: &Path,
    columns: &[ColumnInfo],
    slices: impl IntoIterator<Item = Result<T>>,
    opts: &crate::formats::write_options::CsvOptions,
    format_delimiter: u8,
) -> Result<()> {
    let delimiter = if format_delimiter == b'\t' || opts.delimiter == b',' {
        format_delimiter
    } else {
        opts.delimiter
    };
    let mut wtr = delimited_writer(std::fs::File::create(path)?, opts, delimiter)?;
    if opts.write_header {
        write_delimited_header(&mut wtr, columns)?;
    }
    for slice in slices {
        write_delimited_rows(&mut wtr, slice?.borrow(), opts.decimal_comma)?;
    }
    wtr.flush()?;
    Ok(())
}

/// The header and the first `rows` rows of `table` as `opts` would write them,
/// for the Save As dialog's preview. The byte order mark is left out: it is
/// invisible anyway.
pub fn preview_delimited(
    table: &DataTable,
    opts: &crate::formats::write_options::CsvOptions,
    rows: usize,
) -> String {
    let head = table.clone_with_rows(&(0..rows.min(table.row_count())).collect::<Vec<_>>());
    let opts = crate::formats::write_options::CsvOptions {
        bom: false,
        ..opts.clone()
    };
    let mut out = Vec::new();
    let written = (|| -> Result<()> {
        let mut wtr = delimited_writer(&mut out, &opts, opts.delimiter)?;
        if opts.write_header {
            write_delimited_header(&mut wtr, &head.columns)?;
        }
        write_delimited_rows(&mut wtr, &head, opts.decimal_comma)?;
        wtr.flush()?;
        Ok(())
    })();
    match written {
        Ok(()) => String::from_utf8_lossy(&out).into_owned(),
        Err(e) => format!("{e:#}"),
    }
}

/// A CSV writer over `out` with `opts` applied and, when `opts.bom` asks for
/// it, the UTF-8 byte order mark already written. Shared with the streaming
/// database export, which writes one batch at a time into the same writer.
pub(crate) fn delimited_writer<W: std::io::Write>(
    mut out: W,
    opts: &crate::formats::write_options::CsvOptions,
    delimiter: u8,
) -> Result<csv::Writer<W>> {
    use crate::formats::write_options::QuoteStyle;

    if opts.bom {
        out.write_all(b"\xEF\xBB\xBF")?;
    }
    Ok(csv::WriterBuilder::new()
        .delimiter(delimiter)
        .quote_style(match opts.quote_style {
            QuoteStyle::Necessary => csv::QuoteStyle::Necessary,
            QuoteStyle::Always => csv::QuoteStyle::Always,
            QuoteStyle::Never => csv::QuoteStyle::Never,
        })
        .terminator(if opts.crlf {
            csv::Terminator::CRLF
        } else {
            csv::Terminator::Any(b'\n')
        })
        .from_writer(out))
}

/// The header row: one field per column name.
pub(crate) fn write_delimited_header<W: std::io::Write>(
    wtr: &mut csv::Writer<W>,
    columns: &[ColumnInfo],
) -> Result<()> {
    wtr.write_record(columns.iter().map(|c| c.name.as_str()))?;
    Ok(())
}

/// Every row of `table`, in order.
pub(crate) fn write_delimited_rows<W: std::io::Write>(
    wtr: &mut csv::Writer<W>,
    table: &DataTable,
    decimal_comma: bool,
) -> Result<()> {
    for row_idx in 0..table.row_count() {
        let record: Vec<String> = (0..table.col_count())
            .map(|col_idx| {
                table
                    .get(row_idx, col_idx)
                    .map(|v| cell_text(v, decimal_comma))
                    .unwrap_or_default()
            })
            .collect();
        wtr.write_record(&record)?;
    }
    Ok(())
}

/// One cell as CSV text. With `decimal_comma` a decimal number is written with
/// a comma (`1234,56`), the way a German or French spreadsheet program reads
/// it. Only real numbers change: text that merely looks like a number is the
/// user's own text and is written exactly as it is.
fn cell_text(cell: &CellValue, decimal_comma: bool) -> String {
    match cell {
        CellValue::Float(_) if decimal_comma => cell.to_string().replacen('.', ",", 1),
        _ => cell.to_string(),
    }
}
