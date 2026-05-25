//! Cross-tab and directory grep — pure functions that walk a `DataTable`
//! (or any in-memory cell store) with a [`RowMatcher`] and emit per-cell
//! hits.
//!
//! The orchestration around this (background thread, directory walk,
//! results panel, cancellation) lives on the binary side under
//! `src/app/multi_search.rs`. This module is intentionally pure so the
//! search itself stays integration-testable.

use std::path::PathBuf;

use super::DataTable;
use super::search::RowMatcher;

/// Scope of a multi-search run. The active-tab case stays in the regular
/// search bar (`tab.search_text`), so the multi-search panel only covers
/// the cross-source modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MultiSearchScope {
    /// Walk every currently-open tab. Cheap; runs synchronously.
    #[default]
    AllOpenTabs,
    /// Walk every supported file in a chosen directory. Background
    /// thread; results stream into the panel.
    Directory,
}

impl MultiSearchScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::AllOpenTabs => "All Open Tabs",
            Self::Directory => "Directory",
        }
    }
}

/// One match returned by a multi-search run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiSearchHit {
    /// Human-readable label for the result source — tab title or file
    /// name. The panel renders this first so users can spot which file
    /// each hit came from.
    pub source_label: String,
    /// Absolute path the hit came from. `Some` for directory-scope hits
    /// (used to open the file when the user clicks a result); `None` for
    /// open-tab hits backed by an in-memory scratch tab.
    pub source_path: Option<PathBuf>,
    /// Tab index when the hit comes from an already-open tab. The panel
    /// uses this to jump straight to the existing tab instead of
    /// reopening the file.
    pub tab_idx: Option<usize>,
    /// Zero-based row index within the source `DataTable`. May be larger
    /// than the file's natural row count if the table is virtual.
    pub row: usize,
    /// Zero-based column index.
    pub col: usize,
    /// Column name as it appears in the table — handy for the panel so
    /// the user sees `email` instead of just "col 4".
    pub column_name: String,
    /// Snippet of the matching cell, truncated to a sensible width. The
    /// match position is preserved at the start of the snippet when we
    /// can find it; otherwise we fall back to `text[..max_chars]`.
    pub snippet: String,
}

/// Visit every cell in `table` and push a [`MultiSearchHit`] for each one
/// the matcher accepts. The order is row-major, column-by-column inside
/// each row — same order the table view paints them.
///
/// `tab_idx` and `source_path` are passed through verbatim onto the
/// resulting hits.
pub fn search_table(
    table: &DataTable,
    matcher: &RowMatcher,
    source_label: &str,
    source_path: Option<PathBuf>,
    tab_idx: Option<usize>,
    max_snippet_chars: usize,
) -> Vec<MultiSearchHit> {
    let mut hits = Vec::new();
    let col_count = table.col_count();
    let row_count = table.row_count();
    for row in 0..row_count {
        for col in 0..col_count {
            let Some(v) = table.get(row, col) else {
                continue;
            };
            let text = v.to_string();
            if !matcher.matches(&text) {
                continue;
            }
            let column_name = table
                .columns
                .get(col)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            hits.push(MultiSearchHit {
                source_label: source_label.to_string(),
                source_path: source_path.clone(),
                tab_idx,
                row,
                col,
                column_name,
                snippet: snippet(&text, matcher, max_snippet_chars),
            });
        }
    }
    hits
}

/// Cap a long cell value for display. When we can locate the match
/// inside `text` cheaply (Plain mode → case-insensitive `find`), the
/// snippet starts a few chars before the match so the relevant context
/// stays visible. Otherwise the head of the string is returned. The
/// returned string is always at most `max_chars` graphemes long (chars,
/// in practice — egui labels render fine).
pub fn snippet(text: &str, matcher: &RowMatcher, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.replace(['\n', '\r', '\t'], " ");
    }
    // Try to anchor on the match for Plain mode. Regex would need a
    // re.find(); skip the extra work for the simple path and fall back
    // to head truncation for Regex/Wildcard.
    let anchor_byte = match matcher {
        RowMatcher::Plain(q) => find_ci(text, q),
        _ => None,
    };
    let start_char: usize = anchor_byte
        .map(|byte| {
            // Convert byte offset to char offset so we can reason in char counts.
            let mut chars = 0usize;
            for (idx, _) in text.char_indices() {
                if idx >= byte {
                    break;
                }
                chars += 1;
            }
            chars
        })
        .map(|c| c.saturating_sub(20))
        .unwrap_or(0);
    let take = max_chars.saturating_sub(if start_char == 0 { 1 } else { 2 });
    let mut out = String::new();
    if start_char > 0 {
        out.push('…');
    }
    for (i, ch) in text.chars().enumerate() {
        if i < start_char {
            continue;
        }
        if i >= start_char + take {
            break;
        }
        let ch = match ch {
            '\n' | '\r' | '\t' => ' ',
            other => other,
        };
        out.push(ch);
    }
    if start_char + take < char_count {
        out.push('…');
    }
    out
}

fn find_ci(haystack: &str, needle_lc: &str) -> Option<usize> {
    if needle_lc.is_empty() {
        return Some(0);
    }
    let h = haystack.to_lowercase();
    h.find(needle_lc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{CellValue, ColumnInfo, DataTable, SearchMode};

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
    fn search_plain_lowercases() {
        let table = table_with(
            &[&["alice", "ENG"], &["bob", "QA"], &["Eve", "eng"]],
            &["name", "team"],
        );
        let matcher = RowMatcher::new("eng", SearchMode::Plain);
        let hits = search_table(&table, &matcher, "in-mem", None, None, 80);
        // alice → ENG (col 1), Eve → eng (col 1).
        assert_eq!(hits.len(), 2, "got hits = {hits:?}");
        assert_eq!(hits[0].row, 0);
        assert_eq!(hits[0].col, 1);
        assert_eq!(hits[1].row, 2);
    }

    #[test]
    fn snippet_anchors_around_match() {
        let long = "x".repeat(60) + "needle" + &"y".repeat(40);
        let matcher = RowMatcher::new("needle", SearchMode::Plain);
        let s = snippet(&long, &matcher, 40);
        assert!(s.contains("needle"));
        assert!(s.starts_with('…') || s.starts_with('x'));
        // 40-char cap (we allow ±1 for the leading/trailing ellipses).
        assert!(s.chars().count() <= 41, "snippet too long: {s:?}");
    }
}
