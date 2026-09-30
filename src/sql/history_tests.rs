//! Tests for the pure half of the query history: ordering, de-duplication and
//! the cap. The disk half is a read and a write of one JSON file.

use super::*;

fn entry(query: &str, rows: usize) -> SqlHistoryEntry {
    SqlHistoryEntry {
        query: query.to_string(),
        at_unix: 1_700_000_000,
        duration_ms: 12,
        rows,
        source: "scratch".to_string(),
    }
}

#[test]
fn the_most_recent_query_is_first() {
    let mut e = Vec::new();
    fold(&mut e, entry("SELECT 1", 1), 20);
    fold(&mut e, entry("SELECT 2", 2), 20);
    assert_eq!(e[0].query, "SELECT 2");
    assert_eq!(e[1].query, "SELECT 1");
}

#[test]
fn re_running_a_query_moves_it_up_instead_of_duplicating() {
    // The list answers "what have I run", not "how often": three copies of the
    // same SELECT would push the other queries out of a 20-entry cap.
    let mut e = Vec::new();
    fold(&mut e, entry("SELECT a", 1), 20);
    fold(&mut e, entry("SELECT b", 2), 20);
    fold(&mut e, entry("SELECT a", 9), 20);
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].query, "SELECT a");
    // ...and it carries the *new* run's numbers, not the first run's.
    assert_eq!(e[0].rows, 9);
}

#[test]
fn the_cap_drops_the_oldest() {
    let mut e = Vec::new();
    for i in 0..25 {
        fold(&mut e, entry(&format!("SELECT {i}"), i), 20);
    }
    assert_eq!(e.len(), 20);
    assert_eq!(e[0].query, "SELECT 24");
    assert_eq!(e[19].query, "SELECT 5");
}

#[test]
fn a_limit_of_zero_means_unlimited() {
    // Same convention as `chat_result_row_limit`, so a user who has learned it
    // once does not have to learn it again.
    let mut e = Vec::new();
    for i in 0..50 {
        fold(&mut e, entry(&format!("SELECT {i}"), i), 0);
    }
    assert_eq!(e.len(), 50);
}

#[test]
fn blank_queries_are_not_recorded() {
    let mut e = Vec::new();
    assert!(!fold(&mut e, entry("   \n  ", 0), 20));
    assert!(e.is_empty());
}

#[test]
fn a_query_is_stored_trimmed_so_whitespace_is_not_a_new_entry() {
    let mut e = Vec::new();
    fold(&mut e, entry("SELECT 1", 1), 20);
    fold(&mut e, entry("  SELECT 1\n", 1), 20);
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(e[0].query, "SELECT 1");
}

#[test]
fn the_same_query_on_two_sources_is_two_entries() {
    // One shared list, but "SELECT 1 on prod" and "SELECT 1 on a CSV" are
    // different things to have run.
    let mut e = Vec::new();
    fold(&mut e, entry("SELECT 1", 1), 20);
    let mut other = entry("SELECT 1", 1);
    other.source = db_scope("prod");
    fold(&mut e, other, 20);
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].source, "db:prod");
}

#[test]
fn a_per_source_history_file_is_flattened_newest_first() {
    // The file an earlier build wrote: one list per source.
    let old = r#"{
        "db:prod": [{"query":"SELECT a","at_unix":10}],
        "file:/x.csv": [{"query":"SELECT b","at_unix":30},{"query":"SELECT c","at_unix":5}]
    }"#;
    let e = parse(old);
    let got: Vec<(&str, &str)> = e
        .iter()
        .map(|e| (e.query.as_str(), e.source.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("SELECT b", "file:/x.csv"),
            ("SELECT a", "db:prod"),
            ("SELECT c", "file:/x.csv")
        ]
    );
    // And the new shape reads back as written.
    let text = serde_json::to_string(&e).unwrap();
    assert_eq!(parse(&text), e);
}

#[test]
fn an_entry_saved_before_the_timings_existed_still_loads() {
    // Back-compat for a history file written by an earlier build: the extra
    // fields default rather than failing the whole load.
    let e: SqlHistoryEntry = serde_json::from_str(r#"{"query":"SELECT 1"}"#).unwrap();
    assert_eq!(e.query, "SELECT 1");
    assert_eq!(e.rows, 0);
    assert_eq!(e.duration_ms, 0);
}
