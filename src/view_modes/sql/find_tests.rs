use super::*;

fn find(query: &str, current: usize) -> SqlFind {
    SqlFind {
        input: crate::view_modes::find_box::FindInput {
            query: query.to_string(),
            ..Default::default()
        },
        current,
        ..Default::default()
    }
}

#[test]
fn matches_ignore_case_and_a_closed_strip_finds_nothing() {
    let sql = "select id from Data where ID > 1";
    let (ranges, current) = matches_in(Some(&find("id", 0)), sql);
    assert_eq!(ranges, vec![7..9, 26..28]);
    assert_eq!(current, 0);
    assert!(matches_in(None, sql).0.is_empty());
    assert!(matches_in(Some(&find("", 0)), sql).0.is_empty());
}

#[test]
fn the_current_match_survives_matches_disappearing() {
    // Three matches were there and the third was current; after a replace
    // only one is left, which is then the current one.
    let (ranges, current) = matches_in(Some(&find("x", 2)), "x y");
    assert_eq!(ranges.len(), 1);
    assert_eq!(current, 0);
}

#[test]
fn stepping_wraps_both_ways_and_asks_the_editor_to_follow() {
    let mut f = find("a", 0);
    step(&mut f, 3, true);
    assert_eq!(f.current, 2);
    assert!(f.jump);
    step(&mut f, 3, false);
    assert_eq!(f.current, 0);
    f.jump = false;
    step(&mut f, 0, false);
    assert!(!f.jump, "no matches, nothing to show");
}
