//! Archive viewer: list the contents of `.zip`, `.tar`, and `.tgz`
//! files as a regular octa table. The reader is **read-only**; each
//! row carries the entry's path / size / mtime / is_dir / type, and
//! a small action bar above the table (rendered by the binary side
//! when `format_name == "Archive"`) lets the user extract a row's
//! entry to a tempfile and open it as a new tab via the normal
//! `OctaApp::load_file` path.
//!
//! No `tar.bz2` support yet — that would need a bzip2 crate, and the
//! current dep allowlist doesn't include one.
//!
//! Single-extension matching means **`.tar.gz` doesn't route here**:
//! `Path::extension()` returns just `gz`, which we can't claim
//! without colliding with `file.csv.gz`. Rename to `.tgz` (or add
//! sniff-based routing later) to use this reader.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDateTime, Utc};

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;

pub struct ArchiveReader;

impl FormatReader for ArchiveReader {
    fn name(&self) -> &str {
        "Archive"
    }

    fn extensions(&self) -> &[&str] {
        // No `tar.gz` here — single-component matching in
        // FormatRegistry can't disambiguate it from other `.gz` use
        // cases. `.tgz` (the all-in-one extension) is supported and
        // is what we recommend in the docs.
        &["zip", "tar", "tgz"]
    }

    fn supports_write(&self) -> bool {
        false
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let kind = detect_kind(path).context("identifying archive kind")?;
        let entries = match kind {
            ArchiveKind::Zip => read_zip_entries(path)?,
            ArchiveKind::Tar => read_tar_entries(path, false)?,
            ArchiveKind::Tgz => read_tar_entries(path, true)?,
        };
        Ok(build_table(entries, path, kind))
    }
}

/// Kind of archive — drives which reader codepath fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    Tar,
    Tgz,
}

impl ArchiveKind {
    pub fn label(self) -> &'static str {
        match self {
            ArchiveKind::Zip => "Zip",
            ArchiveKind::Tar => "Tar",
            ArchiveKind::Tgz => "Tar+Gzip",
        }
    }
}

fn detect_kind(path: &Path) -> Result<ArchiveKind> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "zip" => Ok(ArchiveKind::Zip),
        "tar" => Ok(ArchiveKind::Tar),
        "tgz" => Ok(ArchiveKind::Tgz),
        other => bail!("unsupported archive extension: .{}", other),
    }
}

/// In-memory archive entry, used to build the result DataTable.
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    pub path: String,
    pub size_bytes: i64,
    /// `None` for tar (uncompressed per-entry); `Some(n)` for zip.
    pub compressed_bytes: Option<i64>,
    pub mtime: Option<NaiveDateTime>,
    pub is_dir: bool,
}

fn read_zip_entries(path: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut archive =
        zip::ZipArchive::new(BufReader::new(file)).context("reading zip central directory")?;
    let mut out = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let entry = archive.by_index(i).context("reading zip entry header")?;
        let name = entry.name().to_string();
        let is_dir = entry.is_dir() || name.ends_with('/');
        let mtime = entry.last_modified().and_then(|d| {
            let y = d.year() as i32;
            let m = d.month() as u32;
            let day = d.day() as u32;
            let hh = d.hour() as u32;
            let mm = d.minute() as u32;
            let ss = d.second() as u32;
            chrono::NaiveDate::from_ymd_opt(y, m, day).and_then(|date| date.and_hms_opt(hh, mm, ss))
        });
        out.push(ArchiveEntry {
            path: name,
            size_bytes: entry.size() as i64,
            compressed_bytes: Some(entry.compressed_size() as i64),
            mtime,
            is_dir,
        });
    }
    Ok(out)
}

fn read_tar_entries(path: &Path, gz: bool) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let buf = BufReader::new(file);
    if gz {
        let dec = flate2::read::GzDecoder::new(buf);
        read_tar_from(dec)
    } else {
        read_tar_from(buf)
    }
}

fn read_tar_from<R: Read>(reader: R) -> Result<Vec<ArchiveEntry>> {
    let mut archive = tar::Archive::new(reader);
    let mut out = Vec::new();
    for entry in archive.entries().context("reading tar entries")? {
        let entry = entry.context("reading tar entry header")?;
        let header = entry.header();
        let path = entry
            .path()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let size = header.size().unwrap_or(0) as i64;
        let mtime = header.mtime().ok().and_then(|secs| {
            DateTime::<Utc>::from_timestamp(secs as i64, 0).map(|dt| dt.naive_utc())
        });
        let is_dir = header.entry_type().is_dir() || path.ends_with('/');
        out.push(ArchiveEntry {
            path,
            size_bytes: size,
            compressed_bytes: None,
            mtime,
            is_dir,
        });
    }
    Ok(out)
}

