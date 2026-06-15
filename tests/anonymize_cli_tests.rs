//! Integration test for `octa --anonymize SPEC FILE`.

use std::io::Write;
use std::process::Command;

fn octa_bin() -> &'static str {
    env!("CARGO_BIN_EXE_octa")
}

#[test]
fn anonymize_hashes_named_column() {
    let dir = tempfile::tempdir().unwrap();

    // Input CSV: two rows, the email column has a duplicate so we can assert
    // duplicates map to the same hash.
    let csv_path = dir.path().join("people.csv");
    let mut f = std::fs::File::create(&csv_path).unwrap();
    writeln!(f, "name,email").unwrap();
    writeln!(f, "Alice,a@x.com").unwrap();
    writeln!(f, "Bob,a@x.com").unwrap();
    drop(f);

    let spec_path = dir.path().join("spec.json");
    std::fs::write(
        &spec_path,
        r#"{
          "salt": "s3cr",
          "rules": [
            { "columns": ["email"], "strategy": { "type": "hash", "algo": "sha256", "length": 12 } }
          ]
        }"#,
    )
    .unwrap();

    let out = Command::new(octa_bin())
        .arg("--anonymize")
        .arg(&spec_path)
        .arg(&csv_path)
        .arg("-f")
        .arg("csv")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();

    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "name,email");
    // The two email cells were identical -> same 12-char hash.
    let email_a = lines[1].split(',').nth(1).unwrap();
    let email_b = lines[2].split(',').nth(1).unwrap();
    assert_eq!(email_a, email_b);
    assert_eq!(email_a.len(), 12);
    assert_ne!(email_a, "a@x.com");
    // The untouched name column is unchanged.
    assert!(lines[1].starts_with("Alice,"));
}

#[test]
fn anonymize_unknown_column_errors() {
    let dir = tempfile::tempdir().unwrap();
    let csv_path = dir.path().join("people.csv");
    std::fs::write(&csv_path, "name\nAlice\n").unwrap();
    let spec_path = dir.path().join("spec.json");
    std::fs::write(
        &spec_path,
        r#"{ "rules": [ { "columns": "nope", "strategy": { "type": "redact" } } ] }"#,
    )
    .unwrap();
    let out = Command::new(octa_bin())
        .arg("--anonymize")
        .arg(&spec_path)
        .arg(&csv_path)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope"));
}

#[test]
fn anonymize_combined_hash_into_new_column() {
    let dir = tempfile::tempdir().unwrap();
    let csv_path = dir.path().join("people.csv");
    std::fs::write(&csv_path, "first,last\nJohn,Smith\nJane,Smith\n").unwrap();
    let spec_path = dir.path().join("spec.json");
    std::fs::write(
        &spec_path,
        r#"{ "salt":"s", "output":"new_columns",
             "rules":[ { "columns":["first","last"], "new_column":"person_id",
                         "strategy":{ "type":"hash", "algo":"sha256", "length":16 } } ] }"#,
    )
    .unwrap();
    let out = Command::new(octa_bin())
        .arg("--anonymize")
        .arg(&spec_path)
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
    // Originals kept + a new person_id column.
    assert_eq!(lines[0], "first,last,person_id");
    let id1 = lines[1].split(',').nth(2).unwrap();
    let id2 = lines[2].split(',').nth(2).unwrap();
    assert_eq!(id1.len(), 16);
    assert_ne!(id1, id2); // Jane vs John
}
