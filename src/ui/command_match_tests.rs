use super::*;

#[test]
fn every_word_must_appear_in_any_order() {
    assert!(rank("piv", "Pivot / Unpivot...", "Analyse", "Pivot / Unpivot...").is_some());
    assert!(rank("analyse piv", "Pivot / Unpivot...", "Analyse", "x").is_some());
    assert!(rank("piv analyse", "Pivot / Unpivot...", "Analyse", "x").is_some());
    assert!(rank("pivot chart", "Pivot / Unpivot...", "Analyse", "x").is_none());
}

#[test]
fn case_and_accents_do_not_matter() {
    assert!(rank("ZUSAMMENFUHREN", "Zusammenführen", "Daten", "Merge").is_some());
    assert!(rank("über", "Ueber", "x", "uber").is_some());
}

#[test]
fn the_english_name_matches_too() {
    assert!(
        rank(
            "pivot",
            "Pivottabelle umkehren",
            "Analysieren",
            "Pivot / Unpivot..."
        )
        .is_some()
    );
}

#[test]
fn a_name_that_starts_with_the_query_ranks_first() {
    let a = rank("co", "Copy as JSON", "Edit", "Copy as JSON").unwrap();
    let b = rank("co", "Compare folders...", "Analyse", "Compare folders...").unwrap();
    let c = rank("co", "Column filter...", "Columns", "x").unwrap();
    assert_eq!((a, b, c), (0, 0, 0));
    assert_eq!(rank("json", "Copy as JSON", "Edit", "x"), Some(1));
    assert_eq!(rank("", "anything", "x", "x"), Some(1));
}
