//! Unit tests for [`search_highlight`](search_highlight). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::data::{CellValue, ColumnInfo, SearchMode};
use eframe::egui::TextFormat;

fn ranges(query: &str, mode: SearchMode, text: &str) -> Vec<Range<usize>> {
    RowMatcher::new(query, mode).find_ranges(text)
}

#[test]
fn plain_ranges_ascii_and_case_insensitive() {
    assert_eq!(ranges("ab", SearchMode::Plain, "abXAB"), vec![0..2, 3..5]);
}

#[test]
fn plain_ranges_multibyte() {
    // "café" - the 'é' is two bytes; matching "fé" must land on char
    // boundaries (bytes 2..5: 'f' is 1 byte, 'é' is 2).
    let text = "café";
    let r = ranges("fé", SearchMode::Plain, text);
    assert_eq!(r, vec![2..5]);
    // Slicing on the returned range must not panic.
    assert_eq!(&text[r[0].clone()], "fé");
}

#[test]
fn regex_ranges_and_zero_width_skipped() {
    // `a*` would match empty strings everywhere; those are skipped.
    assert_eq!(ranges("a+", SearchMode::Regex, "baaab"), vec![1..4]);
    assert!(ranges("a*", SearchMode::Regex, "bbb").is_empty());
}

#[test]
fn empty_and_invalid_query_yield_no_ranges() {
    assert!(ranges("", SearchMode::Plain, "abc").is_empty());
    assert!(ranges("(", SearchMode::Regex, "abc").is_empty());
}

fn simple_job(text: &str) -> LayoutJob {
    LayoutJob::single_section(text.to_string(), TextFormat::default())
}

#[test]
fn apply_highlight_splits_and_colours() {
    let normal = Color32::from_rgb(10, 20, 30);
    let active = Color32::from_rgb(200, 100, 50);
    let mut job = simple_job("abXAB");
    let rs = ranges("ab", SearchMode::Plain, "abXAB");
    apply_highlight(&mut job, &rs, Some(&rs[1]), normal, active);
    // Runs: [0..2 match-normal] [2..3 plain] [3..5 match-active]
    assert_eq!(job.sections.len(), 3);
    assert_eq!(job.sections[0].byte_range, 0..2);
    assert_eq!(job.sections[0].format.background, normal);
    assert_eq!(job.sections[1].byte_range, 2..3);
    assert_eq!(job.sections[1].format.background, Color32::TRANSPARENT);
    assert_eq!(job.sections[2].byte_range, 3..5);
    assert_eq!(job.sections[2].format.background, active);
}

#[test]
fn apply_highlight_no_ranges_is_noop() {
    let mut job = simple_job("hello");
    let before = job.sections.len();
    apply_highlight(&mut job, &[], None, Color32::RED, Color32::BLUE);
    assert_eq!(job.sections.len(), before);
    assert_eq!(job.sections[0].format.background, Color32::TRANSPARENT);
}

#[test]
fn cell_matches_walks_display_order() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "a".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "b".into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = vec![
        vec![
            CellValue::String("foo".into()),
            CellValue::String("bar".into()),
        ],
        vec![
            CellValue::String("baz".into()),
            CellValue::String("foo".into()),
        ],
    ];
    let m = RowMatcher::new("foo", SearchMode::Plain);
    let rows: Vec<usize> = vec![0, 1];
    assert_eq!(cell_matches(&t, &m, &rows, 2, None), vec![(0, 0), (1, 1)]);
}
