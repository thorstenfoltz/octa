//! Content-based format detection.
//!
//! Octa's primary format dispatch is extension-driven ([`FormatRegistry::
//! reader_for_path`]). That fails for two real cases:
//! - a file with **no extension** (`data`), and
//! - a file with the **wrong extension** (a Parquet file named `export.txt`).
//!
//! [`sniff_format`] inspects the file's bytes and returns the [`FormatReader::
//! name`] of the reader that should handle it, or `None` when it can't decide
//! confidently. Callers map the name back to a reader via
//! [`FormatRegistry::reader_by_name`].
//!
//! The detection is deliberately conservative: it only claims a format when a
//! magic-number / structural signal is unambiguous, so it never mis-routes an
//! ordinary text file. When nothing matches it returns `None` and the caller
//! falls back to its existing behaviour (the plain-text reader).

use std::io::Read;
use std::path::Path;

/// Read up to `max` bytes from the start of `path`. Returns `None` on an I/O
/// error. Short files yield a shorter slice rather than an error.
fn read_head(path: &Path, max: usize) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; max];
    let mut filled = 0;
    while filled < max {
        match f.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) => return None,
        }
    }
    buf.truncate(filled);
    Some(buf)
}

/// Identify a file by content. Returns the [`FormatReader::name`] of the
/// matching reader, or `None` when no confident match is found.
pub fn sniff_format(path: &Path) -> Option<&'static str> {
    let head = read_head(path, 16)?;
    if let Some(name) = sniff_magic(&head) {
        return Some(name);
    }
    sniff_text(path)
}

/// Magic-number checks against the first bytes of a file. Each signal is a
/// stable, documented format header.
fn sniff_magic(head: &[u8]) -> Option<&'static str> {
    // Parquet: "PAR1" at the start (and end) of the file.
    if head.starts_with(b"PAR1") {
        return Some("Parquet");
    }
    // SQLite 3: the 16-byte header string including the trailing NUL.
    if head.starts_with(b"SQLite format 3\0") {
        return Some("SQLite");
    }
    // DuckDB: an 8-byte checksum followed by the "DUCK" magic at offset 8.
    if head.len() >= 12 && &head[8..12] == b"DUCK" {
        return Some("DuckDB");
    }
    // ORC: the 3-byte "ORC" magic opens the file.
    if head.starts_with(b"ORC") {
        return Some("ORC");
    }
    // Arrow IPC file format: "ARROW1" magic.
    if head.starts_with(b"ARROW1") {
        return Some("Arrow IPC");
    }
    // Avro object container file: "Obj\x01".
    if head.starts_with(b"Obj\x01") {
        return Some("Avro");
    }
    // HDF5: the 8-byte signature.
    if head.starts_with(b"\x89HDF\r\n\x1a\n") {
        return Some("HDF5");
    }
    // NumPy .npy: the 6-byte "\x93NUMPY" magic. (.npz is a zip and is caught by
    // the PK magic below, routing to the Archive reader, which is acceptable.)
    if head.starts_with(b"\x93NUMPY") {
        return Some("NumPy");
    }
    // Zip local-file / central-dir / end-of-central-dir headers. Covers plain
    // zips as well as the OOXML / OpenDocument containers; we route to the
    // read-only Archive reader, which lists the entries so the user can see
    // what it actually is.
    if head.starts_with(b"PK\x03\x04")
        || head.starts_with(b"PK\x05\x06")
        || head.starts_with(b"PK\x07\x08")
    {
        return Some("Archive");
    }
    None
}

/// Structural text probes, run only when no binary magic matched. Returns a
/// reader name for JSON / JSON Lines / CSV / TSV, or `None` for prose (which
/// belongs to the plain-text fallback).
fn sniff_text(path: &Path) -> Option<&'static str> {
    let bytes = read_head(path, 8192)?;
    if bytes.is_empty() {
        return None;
    }
    // Embedded NUL bytes mean it's binary; don't guess a text format.
    if bytes.contains(&0) {
        return None;
    }
    let text = String::from_utf8_lossy(&bytes);
    let trimmed = text.trim_start();
    match trimmed.chars().next() {
        Some('[') => return Some("JSON"),
        Some('{') => {
            // Multiple non-empty lines that each open with `{` is the
            // JSON-Lines shape; a single object (possibly multi-line) is JSON.
            let object_lines = trimmed
                .lines()
                .filter(|l| !l.trim().is_empty())
                .take(3)
                .filter(|l| l.trim_start().starts_with('{'))
                .count();
            return Some(if object_lines >= 2 {
                "JSON Lines"
            } else {
                "JSON"
            });
        }
        _ => {}
    }
    // Delimited text: reuse the CSV reader's consistency-based detector, which
    // only succeeds when a candidate delimiter appears with a stable count
    // across the sampled lines (so prose doesn't read as CSV).
    if let Some(delim) = super::csv_reader::detect_delimiter(path) {
        return Some(if delim == b'\t' { "TSV" } else { "CSV" });
    }
    None
}

#[cfg(test)]
#[path = "sniff_tests.rs"]
mod tests;
