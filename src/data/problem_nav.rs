//! Navigation across the cells other engines have already flagged: validation
//! violations and detected outliers. Both sets are cached on the tab and
//! rebuilt by `recompute_filter`, so this computes nothing, it only orders.
//!
//! Deliberately a sibling of `view_modes::record::next_record_index`: same
//! shape, same wrapping rules, same respect for the active filter. A flagged
//! cell in a row the filter hides is not reachable, because the counter and
//! the navigation must agree with what the user can actually see.

use std::collections::{HashMap, HashSet};

/// Display position of each visible row, for ordering and comparison.
fn positions(filtered: &[usize]) -> HashMap<usize, usize> {
    filtered
        .iter()
        .enumerate()
        .map(|(display, &row)| (row, display))
        .collect()
}

/// Problem cells in display order: by the row's position in `filtered`, then
/// by column index. Cells in hidden rows are dropped.
fn ordered(problems: &HashSet<(usize, usize)>, filtered: &[usize]) -> Vec<(usize, usize)> {
    let position = positions(filtered);
    let mut cells: Vec<(usize, usize)> = problems
        .iter()
        .copied()
        .filter(|(row, _)| position.contains_key(row))
        .collect();
    cells.sort_by_key(|(row, col)| (position[row], *col));
    cells
}

/// The next (or previous) flagged cell after `current`, wrapping at the ends.
/// `None` when nothing is flagged or nothing flagged is visible.
///
/// When the selection is not itself on a flagged cell, this moves to the first
/// flagged cell after (or before) it, so the key does something sensible from
/// anywhere in the table.
pub fn next_problem_cell(
    problems: &HashSet<(usize, usize)>,
    filtered: &[usize],
    current: Option<(usize, usize)>,
    forward: bool,
) -> Option<(usize, usize)> {
    let cells = ordered(problems, filtered);
    if cells.is_empty() {
        return None;
    }
    let Some(cur) = current else {
        return cells.first().copied();
    };
    if let Some(idx) = cells.iter().position(|&c| c == cur) {
        let next = if forward {
            (idx + 1) % cells.len()
        } else {
            (idx + cells.len() - 1) % cells.len()
        };
        return cells.get(next).copied();
    }

    let position = positions(filtered);
    let Some(key) = position.get(&cur.0).map(|d| (*d, cur.1)) else {
        // The selection sits in a hidden row; start from the top.
        return cells.first().copied();
    };
    if forward {
        cells
            .iter()
            .find(|(row, col)| (position[row], *col) > key)
            .copied()
            .or_else(|| cells.first().copied())
    } else {
        cells
            .iter()
            .rev()
            .find(|(row, col)| (position[row], *col) < key)
            .copied()
            .or_else(|| cells.last().copied())
    }
}

/// How many flagged cells the user can currently reach. Drives the
/// `Problem 3 of 27` status-bar counter.
pub fn visible_problem_count(problems: &HashSet<(usize, usize)>, filtered: &[usize]) -> usize {
    ordered(problems, filtered).len()
}

/// Which of the visible problems `cell` is, 1-based. `None` when the cell is
/// not a flagged one.
pub fn problem_position(
    problems: &HashSet<(usize, usize)>,
    filtered: &[usize],
    cell: (usize, usize),
) -> Option<usize> {
    ordered(problems, filtered)
        .iter()
        .position(|&c| c == cell)
        .map(|i| i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problems(cells: &[(usize, usize)]) -> HashSet<(usize, usize)> {
        cells.iter().copied().collect()
    }

    #[test]
    fn walks_in_display_order_then_wraps() {
        let p = problems(&[(5, 1), (2, 3), (2, 0)]);
        let filtered: Vec<usize> = (0..10).collect();

        // From nothing selected: the first problem in display order.
        assert_eq!(next_problem_cell(&p, &filtered, None, true), Some((2, 0)));
        assert_eq!(
            next_problem_cell(&p, &filtered, Some((2, 0)), true),
            Some((2, 3))
        );
        assert_eq!(
            next_problem_cell(&p, &filtered, Some((2, 3)), true),
            Some((5, 1))
        );
        // Wrap.
        assert_eq!(
            next_problem_cell(&p, &filtered, Some((5, 1)), true),
            Some((2, 0))
        );
    }

    #[test]
    fn walks_backwards() {
        let p = problems(&[(5, 1), (2, 3), (2, 0)]);
        let filtered: Vec<usize> = (0..10).collect();
        assert_eq!(
            next_problem_cell(&p, &filtered, Some((2, 3)), false),
            Some((2, 0))
        );
        assert_eq!(
            next_problem_cell(&p, &filtered, Some((2, 0)), false),
            Some((5, 1))
        );
    }

    /// A flagged cell in a filtered-away row is not reachable.
    #[test]
    fn skips_rows_hidden_by_the_filter() {
        let p = problems(&[(1, 0), (7, 0)]);
        let filtered = vec![0, 7, 9];
        assert_eq!(next_problem_cell(&p, &filtered, None, true), Some((7, 0)));
        // Only one visible problem: stepping stays on it rather than jumping
        // to the hidden one.
        assert_eq!(
            next_problem_cell(&p, &filtered, Some((7, 0)), true),
            Some((7, 0))
        );
    }

    /// Pressing the key from an ordinary cell lands on the next problem after
    /// it, not back at the top.
    #[test]
    fn starts_from_the_selection_when_it_is_not_a_problem() {
        let p = problems(&[(1, 0), (8, 2)]);
        let filtered: Vec<usize> = (0..10).collect();
        assert_eq!(
            next_problem_cell(&p, &filtered, Some((4, 0)), true),
            Some((8, 2))
        );
        assert_eq!(
            next_problem_cell(&p, &filtered, Some((4, 0)), false),
            Some((1, 0))
        );
    }

    #[test]
    fn empty_cases() {
        let filtered: Vec<usize> = (0..3).collect();
        assert_eq!(
            next_problem_cell(&HashSet::new(), &filtered, None, true),
            None
        );
        assert_eq!(
            next_problem_cell(&problems(&[(0, 0)]), &[], None, true),
            None
        );
    }

    #[test]
    fn counts_and_positions_visible_problems() {
        let p = problems(&[(1, 0), (7, 0), (7, 2)]);
        assert_eq!(visible_problem_count(&p, &[0, 7, 9]), 2);
        assert_eq!(visible_problem_count(&p, &(0..10).collect::<Vec<_>>()), 3);

        let all: Vec<usize> = (0..10).collect();
        assert_eq!(problem_position(&p, &all, (1, 0)), Some(1));
        assert_eq!(problem_position(&p, &all, (7, 2)), Some(3));
        assert_eq!(problem_position(&p, &all, (4, 4)), None);
    }
}
