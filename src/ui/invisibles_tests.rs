use super::*;

fn kinds(text: &str) -> Vec<Option<Invisible>> {
    split_invisibles(text).into_iter().map(|(_, k)| k).collect()
}

#[test]
fn edge_spaces_are_flagged_inner_ones_are_not() {
    use Invisible::*;
    assert_eq!(
        kinds(" a b "),
        vec![Some(EdgeSpace), None, Some(Space), None, Some(EdgeSpace)]
    );
}

#[test]
fn tab_and_space_gaps_look_different() {
    let spaces: Vec<_> = split_invisibles("a  b")
        .iter()
        .filter_map(|(_, k)| k.map(Invisible::marker))
        .collect();
    let tab: Vec<_> = split_invisibles("a\tb")
        .iter()
        .filter_map(|(_, k)| k.map(Invisible::marker))
        .collect();
    assert_eq!(spaces, vec![".", "."]);
    assert_eq!(tab, vec!["->"]);
}

#[test]
fn odd_and_zero_width_characters_are_suspicious() {
    let found: Vec<_> = split_invisibles("a\u{00A0}b\u{200B}c\r")
        .into_iter()
        .filter_map(|(_, k)| k)
        .collect();
    assert_eq!(
        found,
        vec![
            Invisible::OddSpace,
            Invisible::ZeroWidth,
            Invisible::CarriageReturn
        ]
    );
    assert!(found.iter().all(|k| k.is_suspicious()));
}

#[test]
fn ranges_cover_the_text_and_plain_text_has_none() {
    let text = "x\u{00A0}yz ";
    let total: usize = split_invisibles(text).iter().map(|(r, _)| r.len()).sum();
    assert_eq!(total, text.len());
    assert!(!has_invisibles("plain"));
    assert!(has_invisibles("two words"));
}
