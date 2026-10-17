//! Which columns of these tables would actually join?
//!
//! Scores every cross-table column pair by containment (how much of the
//! smaller distinct set appears in the larger), weighted by distinctness so a
//! status or boolean column cannot outrank a real key. Built for N tables from
//! the start, so three open tabs give all three pairings.
//!
//! Sampled, so a high score is strong evidence and not proof. That ceiling is
//! deliberate: the point is to stop people guessing column names, not to
//! certify a foreign key.

use std::collections::HashSet;

use crate::data::DataTable;

/// Rows read per table before scoring. Enough to be meaningful, small enough
/// that the dialog answers instantly on a large file.
pub const DEFAULT_SAMPLE_ROWS: usize = 10_000;

/// Candidates scoring below this are noise and are not returned.
pub const MIN_SCORE: f64 = 0.2;

/// One suggested column pairing. `left` and `right` are
/// `(table index, column index)` into the slice passed to [`suggest_keys`].
#[derive(Debug, Clone, PartialEq)]
pub struct KeyCandidate {
    pub left: (usize, usize),
    pub right: (usize, usize),
    /// Share of the smaller distinct set found in the larger, 0.0 to 1.0.
    pub overlap: f64,
    /// Distinct values divided by sampled non-empty values, per side. A real
    /// key is near 1.0; a status column is near zero.
    pub left_distinct: f64,
    pub right_distinct: f64,
    /// `overlap` weighted by the better distinctness of the two sides.
    pub score: f64,
    /// Distinct values seen per side, and how many of them find no partner on
    /// the other side. Reported **both ways round**; see [`PairScore`] for why
    /// one direction cannot break a tie on its own.
    pub left_values: usize,
    pub right_values: usize,
    pub left_orphans: usize,
    pub right_orphans: usize,
}

/// Distinct trimmed values of one column, plus how many non-empty values were
/// seen (the denominator for distinctness).
pub fn column_values(table: &DataTable, col: usize, sample: usize) -> (HashSet<String>, usize) {
    let mut set = HashSet::new();
    let mut seen = 0usize;
    for row in 0..table.row_count().min(sample) {
        let Some(cell) = table.get(row, col) else {
            continue;
        };
        let s = cell.to_string();
        let s = s.trim();
        if s.is_empty() {
            continue;
        }
        seen += 1;
        set.insert(s.to_string());
    }
    (set, seen)
}

/// How well one ordered column pair joins. `None` when they share no value at
/// all, which is not a candidate.
///
/// Extracted so the ranked list, the relationship map and the map's
/// verify-a-declared-key pass all read the same formula. Duplicating it would
/// let two numbers on one screen disagree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairScore {
    pub overlap: f64,
    pub left_distinct: f64,
    pub right_distinct: f64,
    pub score: f64,
    /// Distinct values seen on each side: the denominators for the orphan
    /// counts below.
    pub left_values: usize,
    pub right_values: usize,
    /// Distinct values with no partner on the other side, **both ways round**.
    ///
    /// One direction is not enough. Two candidates tie exactly when both
    /// tables number their rows from 1, and then the count only separates
    /// them when read from the *child* side: with 4 customers and 1,000
    /// orders, `customers.id` finds all four of its values in both
    /// `orders.id` and `orders.customer_id` (0 either way, useless), while
    /// `orders.id` leaves 996 unmatched and `orders.customer_id` leaves none.
    /// Nothing here knows which side is the child, so both are reported and
    /// the reader picks.
    pub left_orphans: usize,
    pub right_orphans: usize,
}

pub fn score_pair(
    left: &HashSet<String>,
    left_seen: usize,
    right: &HashSet<String>,
    right_seen: usize,
) -> Option<PairScore> {
    if left.is_empty() || right.is_empty() {
        return None;
    }
    score_counts(
        left.len(),
        left_seen,
        right.len(),
        right_seen,
        left.intersection(right).count(),
    )
}

/// [`score_pair`] from counts alone: distinct values and non-empty rows per
/// side, and how many distinct values both sides share. The server path
/// counts these in SQL and calls this, so both paths share one formula.
pub fn score_counts(
    left_values: usize,
    left_seen: usize,
    right_values: usize,
    right_seen: usize,
    shared: usize,
) -> Option<PairScore> {
    if left_values == 0 || right_values == 0 || shared == 0 {
        return None;
    }
    let smaller = left_values.min(right_values) as f64;
    let overlap = shared as f64 / smaller;
    let left_distinct = left_values as f64 / left_seen.max(1) as f64;
    let right_distinct = right_values as f64 / right_seen.max(1) as f64;
    Some(PairScore {
        overlap,
        left_distinct,
        right_distinct,
        // Distinctness is the tie-breaker that keeps `flag` below `id`: both
        // may overlap fully, only one identifies rows.
        score: overlap * left_distinct.max(right_distinct),
        left_values,
        right_values,
        left_orphans: left_values - shared,
        right_orphans: right_values - shared,
    })
}

pub fn candidate(left: (usize, usize), right: (usize, usize), p: PairScore) -> KeyCandidate {
    KeyCandidate {
        left,
        right,
        overlap: p.overlap,
        left_distinct: p.left_distinct,
        right_distinct: p.right_distinct,
        score: p.score,
        left_values: p.left_values,
        right_values: p.right_values,
        left_orphans: p.left_orphans,
        right_orphans: p.right_orphans,
    }
}

/// Best first. Stable, so equal scores keep generation order.
pub fn sort_candidates(out: &mut [KeyCandidate]) {
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Rank column pairs across every pair of tables, best first.
pub fn suggest_keys(tables: &[&DataTable], sample: usize) -> Vec<KeyCandidate> {
    let sample = sample.max(1);
    let mut out = Vec::new();
    if tables.len() < 2 {
        return out;
    }

    // One value set per column, computed once: an N-table run would otherwise
    // rebuild the same set for every pairing it takes part in.
    let sets: Vec<Vec<(HashSet<String>, usize)>> = tables
        .iter()
        .map(|t| {
            (0..t.col_count())
                .map(|c| column_values(t, c, sample))
                .collect()
        })
        .collect();

    for ti in 0..tables.len() {
        for tj in (ti + 1)..tables.len() {
            for ci in 0..tables[ti].col_count() {
                let (left_set, left_seen) = &sets[ti][ci];
                if left_set.is_empty() {
                    continue;
                }
                for (cj, (right_set, right_seen)) in sets[tj].iter().enumerate() {
                    let Some(p) = score_pair(left_set, *left_seen, right_set, *right_seen) else {
                        continue;
                    };
                    if p.score < MIN_SCORE {
                        continue;
                    }
                    out.push(candidate((ti, ci), (tj, cj), p));
                }
            }
        }
    }

    sort_candidates(&mut out);
    out
}

#[cfg(test)]
#[path = "join_keys_tests.rs"]
mod tests;
