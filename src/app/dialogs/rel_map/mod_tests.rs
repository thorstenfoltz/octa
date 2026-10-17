use super::*;

/// Half the locales Octa ships write a decimal comma, and a threshold is
/// exactly the kind of field someone types by hand.
#[test]
fn parse_score_takes_either_decimal_mark() {
    assert_eq!(parse_score("0.35"), Some(0.35));
    assert_eq!(parse_score("0,35"), Some(0.35));
    assert_eq!(parse_score("  .5 "), Some(0.5));
    assert_eq!(parse_score("1"), Some(1.0));
}

/// Out of range clamps rather than erroring: holding a digit key is an
/// easy way to reach 11, and "show nothing" is a coherent answer.
#[test]
fn parse_score_clamps_instead_of_refusing() {
    assert_eq!(parse_score("11"), Some(1.0));
    assert_eq!(parse_score("-2"), Some(0.0));
}

/// 10px per char stands in for a font: this tests the cutting, not
/// egui's metrics.
fn w(s: &str) -> f32 {
    s.chars().count() as f32 * 10.0
}

#[test]
fn a_name_that_fits_is_left_alone() {
    assert_eq!(layout::elide_to_width("orders", 100.0, w), "orders");
}

#[test]
fn a_long_name_is_cut_with_three_dots_inside_the_width() {
    let out = layout::elide_to_width("public.customer_order_line_items", 100.0, w);
    assert_eq!(out, "public....");
    assert!(w(&out) <= 100.0);
}

/// Multi-byte names are cut on characters, never inside one.
#[test]
fn eliding_cuts_on_characters() {
    let out = layout::elide_to_width("schéma.tablé_äöüäöü", 80.0, w);
    assert_eq!(out, "schém...");
}

#[test]
fn a_box_too_narrow_for_anything_still_says_something() {
    assert_eq!(layout::elide_to_width("orders", 20.0, w), "...");
}

/// A half-typed field must not move the threshold: `None` leaves the
/// slider where it was until the text becomes a number again.
#[test]
fn parse_score_rejects_what_is_not_a_number() {
    assert_eq!(parse_score(""), None);
    assert_eq!(parse_score("   "), None);
    assert_eq!(parse_score("-"), None);
    assert_eq!(parse_score("abc"), None);
}
