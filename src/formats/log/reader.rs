//! The log reader: picks a format from the first lines, builds the table,
//! and falls back to plain text when nothing fits, so a `.log` of notes
//! opens exactly as it did before logs were understood.

use std::path::Path;

use anyhow::Result;
use chrono::Datelike;

use super::{LogEntry, LogFormat, ParseCtx, formats, is_continuation};
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;

pub const LOG_READER: &str = "Log";
/// Reached by name only (View -> Reopen as -> Log): uses the best format
/// even below the detection threshold.
pub const LOG_READER_FORCED: &str = "Log (any format)";
pub const SAMPLE_LINES: usize = 500;
pub const MIN_MATCH_SHARE: f64 = 0.6;

pub struct LogReader {
    pub forced: bool,
}

impl FormatReader for LogReader {
    fn name(&self) -> &str {
        if self.forced {
            LOG_READER_FORCED
        } else {
            LOG_READER
        }
    }

    fn extensions(&self) -> &[&str] {
        if self.forced { &[] } else { &["log"] }
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let content = crate::data::encoding::read_to_string_detected(path)?;
        let lines: Vec<&str> = content.lines().collect();
        let ctx = ParseCtx {
            year: file_year(path),
        };
        let Some(format) = detect(&lines, &ctx, self.forced) else {
            if self.forced {
                anyhow::bail!("no known log format found in {}", path.display());
            }
            return crate::formats::text_reader::read_text_file(path);
        };
        let mut table = build_table(
            &lines,
            format.as_ref(),
            &ctx,
            crate::formats::initial_load_rows(),
        );
        table.source_path = Some(path.to_string_lossy().to_string());
        table.format_name = Some(LOG_READER.to_string());
        Ok(table)
    }
}

/// Classic syslog has no year; the file's own date is the best guess.
fn file_year(path: &Path) -> i32 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| chrono::DateTime::<chrono::Local>::from(t).year())
        .unwrap_or_else(|_| chrono::Local::now().year())
}

/// The format matching the most of the first [`SAMPLE_LINES`] entry lines,
/// if it matches at least [`MIN_MATCH_SHARE`] of them (or any, when forced).
pub fn detect(lines: &[&str], ctx: &ParseCtx, forced: bool) -> Option<Box<dyn LogFormat>> {
    let sample: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| !l.trim().is_empty() && !is_continuation(l))
        .take(SAMPLE_LINES)
        .collect();
    if sample.is_empty() {
        return None;
    }
    let mut best: Option<(Box<dyn LogFormat>, usize)> = None;
    for f in formats() {
        let hits = sample
            .iter()
            .filter(|l| f.parse_line(l, ctx).is_some())
            .count();
        if best.as_ref().is_none_or(|(_, h)| hits > *h) {
            best = Some((f, hits));
        }
    }
    let (format, hits) = best?;
    let share = hits as f64 / sample.len() as f64;
    ((forced && hits > 0) || share >= MIN_MATCH_SHARE).then_some(format)
}

enum Row {
    Entry(LogEntry),
    Raw(String),
}

/// One row per entry; continuation lines join the entry above; any other
/// line that fits no entry is its own row with only `raw` set. Keeps the
/// first `cap` rows and records the full count in `total_rows`.
pub fn build_table(
    lines: &[&str],
    format: &dyn LogFormat,
    ctx: &ParseCtx,
    cap: usize,
) -> DataTable {
    let mut rows: Vec<Row> = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(e) = format.parse_line(line, ctx) {
            rows.push(Row::Entry(e));
            continue;
        }
        if is_continuation(line)
            && let Some(Row::Entry(prev)) = rows.last_mut()
        {
            prev.message.push('\n');
            prev.message.push_str(line);
            continue;
        }
        rows.push(Row::Raw((*line).to_string()));
    }
    let total = rows.len();
    rows.truncate(cap);

    let mut field_names: Vec<String> = Vec::new();
    let mut known = std::collections::HashSet::new();
    for r in &rows {
        if let Row::Entry(e) = r {
            for (k, _) in &e.fields {
                if known.insert(k.clone()) {
                    field_names.push(k.clone());
                }
            }
        }
    }

    let width = 3 + field_names.len() + 2;
    let mut cells: Vec<Vec<CellValue>> = Vec::with_capacity(rows.len());
    for r in rows {
        let mut row = vec![CellValue::Null; width];
        match r {
            Row::Entry(e) => {
                row[0] = e.timestamp.map_or(CellValue::Null, CellValue::DateTime);
                row[1] = e.utc_offset.map_or(CellValue::Null, CellValue::String);
                row[2] = e.level.map_or(CellValue::Null, CellValue::String);
                for (k, v) in e.fields {
                    let i = field_names
                        .iter()
                        .position(|n| *n == k)
                        .expect("collected above");
                    row[3 + i] = v;
                }
                if !e.message.is_empty() {
                    row[width - 2] = CellValue::String(e.message);
                }
            }
            Row::Raw(line) => row[width - 1] = CellValue::String(line),
        }
        cells.push(row);
    }

    let mut names = vec!["timestamp".to_string(), "utc_offset".into(), "level".into()];
    names.extend(field_names);
    names.push("message".into());
    names.push("raw".into());
    // The fixed columns only appear when some row fills them: an access log
    // has no level, a clean log no raw lines.
    let fixed = [0, 1, 2, width - 2, width - 1];
    let keep: Vec<usize> = (0..width)
        .filter(|c| !fixed.contains(c) || cells.iter().any(|r| !matches!(r[*c], CellValue::Null)))
        .collect();

    let mut t = DataTable::empty();
    t.columns = keep
        .iter()
        .map(|&c| ColumnInfo {
            name: names[c].clone(),
            data_type: column_type(&cells, c, c == 0),
        })
        .collect();
    t.rows = cells
        .into_iter()
        .map(|r| keep.iter().map(|&c| r[c].clone()).collect())
        .collect();
    t.total_rows = (total > t.rows.len()).then_some(total);
    t
}

fn column_type(cells: &[Vec<CellValue>], c: usize, timestamp: bool) -> String {
    if timestamp {
        return "Timestamp(Microsecond, None)".into();
    }
    let (mut any, mut float) = (false, false);
    for r in cells {
        match &r[c] {
            CellValue::Null => {}
            CellValue::Int(_) => any = true,
            CellValue::Float(_) => (any, float) = (true, true),
            _ => return "Utf8".into(),
        }
    }
    match (any, float) {
        (true, false) => "Int64".into(),
        (true, true) => "Float64".into(),
        _ => "Utf8".into(),
    }
}

/// The banner for a log table with unmatched lines, `None` otherwise.
pub fn unmatched_note(table: &DataTable) -> Option<String> {
    if table.format_name.as_deref() != Some(LOG_READER) {
        return None;
    }
    let raw = table.columns.iter().position(|c| c.name == "raw")?;
    let count = table
        .rows
        .iter()
        .filter(|r| !matches!(r[raw], CellValue::Null))
        .count();
    (count > 0)
        .then(|| crate::i18n::t("logread.unmatched_note").replace("{count}", &count.to_string()))
}
