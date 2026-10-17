use super::parent_prefix;

#[test]
fn parent_prefix_walks_up_one_level_at_a_time() {
    assert_eq!(parent_prefix("a/b/c/"), "a/b/");
    assert_eq!(parent_prefix("a/b/"), "a/");
    // The last level up lands at the bucket root, not at "a".
    assert_eq!(parent_prefix("a/"), "");
    assert_eq!(parent_prefix(""), "");
}
