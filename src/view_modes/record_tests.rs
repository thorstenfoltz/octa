use super::next_record_index;
use crate::app::state::NavDir;

#[test]
fn steps_forward_through_the_filtered_order() {
    let filtered = vec![2, 5, 9];
    assert_eq!(next_record_index(&filtered, 2, NavDir::Next), Some(5));
    assert_eq!(next_record_index(&filtered, 5, NavDir::Next), Some(9));
}

#[test]
fn steps_backward_through_the_filtered_order() {
    let filtered = vec![2, 5, 9];
    assert_eq!(next_record_index(&filtered, 9, NavDir::Prev), Some(5));
    assert_eq!(next_record_index(&filtered, 5, NavDir::Prev), Some(2));
}

#[test]
fn stops_at_both_ends() {
    let filtered = vec![2, 5, 9];
    assert_eq!(next_record_index(&filtered, 9, NavDir::Next), None);
    assert_eq!(next_record_index(&filtered, 2, NavDir::Prev), None);
}

#[test]
fn empty_filter_goes_nowhere() {
    assert_eq!(next_record_index(&[], 0, NavDir::Next), None);
    assert_eq!(next_record_index(&[], 0, NavDir::Prev), None);
}

#[test]
fn a_row_outside_the_filter_recovers_to_the_first_visible_row() {
    // The user filtered the table while sitting on row 7, which the new
    // filter hides. Both directions land on the first row still visible.
    let filtered = vec![2, 5, 9];
    assert_eq!(next_record_index(&filtered, 7, NavDir::Next), Some(2));
    assert_eq!(next_record_index(&filtered, 7, NavDir::Prev), Some(2));
}

#[test]
fn a_single_visible_row_has_no_neighbours() {
    let filtered = vec![4];
    assert_eq!(next_record_index(&filtered, 4, NavDir::Next), None);
    assert_eq!(next_record_index(&filtered, 4, NavDir::Prev), None);
}
