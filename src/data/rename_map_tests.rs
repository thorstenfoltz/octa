use super::*;

#[test]
fn parse_comma_and_tab() {
    let pairs = parse_mapping("a,b\n c \t d \n\n");
    assert_eq!(
        pairs,
        vec![("a".into(), "b".into()), ("c".into(), "d".into())]
    );
}

#[test]
fn matched_and_unmatched() {
    let cols = vec!["id".to_string(), "name".to_string()];
    let plan = plan_renames(
        &cols,
        &[
            ("id".into(), "user_id".into()),
            ("missing".into(), "x".into()),
        ],
    );
    assert_eq!(
        plan.matched,
        vec![(0, "id".to_string(), "user_id".to_string())]
    );
    assert_eq!(plan.unmatched, vec!["missing".to_string()]);
    assert!(plan.collisions.is_empty());
}

#[test]
fn collision_target_exists() {
    let cols = vec!["a".to_string(), "b".to_string()];
    // renaming a -> b collides with existing column b.
    let plan = plan_renames(&cols, &[("a".into(), "b".into())]);
    assert!(!plan.collisions.is_empty());
}

#[test]
fn collision_duplicate_target() {
    let cols = vec!["a".to_string(), "b".to_string()];
    let plan = plan_renames(&cols, &[("a".into(), "z".into()), ("b".into(), "z".into())]);
    assert!(!plan.collisions.is_empty());
}

#[test]
fn swap_is_not_a_collision() {
    // a -> b and b -> a: both sources are renamed away, so neither target
    // duplicates a surviving column.
    let cols = vec!["a".to_string(), "b".to_string()];
    let plan = plan_renames(&cols, &[("a".into(), "b".into()), ("b".into(), "a".into())]);
    assert!(plan.collisions.is_empty(), "got {:?}", plan.collisions);
    assert_eq!(plan.matched.len(), 2);
}

#[test]
fn dedupe_suffixes_repeats_in_column_order() {
    let cols = vec!["a".into(), "b".into(), "a".into(), "a".into()];
    assert_eq!(
        plan_dedupe(&cols, false),
        vec![
            (2, "a".to_string(), "a_2".to_string()),
            (3, "a".to_string(), "a_3".to_string()),
        ]
    );
}

#[test]
fn dedupe_leaves_unique_names_alone() {
    let cols = vec!["a".into(), "B".into()];
    assert!(plan_dedupe(&cols, false).is_empty());
    // Case-folded they are still different names.
    assert!(plan_dedupe(&cols, true).is_empty());
}

#[test]
fn dedupe_can_ignore_case() {
    let cols = vec!["Name".into(), "name".into()];
    assert!(plan_dedupe(&cols, false).is_empty());
    assert_eq!(
        plan_dedupe(&cols, true),
        vec![(1, "name".to_string(), "name_2".to_string())]
    );
}

#[test]
fn dedupe_does_not_steal_a_suffix_another_column_owns() {
    // `a_2` is a real column here, so the duplicate `a` has to become `a_3`
    // and `a_2` keeps its name.
    let cols = vec!["a".into(), "a".into(), "a_2".into()];
    assert_eq!(
        plan_dedupe(&cols, false),
        vec![(1, "a".to_string(), "a_3".to_string())]
    );
}

#[test]
fn dedupe_result_has_no_duplicates_left() {
    let cols: Vec<String> = vec!["x".into(), "x".into(), "X".into(), "x_2".into()];
    let mut out = cols.clone();
    for (i, _, new) in plan_dedupe(&cols, true) {
        out[i] = new;
    }
    let folded: std::collections::HashSet<String> = out.iter().map(|s| s.to_lowercase()).collect();
    assert_eq!(folded.len(), out.len(), "{out:?}");
}
