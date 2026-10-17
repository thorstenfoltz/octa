//! Unit tests for [`natural_sort`](super).

use super::*;
use std::cmp::Ordering::*;

#[test]
fn numbers_inside_text_compare_by_value() {
    assert_eq!(cmp_natural("a2", "a10"), Less);
    assert_eq!(cmp_natural("file10", "file2"), Greater);
    assert_eq!(cmp_natural("Room 3", "Room 12"), Less);
}

#[test]
fn version_like_text_sorts_part_by_part() {
    assert_eq!(cmp_natural("v1.9", "v1.10"), Less);
}

#[test]
fn more_leading_zeros_come_first_but_only_on_a_tie() {
    assert_eq!(cmp_natural("007", "7"), Less);
    assert_eq!(cmp_natural("7", "007"), Greater);
    // Letters after the number decide before the zeros do.
    assert_eq!(cmp_natural("a07c", "a7b"), Greater);
}

#[test]
fn very_long_digit_runs_do_not_overflow() {
    let forty_nines = "9".repeat(40);
    let one_and_forty_zeros = format!("1{}", "0".repeat(40));
    assert_eq!(cmp_natural(&forty_nines, &one_and_forty_zeros), Less);
}

#[test]
fn case_is_ignored_as_in_letter_order() {
    assert_eq!(cmp_natural("Apple", "apple"), Equal);
    assert_eq!(cmp_natural("apple", "Banana"), Less);
}

#[test]
fn a_prefix_comes_first_and_empty_first_of_all() {
    assert_eq!(cmp_natural("file", "file1"), Less);
    assert_eq!(cmp_natural("", "a"), Less);
    assert_eq!(cmp_natural("", ""), Equal);
}

#[test]
fn digits_at_the_start_and_the_end() {
    assert_eq!(cmp_natural("2a", "10a"), Less);
    assert_eq!(cmp_natural("a2", "a2b"), Less);
}
