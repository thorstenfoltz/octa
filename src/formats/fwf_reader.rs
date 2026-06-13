//! Fixed-width file (FWF) reader. Read-only, best-effort.
//!
//! Fixed-width files have no delimiter: each field occupies a fixed range of
//! character columns, padded with spaces. There is no in-band schema, so this
//! reader **infers** the column boundaries by scanning a sample of lines for
//! character positions that are blank in (nearly) every line - those positions
//! are the gaps between fields. The first line is treated as the header.
//!
//! This is a heuristic and works best on cleanly aligned reports (typical
//! mainframe / spreadsheet `.prn` exports). It is deliberately read-only: there
//! is no faithful way to reconstruct the original field widths on write.

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::{FormatReader, initial_load_rows};
use anyhow::Result;
use std::path::Path;

pub struct FwfReader;

/// Number of leading non-empty lines sampled to infer column boundaries.
const SAMPLE_LINES: usize = 1000;

impl FormatReader for FwfReader {
    fn name(&self) -> &str {
        "Fixed-Width"
    }

    fn extensions(&self) -> &[&str] {
        &["fwf", "prn"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let content = std::fs::read_to_string(path)?;
        read_fwf(&content, path)
    }
}

/// Parse fixed-width `content`. Pure (no IO) so it is unit-testable.
fn read_fwf(content: &str, path: &Path) -> Result<DataTable> {
    // Keep only non-empty lines; blank lines carry no field data and would
    // otherwise blank out every position during gap detection.
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

    if lines.is_empty() {
        let mut table = DataTable::empty();
        table.source_path = Some(path.to_string_lossy().to_string());
        table.format_name = Some("Fixed-Width".to_string());
        return Ok(table);
    }

    // Work in char positions (FWF is usually ASCII, but stay UTF-8 safe).
    let rows_chars: Vec<Vec<char>> = lines.iter().map(|l| l.chars().collect()).collect();
    let ranges = infer_field_ranges(&rows_chars);

    // First line is the header; fall back to col_N for blank field names.
    let header = &rows_chars[0];
    let columns: Vec<ColumnInfo> = ranges
        .iter()
        .enumerate()
        .map(|(i, &(start, end))| {
            let name = slice_trimmed(header, start, end);
            let name = if name.is_empty() {
                format!("col_{}", i + 1)
            } else {
                name
            };
            ColumnInfo {
                name,
                data_type: "Utf8".to_string(),
            }
        })
        .collect();

    let cap = initial_load_rows();
    let data_lines = &rows_chars[1..];
    let total_data = data_lines.len();
    let take = total_data.min(cap);

    let rows: Vec<Vec<CellValue>> = data_lines[..take]
        .iter()
        .map(|line| {
            ranges
                .iter()
                .map(|&(start, end)| CellValue::String(slice_trimmed(line, start, end)))
                .collect()
        })
        .collect();

    let mut table = DataTable::empty();
    table.columns = columns;
    table.rows = rows;
    table.source_path = Some(path.to_string_lossy().to_string());
    table.format_name = Some("Fixed-Width".to_string());
    if take < total_data {
        table.total_rows = Some(total_data);
    }
    Ok(table)
}

/// Infer field `(start, end)` char ranges (end exclusive) from sampled rows.
/// A position is a "gap" when it is blank in every sampled line; fields are the
/// maximal runs of non-gap positions. Returns a single full-width field when no
/// gaps are found (e.g. a single-column file).
fn infer_field_ranges(rows: &[Vec<char>]) -> Vec<(usize, usize)> {
    let sample = &rows[..rows.len().min(SAMPLE_LINES)];
    let width = sample.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 {
        return vec![(0, 0)];
    }

    // gap[p] == true when column p is blank (space or past end) in every line.
    let is_gap = |p: usize| {
        sample
            .iter()
            .all(|line| line.get(p).map(|c| *c == ' ').unwrap_or(true))
    };

    let mut ranges = Vec::new();
    let mut start: Option<usize> = None;
    for p in 0..width {
        if is_gap(p) {
            if let Some(s) = start.take() {
                ranges.push((s, p));
            }
        } else if start.is_none() {
            start = Some(p);
        }
    }
    if let Some(s) = start {
        ranges.push((s, width));
    }

    if ranges.is_empty() {
        ranges.push((0, width));
    }
    ranges
}

/// Slice `line[start..end]` (clamped to the line length) and trim surrounding
/// whitespace.
fn slice_trimmed(line: &[char], start: usize, end: usize) -> String {
    let end = end.min(line.len());
    if start >= end {
        return String::new();
    }
    line[start..end]
        .iter()
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
#[path = "fwf_reader_tests.rs"]
mod tests;
