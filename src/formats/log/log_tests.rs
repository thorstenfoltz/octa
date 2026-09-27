//! Unit tests for the log formats and reader. Included via `#[path]`.

use super::*;
use crate::data::CellValue;

const CTX: ParseCtx = ParseCtx { year: 2026 };

fn field<'a>(e: &'a LogEntry, name: &str) -> &'a CellValue {
    &e.fields
        .iter()
        .find(|(k, _)| k == name)
        .unwrap_or_else(|| panic!("no field {name}"))
        .1
}

fn parse(format: &dyn LogFormat, line: &str) -> LogEntry {
    format
        .parse_line(line, &CTX)
        .unwrap_or_else(|| panic!("{} did not parse {line}", format.name()))
}

#[test]
fn apache_combined_splits_the_request_and_keeps_the_offset() {
    let e = parse(
        &apache::Combined,
        r#"203.0.113.9 - frank [10/Oct/2026:13:55:36 -0700] "GET /a.png HTTP/1.1" 200 2326 "http://x/" "Mozilla/5.0""#,
    );
    assert_eq!(e.timestamp.as_deref(), Some("2026-10-10 13:55:36"));
    assert_eq!(e.utc_offset.as_deref(), Some("-07:00"));
    assert_eq!(field(&e, "method"), &CellValue::String("GET".into()));
    assert_eq!(field(&e, "status"), &CellValue::Int(200));
    assert_eq!(
        field(&e, "user_agent"),
        &CellValue::String("Mozilla/5.0".into())
    );
    assert!(apache::Common.parse_line(r#"203.0.113.9 - frank [10/Oct/2026:13:55:36 -0700] "GET /a.png HTTP/1.1" 200 2326 "http://x/" "Mozilla/5.0""#, &CTX).is_none());
}

#[test]
fn apache_common_treats_dash_bytes_as_empty() {
    let e = parse(
        &apache::Common,
        r#"::1 - - [10/Oct/2026:13:55:36 +0000] "GET / HTTP/1.0" 304 -"#,
    );
    assert_eq!(field(&e, "bytes"), &CellValue::Null);
    assert_eq!(field(&e, "user"), &CellValue::Null);
}

#[test]
fn syslog_3164_takes_the_year_from_the_context_and_level_from_pri() {
    let e = parse(
        &syslog::Rfc3164,
        "<11>Oct  3 22:14:15 web1 sshd[4721]: Failed password for root",
    );
    assert_eq!(e.timestamp.as_deref(), Some("2026-10-03 22:14:15"));
    assert_eq!(e.level.as_deref(), Some("ERROR"));
    assert_eq!(field(&e, "app"), &CellValue::String("sshd".into()));
    assert_eq!(field(&e, "pid"), &CellValue::Int(4721));
    assert_eq!(e.message, "Failed password for root");
}

#[test]
fn syslog_5424_reads_the_iso_timestamp() {
    let e = parse(
        &syslog::Rfc5424,
        "<165>1 2026-10-11T22:14:15.003Z host app 42 ID47 - message here",
    );
    assert_eq!(e.timestamp.as_deref(), Some("2026-10-11 22:14:15.003"));
    assert_eq!(e.utc_offset.as_deref(), Some("+00:00"));
    assert_eq!(e.level.as_deref(), Some("INFO"));
    assert_eq!(e.message, "message here");
}

#[test]
fn logfmt_needs_the_whole_line_to_be_pairs() {
    let e = parse(
        &logfmt::Logfmt,
        r#"time=2026-09-25T10:00:00Z level=warn msg="disk almost full" free_mb=512"#,
    );
    assert_eq!(e.level.as_deref(), Some("WARN"));
    assert_eq!(e.message, "disk almost full");
    assert_eq!(field(&e, "free_mb"), &CellValue::Int(512));
    assert!(
        logfmt::Logfmt
            .parse_line("just some words a=b", &CTX)
            .is_none()
    );
}

#[test]
fn json_lines_maps_known_keys_and_keeps_the_rest() {
    let e = parse(
        &json_lines::JsonLines,
        r#"{"ts":1758794400,"severity":"error","message":"boom","user":{"id":7}}"#,
    );
    assert_eq!(e.timestamp.as_deref(), Some("2025-09-25 10:00:00"));
    assert_eq!(e.level.as_deref(), Some("ERROR"));
    assert!(matches!(field(&e, "user"), CellValue::Nested(_)));
}

#[test]
fn timestamped_text_reads_java_and_python_style_lines() {
    let e = parse(
        &timestamped::Timestamped,
        "2026-09-25 10:00:01,123 ERROR [main] c.a.Service - failed",
    );
    assert_eq!(e.timestamp.as_deref(), Some("2026-09-25 10:00:01.123"));
    assert_eq!(e.level.as_deref(), Some("ERROR"));
    assert_eq!(e.message, "[main] c.a.Service - failed");
}

#[test]
fn levels_normalise_and_continuations_are_recognised() {
    for (raw, want) in [
        ("warning", "WARN"),
        ("W", "WARN"),
        ("crit", "FATAL"),
        ("Information", "INFO"),
        ("odd", "ODD"),
    ] {
        assert_eq!(normalise_level(raw), want);
    }
    assert!(is_continuation("\tat com.a.B.run(B.java:10)"));
    assert!(is_continuation("Caused by: java.io.IOException"));
    assert!(is_continuation("Traceback (most recent call last):"));
    assert!(!is_continuation("2026-09-25 10:00:01 INFO ok"));
}

#[test]
fn detection_picks_the_best_format_and_refuses_prose() {
    let nginx = [
        r#"1.2.3.4 - - [10/Oct/2026:13:55:36 +0000] "GET / HTTP/1.1" 200 12 "-" "curl""#,
        r#"1.2.3.5 - - [10/Oct/2026:13:55:37 +0000] "GET /x HTTP/1.1" 404 0 "-" "curl""#,
    ];
    assert_eq!(
        reader::detect(&nginx, &CTX, false).unwrap().name(),
        "Apache/nginx combined"
    );
    let prose = [
        "Dear diary,",
        "today was fine.",
        "at least 2026-09-25 10:00:00 was.",
    ];
    assert!(reader::detect(&prose, &CTX, false).is_none());
}

#[test]
fn stack_traces_join_their_entry_and_strays_keep_raw() {
    let lines = [
        "2026-09-25 10:00:01 ERROR boom",
        "\tat com.a.B.run(B.java:10)",
        "Caused by: java.io.IOException",
        "an unrelated stray line",
        "2026-09-25 10:00:02 INFO fine",
    ];
    let t = reader::build_table(&lines, &timestamped::Timestamped, &CTX, usize::MAX);
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["timestamp", "level", "message", "raw"]);
    assert_eq!(t.rows.len(), 3);
    assert_eq!(
        t.rows[0][2],
        CellValue::String(
            "boom\n\tat com.a.B.run(B.java:10)\nCaused by: java.io.IOException".into()
        )
    );
    assert_eq!(
        t.rows[1][3],
        CellValue::String("an unrelated stray line".into())
    );
    assert_eq!(t.rows[1][0], CellValue::Null);
}

#[test]
fn the_cap_keeps_the_first_rows_and_records_the_total() {
    let lines = [
        "2026-09-25 10:00:01 INFO a",
        "2026-09-25 10:00:02 INFO b",
        "2026-09-25 10:00:03 INFO c",
    ];
    let t = reader::build_table(&lines, &timestamped::Timestamped, &CTX, 2);
    assert_eq!(t.rows.len(), 2);
    assert_eq!(t.total_rows, Some(3));
}

#[test]
fn a_log_of_notes_opens_as_text() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("notes.log");
    std::fs::write(&p, "remember the milk\ncall Sam\n").unwrap();
    let t = crate::formats::read_table_auto(&p, None, u64::MAX).unwrap();
    assert_eq!(t.format_name.as_deref(), Some("Text"));
    let q = dir.path().join("access.log.1");
    std::fs::write(
        &q,
        "::1 - - [10/Oct/2026:13:55:36 +0000] \"GET / HTTP/1.0\" 200 5\n",
    )
    .unwrap();
    let t = crate::formats::read_table_auto(&q, None, u64::MAX).unwrap();
    assert_eq!(t.format_name.as_deref(), Some("Log"));
}
