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
const MIN_SCORE: f64 = 0.2;

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
    let shared = left.intersection(right).count();
    if shared == 0 {
        return None;
    }
    let smaller = left.len().min(right.len()) as f64;
    let overlap = shared as f64 / smaller;
    let left_distinct = left.len() as f64 / left_seen.max(1) as f64;
    let right_distinct = right.len() as f64 / right_seen.max(1) as f64;
    Some(PairScore {
        overlap,
        left_distinct,
        right_distinct,
        // Distinctness is the tie-breaker that keeps `flag` below `id`: both
        // may overlap fully, only one identifies rows.
        score: overlap * left_distinct.max(right_distinct),
        left_values: left.len(),
        right_values: right.len(),
        left_orphans: left.len() - shared,
        right_orphans: right.len() - shared,
    })
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
                    out.push(KeyCandidate {
                        left: (ti, ci),
                        right: (tj, cj),
                        overlap: p.overlap,
                        left_distinct: p.left_distinct,
                        right_distinct: p.right_distinct,
                        score: p.score,
                        left_values: p.left_values,
                        right_values: p.right_values,
                        left_orphans: p.left_orphans,
                        right_orphans: p.right_orphans,
                    });
                }
            }
        }
    }

    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{CellValue, ColumnInfo, DataTable};

    fn table(cols: &[&str], rows: Vec<Vec<&str>>) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = cols
            .iter()
            .map(|c| ColumnInfo {
                name: (*c).to_string(),
                data_type: "Utf8".into(),
            })
            .collect();
        t.rows = rows
            .into_iter()
            .map(|r| {
                r.into_iter()
                    .map(|v| CellValue::String(v.to_string()))
                    .collect()
            })
            .collect();
        t
    }

    #[test]
    fn a_tie_is_broken_from_the_child_side_whichever_way_round_it_is_read() {
        // Both tables number their rows from 1, which is what makes two
        // candidates score identically: `customers.id` against `orders.id` is
        // a coincidence (order numbers 1..4 simply exist), `customers.id`
        // against `orders.customer_id` is the real key. Reading orphans from
        // the parent side cannot tell them apart - all four customer ids are
        // present either way - so both directions have to be reported.
        let int_col = |name: &str| ColumnInfo {
            name: name.to_string(),
            data_type: "Int64".into(),
        };
        let mut customers = DataTable::empty();
        customers.columns = vec![int_col("id")];
        customers.rows = (1..=4i64).map(|i| vec![CellValue::Int(i)]).collect();
        let mut orders = DataTable::empty();
        orders.columns = vec![int_col("id"), int_col("customer_id")];
        orders.rows = (1..=1000i64)
            .map(|i| vec![CellValue::Int(i), CellValue::Int(1 + (i - 1) % 4)])
            .collect();

        let out = suggest_keys(&[&customers, &orders], DEFAULT_SAMPLE_ROWS);
        let find = |right_col: usize| {
            out.iter()
                .find(|k| k.left == (0, 0) && k.right == (1, right_col))
                .unwrap_or_else(|| panic!("no candidate for right column {right_col}"))
        };
        let coincidence = find(0);
        let real_key = find(1);

        // The tie itself: identical scores, and identical orphan counts in the
        // direction the caller did not choose.
        assert_eq!(
            coincidence.score, real_key.score,
            "the two must actually tie"
        );
        assert_eq!(coincidence.left_orphans, 0);
        assert_eq!(real_key.left_orphans, 0);

        // The other direction settles it.
        assert_eq!(coincidence.right_orphans, 996);
        assert_eq!(real_key.right_orphans, 0);
        assert_eq!(coincidence.right_values, 1000);
        assert_eq!(real_key.right_values, 4);
    }

    #[test]
    fn the_real_key_ranks_first() {
        let orders = table(
            &["cust_id", "status"],
            vec![vec!["c1", "open"], vec!["c2", "open"], vec!["c3", "shut"]],
        );
        let customers = table(
            &["id", "active"],
            vec![
                vec!["c1", "yes"],
                vec!["c2", "yes"],
                vec!["c3", "no"],
                vec!["c9", "no"],
            ],
        );

        let out = suggest_keys(&[&orders, &customers], DEFAULT_SAMPLE_ROWS);
        assert!(!out.is_empty());
        let best = &out[0];
        assert_eq!(best.left, (0, 0));
        assert_eq!(best.right, (1, 0));
        assert!(best.overlap > 0.99, "overlap was {}", best.overlap);
    }

    /// A low-cardinality column that happens to overlap must not beat a key.
    #[test]
    fn low_cardinality_decoys_lose() {
        let a = table(
            &["id", "flag"],
            vec![vec!["1", "yes"], vec!["2", "no"], vec!["3", "yes"]],
        );
        let b = table(
            &["key", "flag"],
            vec![vec!["1", "yes"], vec!["2", "no"], vec!["3", "no"]],
        );
        let out = suggest_keys(&[&a, &b], DEFAULT_SAMPLE_ROWS);
        assert_eq!(out[0].left, (0, 0), "expected id -> key first, got {out:?}");
        assert_eq!(out[0].right, (1, 0));
    }

    #[test]
    fn disjoint_tables_produce_nothing() {
        let a = table(&["x"], vec![vec!["a"], vec!["b"]]);
        let b = table(&["y"], vec![vec!["c"], vec!["d"]]);
        assert!(suggest_keys(&[&a, &b], DEFAULT_SAMPLE_ROWS).is_empty());
    }

    /// Built for N, not for two: three tables give all three pairings.
    #[test]
    fn three_tables_are_compared_pairwise() {
        let a = table(&["id"], vec![vec!["1"], vec!["2"]]);
        let b = table(&["id"], vec![vec!["1"], vec!["2"]]);
        let c = table(&["id"], vec![vec!["1"], vec!["2"]]);
        let out = suggest_keys(&[&a, &b, &c], DEFAULT_SAMPLE_ROWS);
        let pairs: std::collections::HashSet<(usize, usize)> =
            out.iter().map(|k| (k.left.0, k.right.0)).collect();
        assert_eq!(pairs.len(), 3, "expected 0-1, 0-2, 1-2, got {pairs:?}");
    }

    #[test]
    fn empty_input_is_not_an_error() {
        assert!(suggest_keys(&[], DEFAULT_SAMPLE_ROWS).is_empty());
        let a = table(&["x"], vec![vec!["1"]]);
        assert!(suggest_keys(&[&a], DEFAULT_SAMPLE_ROWS).is_empty());
    }

    /// Blank cells are not values: a column of empties suggests nothing.
    #[test]
    fn empty_cells_are_ignored() {
        let a = table(&["id", "note"], vec![vec!["1", ""], vec!["2", "  "]]);
        let b = table(&["id", "note"], vec![vec!["1", ""], vec!["2", ""]]);
        let out = suggest_keys(&[&a, &b], DEFAULT_SAMPLE_ROWS);
        assert!(
            out.iter().all(|k| k.left.1 == 0 && k.right.1 == 0),
            "the empty note columns must not pair: {out:?}"
        );
    }

    /// The sample bounds the work: with a sample of 1 only the first row of
    /// each table is considered.
    #[test]
    fn sample_size_bounds_the_scan() {
        let a = table(&["id"], vec![vec!["1"], vec!["2"], vec!["3"]]);
        let b = table(&["id"], vec![vec!["9"], vec!["2"], vec!["3"]]);
        assert!(
            suggest_keys(&[&a, &b], 1).is_empty(),
            "row 1 shares nothing"
        );
        assert!(!suggest_keys(&[&a, &b], 3).is_empty());
    }
}
