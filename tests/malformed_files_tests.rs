//! A damaged file must never take Octa down. Every reader, given a broken
//! copy of a fixture, answers with an error or a table: a panic in a reader
//! closes the window (small files are read on the window thread), and an
//! endless loop freezes it.
//!
//! The damage is deterministic, so a failure reproduces: the file cut short
//! at several lengths, single bytes flipped, and four bytes set to a huge
//! little-endian number, which is what a corrupt length field looks like.
//! SQLite and DuckDB files are not here: those engines are C, and a crash
//! there cannot be caught.

mod common;

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use octa::formats::FormatRegistry;

/// Long enough for a slow reader on a slow CI machine, short enough that a
/// loop shows up.
const TIMEOUT: Duration = Duration::from_secs(30);

fn fixtures() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![common::fixture_path("")];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            stack.extend(std::fs::read_dir(&p).unwrap().map(|e| e.unwrap().path()));
        } else {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// What can happen to a file on the way: cut short, a byte changed, a
/// length field gone wrong.
fn damaged(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let len = bytes.len();
    let mut out = vec![("empty".to_string(), Vec::new())];
    for at in (1..8).map(|k| len * k / 8).chain([len.saturating_sub(1)]) {
        out.push((format!("cut at {at}"), bytes[..at].to_vec()));
    }
    for at in (0..16).map(|k| len * k / 16).filter(|at| *at < len) {
        let mut b = bytes.to_vec();
        b[at] ^= 0xFF;
        out.push((format!("byte {at} flipped"), b));
    }
    if len >= 4 {
        for at in (0..8).map(|k| (len - 4) * k / 8) {
            let mut b = bytes.to_vec();
            b[at..at + 4].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0x7F]);
            out.push((format!("huge number at {at}"), b));
        }
    }
    out
}

/// Everything an open does with a file: sniff it, read its schema (schema
/// drift does that for a whole folder), read it, list its tables and read
/// the first few. Errors are fine; only a panic or a hang
/// is a finding.
fn open_like_the_gui(path: &Path) {
    let _ = octa::formats::sniff::sniff_format(path);
    let registry = FormatRegistry::new();
    let Some(reader) = registry.reader_for_path(path) else {
        return;
    };
    let _ = reader.read_schema(path);
    let _ = reader.read_file(path);
    if let Ok(Some(tables)) = reader.list_tables(path) {
        for t in tables.iter().take(3) {
            let _ = reader.read_table(path, &t.name);
        }
    }
}

fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

#[test]
fn a_damaged_file_is_an_error_never_a_crash_or_a_hang() {
    common::ensure_fixtures();
    let dir = tempfile::tempdir().unwrap();
    let started = Instant::now();
    let mut problems = Vec::new();
    let mut opened = 0;
    for fixture in fixtures() {
        let rel = fixture
            .strip_prefix(common::fixture_path(""))
            .unwrap()
            .display()
            .to_string();
        let name = fixture.file_name().unwrap();
        let bytes = std::fs::read(&fixture).unwrap();
        for (what, broken) in damaged(&bytes) {
            // A folder each, under the fixture's own name: readers pick by
            // extension, and some look for files beside it.
            let folder = dir.path().join(opened.to_string());
            std::fs::create_dir(&folder).unwrap();
            let path = folder.join(name);
            std::fs::write(&path, broken).unwrap();
            opened += 1;

            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let run = std::panic::catch_unwind(|| open_like_the_gui(&path));
                let _ = tx.send(run.map_err(panic_text));
            });
            match rx.recv_timeout(TIMEOUT) {
                Ok(Ok(())) => {}
                Ok(Err(message)) => problems.push(format!("{rel} / {what}: panicked: {message}")),
                Err(_) => problems.push(format!("{rel} / {what}: no answer in {TIMEOUT:?}")),
            }
        }
    }
    eprintln!("{opened} damaged files opened in {:.1?}", started.elapsed());
    assert!(
        opened > 500,
        "only {opened} damaged files: fixtures missing?"
    );
    assert!(
        problems.is_empty(),
        "{} damaged files took a reader down:\n{}",
        problems.len(),
        problems.join("\n")
    );
}
