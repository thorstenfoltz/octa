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
#[path = "problem_nav_tests.rs"]
mod tests;
