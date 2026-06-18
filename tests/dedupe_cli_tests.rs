//! Integration tests for `octa --dedupe FILE [--dedupe-on ...] [--dedupe-keep ...]`.

use std::io::Write;
use std::process::Command;

fn octa_bin() -> &'static str {
    env!("CARGO_BIN_EXE_octa")
}

fn write_csv(dir: &std::path::Path) -> std::path::PathBuf {
    let csv_path = dir.join("rows.csv");
    let mut f = std::fs::File::create(&csv_path).unwrap();
    writeln!(f, "id,city").unwrap();
    writeln!(f, "1,Berlin").unwrap();
    writeln!(f, "1,Berlin").unwrap(); // exact duplicate of row 1
    writeln!(f, "2,Berlin").unwrap(); // same city, different id
    drop(f);
    csv_path
}

#[test]
fn dedupe_whole_row_removes_exact_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    let csv_path = write_csv(dir.path());

    let out = Command::new(octa_bin())
        .arg("--dedupe")
        .arg(&csv_path)
        .args(["-f", "csv"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    // Header + the two distinct rows (one exact duplicate dropped).
    assert_eq!(lines[0], "id,city");
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[1], "1,Berlin");
    assert_eq!(lines[2], "2,Berlin");
    // Summary on stderr.
    assert!(String::from_utf8_lossy(&out.stderr).contains("1 duplicate"));
}

#[test]
fn dedupe_on_column_keeps_first() {
    let dir = tempfile::tempdir().unwrap();
    let csv_path = write_csv(dir.path());

    // Keying only on `city` collapses all three rows (all Berlin) to one.
    let out = Command::new(octa_bin())
        .arg("--dedupe")
        .arg(&csv_path)
        .args(["--dedupe-on", "city", "--dedupe-keep", "first", "-f", "csv"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2); // header + first Berlin row
    assert_eq!(lines[1], "1,Berlin");
}

#[test]
fn dedupe_on_column_keeps_last() {
    let dir = tempfile::tempdir().unwrap();
    let csv_path = write_csv(dir.path());

    let out = Command::new(octa_bin())
        .arg("--dedupe")
        .arg(&csv_path)
        .args(["--dedupe-on", "city", "--dedupe-keep", "last", "-f", "csv"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1], "2,Berlin"); // last Berlin row survives
}

#[test]
fn dedupe_unknown_column_errors() {
    let dir = tempfile::tempdir().unwrap();
    let csv_path = write_csv(dir.path());
    let out = Command::new(octa_bin())
        .arg("--dedupe")
        .arg(&csv_path)
        .args(["--dedupe-on", "nope"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope"));
}
