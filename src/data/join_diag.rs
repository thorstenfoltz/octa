//! Explain why two key columns fail to join.
//!
//! [`join_keys`](crate::data::join_keys) suggests *which* columns to join on
//! and [`join`](crate::data::join) executes the join, but nothing tells you why
//! a join you already believe in returns far fewer rows than expected. That is
//! the most common dead end in joining two real-world extracts, and it almost
//! always has a boring cause: a trailing space, a case difference, an ID that
//! lost its leading zeros on the way through a spreadsheet.
//!
//! This diagnoses and nothing else. It never mutates either table; the fixes it
//! reports are advice for the user to act on.
//!
//! It also invents no new string maths: every suggested fix is the same
//! containment count recomputed with one normalisation applied, and the text
//! normalisations come straight from
//! [`fuzzy_duplicates::normalize`](crate::data::fuzzy_duplicates::normalize).

use crate::data::fuzzy_duplicates::{NormalizeOpts, normalize};
use crate::data::{CellValue, DataTable};
use std::collections::HashSet;

/// Unmatched example values kept per side.
pub const MAX_SAMPLES: usize = 5;

/// A normalisation that would increase the number of matching keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixKind {
    TrimWhitespace,
    IgnoreCase,
    CollapseWhitespace,
    StripPunctuation,
    StripLeadingZeros,
}

impl FixKind {
    /// i18n key for the user-facing sentence.
    pub fn i18n_key(self) -> &'static str {
        match self {
            FixKind::TrimWhitespace => "joindiag.fix_trim",
            FixKind::IgnoreCase => "joindiag.fix_case",
            FixKind::CollapseWhitespace => "joindiag.fix_collapse",
            FixKind::StripPunctuation => "joindiag.fix_punct",
            FixKind::StripLeadingZeros => "joindiag.fix_zeros",
        }
    }

    /// Stable identifier for CLI / MCP output. Never localized.
    pub fn id(self) -> &'static str {
        match self {
            FixKind::TrimWhitespace => "trim_whitespace",
            FixKind::IgnoreCase => "ignore_case",
            FixKind::CollapseWhitespace => "collapse_whitespace",
            FixKind::StripPunctuation => "strip_punctuation",
            FixKind::StripLeadingZeros => "strip_leading_zeros",
        }
    }
}

/// One normalisation and how much it would help.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestedFix {
    pub kind: FixKind,
    /// Distinct left keys that would find a partner under this normalisation.
    /// Only reported when strictly greater than the current match count.
    pub would_match: usize,
}

/// The full diagnosis. Counts are over *distinct* key values, not rows: a join
/// failing on 3 distinct IDs is one problem, however many rows carry them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JoinDiagnosis {
    pub left_rows: usize,
    pub right_rows: usize,
    pub distinct_left: usize,
    pub distinct_right: usize,
    pub matched_left: usize,
    pub matched_right: usize,
    pub unmatched_left: Vec<String>,
    pub unmatched_right: Vec<String>,
    pub fixes: Vec<SuggestedFix>,
    /// Either side had more rows than `sample`, so the counts are partial.
    pub capped: bool,
}

/// Non-empty key values of one column, capped at `sample` rows.
///
/// Deliberately **not** trimmed: the baseline has to reflect what an actual
/// join would do, byte for byte. Trimming here would silently fix the very
/// problem `FixKind::TrimWhitespace` exists to report.
fn values(t: &DataTable, col: usize, sample: usize) -> Vec<String> {
    (0..t.row_count().min(sample))
        .filter_map(|r| t.get(r, col))
        .map(CellValue::to_string)
        .filter(|s| !s.trim().is_empty())
        .collect()
}

fn apply(kind: FixKind, s: &str) -> String {
    let opts = |lower, collapse_ws, strip_punct| NormalizeOpts {
        lower,
        collapse_ws,
        strip_punct,
    };
    match kind {
        FixKind::TrimWhitespace => s.trim().to_string(),
        FixKind::IgnoreCase => normalize(s, &opts(true, false, false)),
        FixKind::CollapseWhitespace => normalize(s, &opts(false, true, false)),
        FixKind::StripPunctuation => normalize(s, &opts(false, true, true)),
        FixKind::StripLeadingZeros => {
            let t = s.trim().trim_start_matches('0');
            if t.is_empty() {
                "0".to_string()
            } else {
                t.to_string()
            }
        }
    }
}

/// Distinct values on each side that have a partner on the other, optionally
/// under a normalisation.
fn overlap(kind: Option<FixKind>, left: &[String], right: &[String]) -> (usize, usize) {
    let norm = |s: &String| match kind {
        Some(k) => apply(k, s),
        None => s.clone(),
    };
    let l: HashSet<String> = left.iter().map(&norm).collect();
    let r: HashSet<String> = right.iter().map(&norm).collect();
    let shared = l.intersection(&r).count();
    // Symmetric by construction, but returned per side so the caller can report
    // "matched on the left / on the right" without recomputing.
    (shared, shared)
}

/// Diagnose a join between `left.left_col` and `right.right_col`.
pub fn diagnose(
    left: &DataTable,
    left_col: usize,
    right: &DataTable,
    right_col: usize,
    sample: usize,
) -> JoinDiagnosis {
    let lv = values(left, left_col, sample);
    let rv = values(right, right_col, sample);

    let ldist: HashSet<&String> = lv.iter().collect();
    let rdist: HashSet<&String> = rv.iter().collect();
    let (matched_left, matched_right) = overlap(None, &lv, &rv);

    let mut unmatched_left: Vec<String> = ldist
        .iter()
        .filter(|v| !rdist.contains(**v))
        .map(|v| (*v).clone())
        .collect();
    let mut unmatched_right: Vec<String> = rdist
        .iter()
        .filter(|v| !ldist.contains(**v))
        .map(|v| (*v).clone())
        .collect();
    // Sorted before truncating so the same table always shows the same samples;
    // a HashSet's iteration order would otherwise shuffle them between runs.
    unmatched_left.sort();
    unmatched_right.sort();
    unmatched_left.truncate(MAX_SAMPLES);
    unmatched_right.truncate(MAX_SAMPLES);

    let mut fixes = Vec::new();
    for kind in [
        FixKind::TrimWhitespace,
        FixKind::IgnoreCase,
        FixKind::CollapseWhitespace,
        FixKind::StripPunctuation,
        FixKind::StripLeadingZeros,
    ] {
        let (would, _) = overlap(Some(kind), &lv, &rv);
        // Strictly greater: a normalisation that changes nothing is not advice.
        if would > matched_left {
            fixes.push(SuggestedFix {
                kind,
                would_match: would,
            });
        }
    }
    // Most helpful first. `Reverse` rather than a flipped comparator so
    // clippy's sort_by_key form applies.
    fixes.sort_by_key(|f| std::cmp::Reverse(f.would_match));

    JoinDiagnosis {
        left_rows: left.row_count(),
        right_rows: right.row_count(),
        distinct_left: ldist.len(),
        distinct_right: rdist.len(),
        matched_left,
        matched_right,
        unmatched_left,
        unmatched_right,
        fixes,
        capped: left.row_count() > sample || right.row_count() > sample,
    }
}

#[cfg(test)]
#[path = "join_diag_tests.rs"]
mod tests;
