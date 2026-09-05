//! One-shot orientation snapshot of a tabular file: format, file
//! size, row count, column list, and a small sample of rows. Collapses
//! the common `list_tables` -> `schema` -> `read_table` discovery dance
//! into a single call.
//!
//! Pure (modulo two `std::fs` calls for the path's metadata + the
//! reader's own I/O) so the MCP and CLI surfaces share one
//! implementation.

use std::path::Path;

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::initial_load_rows;

/// Maximum value the caller can request via `sample_rows`. Higher
/// values are silently clamped. Keeps an MCP / CLI caller from
/// asking for a million rows through a "preview" tool.
pub const MAX_SAMPLE_ROWS: usize = 100;

/// Default sample row count when the caller omits the parameter.
pub const DEFAULT_SAMPLE_ROWS: usize = 5;

/// Complete description of a single tabular file (or, for multi-table
/// sources, one specific table within it).
#[derive(Debug, Clone)]
pub struct FileDescription {
    /// The path that was read, normalised through `Path::display`.
    pub path: String,
    /// Friendly format name as reported by the underlying reader
    /// (e.g. `"Parquet"`, `"SQLite"`). May be `None` for readers that
    /// don't populate `DataTable::format_name`.
    pub format_name: Option<String>,
    /// File size in bytes, from `std::fs::metadata`. `None` if the
    /// metadata call failed (rare; permissions or symlink issues).
    pub file_size_bytes: Option<u64>,
    /// For multi-table sources, the table that was read.
    pub table: Option<String>,
    /// Number of rows the reader returned. For streaming formats this
    /// is bounded by the file-loader cap, surfaced via
    /// `initial_load_capped`.
    pub row_count: usize,
    /// True when the row count is bounded by `initial_load_rows` and
    /// the file may hold more rows.
    pub initial_load_capped: bool,
    /// The initial-load cap in effect when the file was read.
    pub initial_load_cap: usize,
    /// Column schema.
    pub columns: Vec<ColumnInfo>,
    /// First N rows of the table, where N = `sample_rows` clamped
    /// to `MAX_SAMPLE_ROWS` and the actual row count.
    pub sample_rows: Vec<Vec<CellValue>>,
}

/// Read a file and build its description.
///
/// `sample_rows` is the desired number of preview rows; passing
/// `None` uses [`DEFAULT_SAMPLE_ROWS`]. Values are clamped to
/// [`MAX_SAMPLE_ROWS`] and to the actual row count.
///
/// For multi-table sources called with `table = None`, the reader's
/// default behaviour is used (typically the first table or an error
/// directing the caller to `list_tables`).
pub fn describe_file(
    path: &Path,
    table: Option<&str>,
    sample_rows: Option<usize>,
) -> anyhow::Result<FileDescription> {
    // Through `read_table_auto`, not the registry directly: that is the one
    // entry point that decompresses a `.csv.gz` / `.jsonl.zst` first. Reaching
    // for `reader_for_path` here meant `--describe` (and the MCP tool beside
    // it) reported a compressed file as four lines of binary text while
    // `--head` on the same path read it correctly.
    let dt: DataTable = crate::formats::read_table_auto(
        path,
        table,
        crate::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES,
    )?;

    let cap = initial_load_rows();
    let row_count = dt.row_count();
    let capped = cap != usize::MAX && row_count >= cap;

    let want = sample_rows
        .unwrap_or(DEFAULT_SAMPLE_ROWS)
        .min(MAX_SAMPLE_ROWS);
    let take = want.min(row_count);
    let mut preview: Vec<Vec<CellValue>> = Vec::with_capacity(take);
    for r in 0..take {
        let mut row = Vec::with_capacity(dt.col_count());
        for c in 0..dt.col_count() {
            row.push(dt.get(r, c).cloned().unwrap_or(CellValue::Null));
        }
        preview.push(row);
    }

    let file_size = std::fs::metadata(path).ok().map(|m| m.len());

    Ok(FileDescription {
        path: path.display().to_string(),
        format_name: dt.format_name.clone(),
        file_size_bytes: file_size,
        table: table.map(|s| s.to_string()),
        row_count,
        initial_load_capped: capped,
        initial_load_cap: cap,
        columns: dt.columns.clone(),
        sample_rows: preview,
    })
}
