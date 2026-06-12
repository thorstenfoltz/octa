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
#[path = "search_highlight_tests.rs"]
mod tests;
