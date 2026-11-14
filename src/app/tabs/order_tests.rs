use super::super::TabId;
use super::*;

#[test]
fn moving_shifts_the_tabs_in_between() {
    // [a b c d], move 0 -> 2 gives [b c a d].
    assert_eq!(
        (0..4).map(|i| moved(i, 0, 2)).collect::<Vec<_>>(),
        [2, 0, 1, 3]
    );
    // move 3 -> 1 gives [a d b c].
    assert_eq!(
        (0..4).map(|i| moved(i, 3, 1)).collect::<Vec<_>>(),
        [0, 2, 3, 1]
    );
    assert_eq!(moved(2, 1, 1), 2);
}

#[test]
fn ids_are_unique() {
    let a = TabId::fresh();
    let b = TabId::fresh();
    assert_ne!(a, b);
}

#[test]
fn inverse_maps_old_to_new_and_rejects_non_permutations() {
    assert_eq!(inverse(&[2, 0, 1], 3), Some(vec![1, 2, 0]));
    assert_eq!(inverse(&[0, 0, 1], 3), None);
    assert_eq!(inverse(&[0, 1], 3), None);
    assert_eq!(inverse(&[0, 3, 1], 3), None);
}