fn build_table(entries: Vec<ArchiveEntry>, source_path: &Path, kind: ArchiveKind) -> DataTable {
    let columns = vec![
        ColumnInfo {
            name: "path".to_string(),
            data_type: "Utf8".to_string(),
        },
        ColumnInfo {
            name: "size_bytes".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "compressed_bytes".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "mtime".to_string(),
            data_type: "Timestamp(Microsecond, None)".to_string(),
        },
        ColumnInfo {
            name: "is_dir".to_string(),
            data_type: "Boolean".to_string(),
        },
        ColumnInfo {
            name: "type".to_string(),
            data_type: "Utf8".to_string(),
        },
    ];

    let rows: Vec<Vec<CellValue>> = entries
        .into_iter()
        .map(|e| {
            vec![
                CellValue::String(e.path.clone()),
                CellValue::Int(e.size_bytes),
                match e.compressed_bytes {
                    Some(n) => CellValue::Int(n),
                    None => CellValue::Null,
                },
                match e.mtime {
                    Some(dt) => CellValue::String(dt.format("%Y-%m-%d %H:%M:%S").to_string()),
                    None => CellValue::Null,
                },
                CellValue::Bool(e.is_dir),
                CellValue::String(classify_path(&e.path, e.is_dir)),
            ]
        })
        .collect();

    DataTable {
        columns,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: Some(source_path.to_string_lossy().to_string()),
        // Tag with the dialect so the UI banner can label the archive
        // (e.g. "Zip" vs "Tar+Gzip"). Starts with "Archive" so the
        // central-panel detection (which checks `format_name.starts_with("Archive")`)
        // covers every dialect uniformly.
        format_name: Some(format!("Archive ({})", kind.label())),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    }
}

/// Guess a coarse "type" string for an entry from its extension or
/// directory status. Used for the `type` column so users can sort by
/// it. Not authoritative — the file's real format is determined when
/// the entry is extracted and re-opened.
fn classify_path(path: &str, is_dir: bool) -> String {
    if is_dir {
        return "dir".to_string();
    }
    let lower = path.to_ascii_lowercase();
    let ext = std::path::Path::new(&lower)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    if ext.is_empty() {
        "file".to_string()
    } else {
        ext.to_string()
    }
}

/// Extract one entry by path into a `BufRead`-friendly `Vec<u8>`.
/// Returns `Err` when the entry doesn't exist. Public so the binary
/// side can re-open it from a tempfile.
pub fn extract_entry_bytes(archive_path: &Path, entry_path: &str) -> Result<Vec<u8>> {
    let kind = detect_kind(archive_path)?;
    match kind {
        ArchiveKind::Zip => extract_zip_entry(archive_path, entry_path),
        ArchiveKind::Tar => extract_tar_entry(archive_path, entry_path, false),
        ArchiveKind::Tgz => extract_tar_entry(archive_path, entry_path, true),
    }
}

fn extract_zip_entry(archive_path: &Path, entry_path: &str) -> Result<Vec<u8>> {
    let file =
        File::open(archive_path).with_context(|| format!("opening {}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file))?;
    let mut entry = archive.by_name(entry_path).with_context(|| {
        format!(
            "entry \"{}\" not found in {}",
            entry_path,
            archive_path.display()
        )
    })?;
    if entry.is_dir() {
        bail!("entry \"{}\" is a directory, not a file", entry_path);
    }
    let mut out = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut out)
        .context("reading zip entry body")?;
    Ok(out)
}

fn extract_tar_entry(archive_path: &Path, entry_path: &str, gz: bool) -> Result<Vec<u8>> {
    let file =
        File::open(archive_path).with_context(|| format!("opening {}", archive_path.display()))?;
    let buf = BufReader::new(file);
    if gz {
        find_in_tar(flate2::read::GzDecoder::new(buf), entry_path)
    } else {
        find_in_tar(buf, entry_path)
    }
}

fn find_in_tar<R: Read>(reader: R, entry_path: &str) -> Result<Vec<u8>> {
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().context("reading tar entries")? {
        let mut entry = entry.context("reading tar entry header")?;
        let path = entry
            .path()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if path == entry_path {
            if entry.header().entry_type().is_dir() {
                bail!("entry \"{}\" is a directory, not a file", entry_path);
            }
            let mut out = Vec::new();
            // Streaming read — tar entries don't store size separately
            // from the payload, so we just drain.
            entry
                .read_to_end(&mut out)
                .context("reading tar entry body")?;
            return Ok(out);
        }
    }
    bail!("entry \"{}\" not found in archive", entry_path)
}

/// Holds tempfile handles for archive entries the user has opened so
/// the OS keeps the files alive until the app exits. Mirrors the
/// approach in `parse_in_new_tab`. Optional Vec on `OctaApp` because
/// most sessions never touch an archive.
///
/// This struct intentionally has no public fields — the binary side
/// pushes the `NamedTempFile` after calling `.keep()` is unsafe
/// (would lose the path) so we hold the wrapper instead.
pub use tempfile::NamedTempFile;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_picks_extension_for_files() {
        assert_eq!(classify_path("a/b/c.csv", false), "csv");
        assert_eq!(classify_path("noext", false), "file");
        assert_eq!(classify_path("a/b/", true), "dir");
        assert_eq!(classify_path("CAPS.JSON", false), "json");
    }
}
