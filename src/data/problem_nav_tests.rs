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
