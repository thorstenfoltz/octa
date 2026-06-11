//! Reusable search-highlight engine shared by every view that can highlight
//! search matches in place (table cells, raw/notebook text editors, JSON/YAML
//! tree nodes, Markdown preview, the Documentation dialog).
//!
//! The text path works by post-processing an already-built [`LayoutJob`]:
//! [`apply_highlight`] splits the job's sections at match boundaries and sets a
//! background colour on the covered runs. Because it operates on a finished
//! job it composes with any layouter (plain, syntect, column-coloured) without
//! the caller having to know how the job was produced.

use std::ops::Range;

use eframe::egui::{
    Color32,
    text::{LayoutJob, LayoutSection},
};

use crate::data::DataTable;
use crate::data::search::RowMatcher;
use crate::ui::theme::ThemeColors;

/// Highlight backgrounds derived from the active theme: `(normal, active)`.
///
/// `normal` paints every match; `active` paints the one the user has navigated
/// to. Both are translucent so the underlying text colour and (in the table)
/// any alternating-row tint stay legible.
pub fn highlight_colors(colors: &ThemeColors) -> (Color32, Color32) {
    let w = colors.warning;
    let a = colors.accent;
    (
        Color32::from_rgba_unmultiplied(w.r(), w.g(), w.b(), 110),
        Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 190),
    )
}

/// Split `job`'s sections at the boundaries of `ranges` (byte offsets into the
/// laid-out text) and set a background on the covered runs. The run matching
/// `current` (when supplied) gets `active`; all other matches get `normal`.
///
/// `ranges` and `current` must be valid char boundaries of the same text the
/// job was built from. A no-op when `ranges` is empty, so callers can pass it
/// unconditionally.
pub fn apply_highlight(
    job: &mut LayoutJob,
    ranges: &[Range<usize>],
    current: Option<&Range<usize>>,
    normal: Color32,
    active: Color32,
) {
    if ranges.is_empty() {
        return;
    }
    let old = std::mem::take(&mut job.sections);
    for section in old {
        let (s, e) = (section.byte_range.start, section.byte_range.end);
        // Cut points: section ends plus every range edge that falls strictly
        // inside the section. Sorted + deduped, consecutive pairs are the
        // sub-runs to emit.
        let mut cuts: Vec<usize> = vec![s, e];
        for r in ranges {
            if r.start > s && r.start < e {
                cuts.push(r.start);
            }
            if r.end > s && r.end < e {
                cuts.push(r.end);
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a >= b {
                continue;
            }
            let in_current = current.is_some_and(|c| c.start <= a && b <= c.end);
            let in_any = ranges.iter().any(|r| r.start <= a && b <= r.end);
            let mut fmt = section.format.clone();
            if in_current {
                fmt.background = active;
            } else if in_any {
                fmt.background = normal;
            }
            job.sections.push(LayoutSection {
                // Preserve the section's leading space only on its first run.
                leading_space: if a == s { section.leading_space } else { 0.0 },
                byte_range: a..b,
                format: fmt,
            });
        }
    }
}

/// Ordered (data-row, column) coordinates of every cell whose stringified value
/// matches, walked in display order (top-to-bottom over `rows`, left-to-right
/// over columns). Used by the table highlight paint and next/previous jump.
pub fn cell_matches(
    table: &DataTable,
    matcher: &RowMatcher,
    rows: &[usize],
    col_count: usize,
    scope_col: Option<usize>,
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    // Honour a single-column search scope; otherwise scan every column.
    let (lo, hi) = match scope_col {
        Some(c) if c < col_count => (c, c + 1),
        _ => (0, col_count),
    };
    for &row in rows {
        for col in lo..hi {
            if let Some(v) = table.get(row, col)
                && matcher.matches(&v.to_string())
            {
                out.push((row, col));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
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
}
