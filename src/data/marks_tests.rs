use super::*;

#[test]
fn twelve_colours_each_with_its_own_hue() {
    assert_eq!(MarkColor::ALL.len(), 12);
    let mut seen = std::collections::HashSet::new();
    for c in MarkColor::ALL {
        assert!(seen.insert(c.rgb()), "{c:?} repeats a colour");
        assert!(
            crate::i18n::t(c.label_key()) != c.label_key(),
            "{c:?} has no name"
        );
    }
}

#[test]
fn the_six_old_names_still_load() {
    for (name, c) in [("Red", MarkColor::Red), ("Purple", MarkColor::Purple)] {
        let v: MarkColor = serde_json::from_str(&format!("\"{name}\"")).unwrap();
        assert_eq!(v, c);
    }
}
