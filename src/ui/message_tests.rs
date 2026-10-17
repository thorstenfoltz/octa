use super::*;

#[test]
fn a_short_list_is_spelled_out_in_full() {
    let items: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    assert_eq!(elide_list(&items, 6), "a, b, c");
    // Exactly at the cap is still the whole list, not "and 0 more".
    assert_eq!(elide_list(&items, 3), "a, b, c");
}

/// The bug this exists for: a JSON file with dozens of date columns made
/// one banner label wider than the screen, pushing its Okay / Dismiss
/// buttons out of reach in a panel that does not scroll sideways.
#[test]
fn a_long_list_is_cut_to_a_bounded_length() {
    let items: Vec<String> = (0..200).map(|i| format!("column_{i}")).collect();
    let text = elide_list(&items, BANNER_LIST_MAX);
    assert!(text.starts_with("column_0, column_1"));
    assert!(text.contains("194"), "the remainder is not counted: {text}");
    assert!(
        text.len() < 200,
        "still long enough to push the buttons off screen: {} chars",
        text.len()
    );
}

/// `max: 0` would otherwise produce a label that names nothing at all.
#[test]
fn at_least_one_name_is_always_shown() {
    let items: Vec<String> = ["only", "these"].iter().map(|s| s.to_string()).collect();
    assert!(elide_list(&items, 0).starts_with("only"));
}

#[test]
fn an_empty_list_is_empty() {
    assert_eq!(elide_list(&[], 6), "");
}
