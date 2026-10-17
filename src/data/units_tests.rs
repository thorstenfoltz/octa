use super::*;

fn n(s: &str) -> Option<f64> {
    parse_value(s).map(|p| p.number)
}

#[test]
fn magnitude_suffixes_fold_into_the_number() {
    assert_eq!(n("1.2k"), Some(1200.0));
    assert_eq!(n("3 Mio"), Some(3_000_000.0));
    assert_eq!(n("2M"), Some(2_000_000.0));
    assert_eq!(n("4 mrd"), Some(4e9));
    assert_eq!(parse_value("1.2k").unwrap().unit, "");
    assert_eq!(parse_value("1.2k").unwrap().flavour, Flavour::Magnitude);
}

#[test]
fn a_currency_is_recognised_on_either_side() {
    assert_eq!(n("EUR 4,00"), Some(4.0));
    assert_eq!(n("4,00 EUR"), Some(4.0));
    assert_eq!(n("$1,200.50"), Some(1200.50));
    assert_eq!(parse_value("$1,200.50").unwrap().unit, "$");
    assert_eq!(parse_value("EUR 4,00").unwrap().flavour, Flavour::Currency);
}

/// `$1,200` on its own is genuinely undecidable: twelve hundred dollars in
/// Ohio, one euro twenty in Bavaria. A per-value read has to guess, and a
/// wrong guess here is wrong by a factor of a thousand - which is why
/// `detect_column` settles the convention over the whole column and hands
/// it back for the apply step to use.
#[test]
fn a_lone_thousands_group_is_undecidable_but_a_column_of_them_is_not() {
    // Alone: the relaxed parser falls back to the European reading.
    assert_eq!(n("$1,200"), Some(1.2));

    // In a column where another value settles it, the style comes back and
    // parsing with it gives the right number.
    let d = detect_column(&col(&["$1,200", "$3,400", "$1,234.56"])).expect("detected");
    assert_eq!(d.style, Some(NumberStyle::English));
    assert_eq!(
        parse_value_with("$1,200", d.style).map(|p| p.number),
        Some(1200.0)
    );

    // And the other way round, where the column is European.
    let d = detect_column(&col(&["EUR 1,20", "EUR 3,40", "EUR 1.234,56"])).expect("detected");
    assert_eq!(d.style, Some(NumberStyle::European));
    assert_eq!(
        parse_value_with("EUR 1,20", d.style).map(|p| p.number),
        Some(1.2)
    );
}

/// The whole reason this reuses `num_parse`: `4,00` is four euros in
/// Germany and `1,200` is twelve hundred dollars in the States, and one
/// hand-rolled parser here would get one of them wrong.
#[test]
fn european_and_english_decimals_both_work() {
    assert_eq!(n("1.234,56 EUR"), Some(1234.56));
    assert_eq!(n("1,234.56 USD"), Some(1234.56));
}

#[test]
fn units_keep_their_name() {
    assert_eq!(n("12 kg"), Some(12.0));
    assert_eq!(parse_value("12 kg").unwrap().unit, "kg");
    assert_eq!(parse_value("5ms").unwrap().unit, "ms");
    assert_eq!(parse_value("5ms").unwrap().flavour, Flavour::Unit);
}

/// Kept as written. Turning 45% into 0.45 is a change of meaning, and this
/// module does not change meanings.
#[test]
fn a_percentage_keeps_its_number() {
    assert_eq!(n("45%"), Some(45.0));
    assert_eq!(parse_value("45 %").unwrap().unit, "%");
    assert_eq!(parse_value("45%").unwrap().flavour, Flavour::Percent);
}

/// A bare number belongs to the number-inference pass. Claiming it here
/// would put two competing suggestions in front of the user.
#[test]
fn a_plain_number_is_not_this_features_business() {
    assert_eq!(parse_value("42"), None);
    assert_eq!(parse_value("1.234,56"), None);
    assert_eq!(parse_value("-3.5"), None);
}

#[test]
fn ordinary_text_is_left_alone() {
    for s in ["hello", "", "  ", "2024-01-02", "12-14", "N/A", "a1"] {
        assert_eq!(parse_value(s), None, "{s:?} should not parse");
    }
}

fn col<'a>(values: &[&'a str]) -> Vec<Option<&'a str>> {
    values.iter().map(|v| Some(*v)).collect()
}

#[test]
fn a_uniform_column_is_detected() {
    let d = detect_column(&col(&["12 kg", "3 kg", "40 kg", "7 kg"])).expect("detected");
    assert_eq!(d.flavour, Flavour::Unit);
    assert_eq!(d.unit, "kg");
    assert_eq!(d.matched, 4);
    assert_eq!(d.total, 4);
    assert!(!d.mixed_units);
}

/// The column nobody should sum, said out loud rather than hidden.
#[test]
fn mixed_units_are_reported_as_mixed() {
    let d = detect_column(&col(&["12 kg", "3 lb", "40 kg", "7 lb"])).expect("detected");
    assert!(d.mixed_units);
}

#[test]
fn prose_with_a_number_in_it_is_not_a_unit_column() {
    assert_eq!(
        detect_column(&col(&["12 kg", "see note", "ask Bob", "n/a", "later"])),
        None
    );
}

#[test]
fn a_tiny_column_is_not_enough_to_go_on() {
    assert_eq!(detect_column(&col(&["12 kg", "3 kg"])), None);
}

#[test]
fn a_column_of_plain_numbers_is_not_detected() {
    assert_eq!(detect_column(&col(&["1", "2", "3", "4"])), None);
}
