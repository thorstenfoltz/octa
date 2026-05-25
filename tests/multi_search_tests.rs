//! Multi-search (F6): cross-tab + directory grep.
//!
//! Covers the pure cell-walker in `octa::data::multi_search`. The
//! directory worker on the binary side reuses the same algorithm — the
//! integration boundary is the format registry, which is exercised by
//! the format-specific test suites.

use std::path::PathBuf;

use octa::data::multi_search::{MultiSearchScope, search_table, snippet};
use octa::data::search::RowMatcher;
use octa::data::{CellValue, ColumnInfo, DataTable, SearchMode};

fn table_with(rows: &[&[&str]], cols: &[&str]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    t.rows = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|s| CellValue::String((*s).to_string()))
                .collect()
        })
        .collect();
    t
}

#[test]
fn multi_search_scope_labels_are_distinct() {
    assert_eq!(MultiSearchScope::AllOpenTabs.label(), "All Open Tabs");
    assert_eq!(MultiSearchScope::Directory.label(), "Directory");
    assert_eq!(MultiSearchScope::default(), MultiSearchScope::AllOpenTabs);
}

#[test]
fn search_table_walks_row_major_and_captures_column_names() {
    let table = table_with(
        &[
            &["alice@example.com", "engineer"],
            &["bob@hotmail.com", "qa"],
            &["eve@example.com", "engineer"],
        ],
        &["email", "team"],
    );
    let matcher = RowMatcher::new("example", SearchMode::Plain);
    let hits = search_table(&table, &matcher, "tab1", None, Some(0), 80);
    assert_eq!(hits.len(), 2, "got hits = {hits:?}");
    assert_eq!(hits[0].row, 0);
    assert_eq!(hits[0].col, 0);
    assert_eq!(hits[0].column_name, "email");
    assert_eq!(hits[0].tab_idx, Some(0));
    assert_eq!(hits[1].row, 2);
    assert_eq!(hits[1].column_name, "email");
}

#[test]
fn search_table_handles_regex_mode() {
    let table = table_with(
        &[
            &["2024-01-02", "ok"],
            &["2025-02-03", "warn"],
            &["unknown", "skip"],
        ],
        &["when", "status"],
    );
    let re = RowMatcher::new(r"^\d{4}-\d{2}-\d{2}$", SearchMode::Regex);
    let hits = search_table(&table, &re, "t", None, None, 40);
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|h| h.col == 0));
}

#[test]
fn search_table_ignores_invalid_regex_via_matcher() {
    // RowMatcher::Invalid never matches; cheap protection for the
    // panel against a broken regex producing a crash.
    let table = table_with(&[&["foo"], &["bar"]], &["c"]);
    let bad = RowMatcher::new("(", SearchMode::Regex);
    let hits = search_table(&table, &bad, "t", None, None, 40);
    assert!(hits.is_empty(), "Invalid matcher should yield no hits");
}

#[test]
fn search_table_streams_results_across_tabs() {
    // Simulates the All-Open-Tabs scope: search several DataTables and
    // expect hits to be deterministic, ordered by (tab, row, col).
    let t1 = table_with(&[&["needle1"], &["miss"]], &["v"]);
    let t2 = table_with(&[&["miss"], &["needle2"], &["miss"]], &["v"]);
    let t3 = table_with(&[&["miss"]], &["v"]);
    let matcher = RowMatcher::new("needle", SearchMode::Plain);
    let mut all = Vec::new();
    for (idx, tab) in [&t1, &t2, &t3].iter().enumerate() {
        all.extend(search_table(
            tab,
            &matcher,
            &format!("tab{}", idx + 1),
            None,
            Some(idx),
            40,
        ));
    }
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].tab_idx, Some(0));
    assert_eq!(all[0].row, 0);
    assert_eq!(all[1].tab_idx, Some(1));
    assert_eq!(all[1].row, 1);
}

#[test]
fn search_table_attaches_source_path_for_directory_results() {
    let table = table_with(&[&["alpha"], &["beta"]], &["v"]);
    let matcher = RowMatcher::new("alpha", SearchMode::Plain);
    let path = PathBuf::from("/tmp/test.csv");
    let hits = search_table(&table, &matcher, "test.csv", Some(path.clone()), None, 80);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_path.as_deref(), Some(path.as_path()));
    assert_eq!(hits[0].tab_idx, None);
}

#[test]
fn snippet_short_text_is_returned_verbatim_with_whitespace_normalized() {
    let matcher = RowMatcher::new("x", SearchMode::Plain);
    let s = snippet("hello\nworld\tfoo", &matcher, 80);
    // Whitespace inside the snippet is collapsed so a single-line
    // result line stays readable.
    assert_eq!(s, "hello world foo");
}

#[test]
fn snippet_anchors_around_plain_match() {
    let long = "x".repeat(60) + "needle" + &"y".repeat(40);
    let matcher = RowMatcher::new("needle", SearchMode::Plain);
    let s = snippet(&long, &matcher, 40);
    assert!(s.contains("needle"), "snippet should keep match: {s:?}");
    // Should have either a leading ellipsis (anchored) or start with x.
    assert!(s.starts_with('…') || s.starts_with('x'));
    assert!(s.chars().count() <= 41);
}

#[test]
fn snippet_falls_back_to_head_for_regex_mode() {
    let long = "AAAAAAAAAA".repeat(20); // 200 chars
    let matcher = RowMatcher::new("A+", SearchMode::Regex);
    let s = snippet(&long, &matcher, 30);
    // For non-plain matchers we don't anchor — we just truncate from
    // the start. Expect leading char to be 'A' and a trailing ellipsis.
    assert!(s.starts_with('A'));
    assert!(s.ends_with('…'));
    assert!(s.chars().count() <= 30);
}

#[test]
fn search_table_empty_table_returns_empty() {
    let table = DataTable::empty();
    let matcher = RowMatcher::new("anything", SearchMode::Plain);
    let hits = search_table(&table, &matcher, "empty", None, None, 80);
    assert!(hits.is_empty());
}

#[test]
fn search_table_skips_oversized_files_simulation() {
    // The directory worker calls `search_table` once per file that
    // passed the size gate. Files over the cap are filtered *before*
    // we ever reach the matcher — there's no per-cell oversize gate.
    // This test documents that contract: given a small table, every
    // matching cell becomes a hit (no implicit cap inside the walker).
    let table = table_with(
        &[&["needle a"], &["miss"], &["needle b"], &["needle c"]],
        &["v"],
    );
    let matcher = RowMatcher::new("needle", SearchMode::Plain);
    let hits = search_table(&table, &matcher, "t", None, None, 80);
    assert_eq!(hits.len(), 3);
}

#[test]
fn search_table_returns_hits_sorted_row_major() {
    let table = table_with(
        &[&["needle", "no"], &["no", "needle"], &["needle", "needle"]],
        &["a", "b"],
    );
    let matcher = RowMatcher::new("needle", SearchMode::Plain);
    let hits = search_table(&table, &matcher, "t", None, None, 40);
    let coords: Vec<(usize, usize)> = hits.iter().map(|h| (h.row, h.col)).collect();
    assert_eq!(
        coords,
        vec![(0, 0), (1, 1), (2, 0), (2, 1)],
        "hits should be row-major then column-major"
    );
}
