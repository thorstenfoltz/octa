use super::*;
use crate::data::MarkColor;
use crate::data::conditional_format::{CondOp, CondRule};
use crate::data::num_format::{NumberFormat, RoundingMode};

fn rule(op: CondOp, value: &str) -> CondRule {
    rule_cs(op, value, false)
}

fn rule_cs(op: CondOp, value: &str, case_sensitive: bool) -> CondRule {
    CondRule {
        column: Some(0),
        op,
        value: value.to_string(),
        color: MarkColor::Red,
        case_sensitive,
    }
}

#[test]
fn numeric_comparisons_become_native_cell_rules() {
    for op in [CondOp::Gt, CondOp::Lt, CondOp::Ge, CondOp::Le] {
        assert!(
            matches!(map_rule(&rule(op, "1000")), XlsxRule::Cell { .. }),
            "{op:?} with a numeric operand must map to a native rule"
        );
    }
}

#[test]
fn text_ordering_is_baked() {
    // Excel orders text by locale collation, `rule_matches` by Rust string
    // ordering. They disagree on case and accents, so a native rule would
    // colour different cells than Octa does.
    for op in [CondOp::Gt, CondOp::Lt, CondOp::Ge, CondOp::Le] {
        assert_eq!(
            map_rule(&rule(op, "banana")),
            XlsxRule::Bake,
            "{op:?} over text must bake"
        );
    }
}

#[test]
fn equality_maps_for_both_numbers_and_text() {
    assert!(matches!(
        map_rule(&rule(CondOp::Eq, "42")),
        XlsxRule::Cell { .. }
    ));
    assert!(matches!(
        map_rule(&rule(CondOp::Ne, "42")),
        XlsxRule::Cell { .. }
    ));
    // ConditionalFormatCellRule is generic over IntoConditionalFormatValue,
    // which is implemented for &str, so text equality is native too.
    assert!(matches!(
        map_rule(&rule(CondOp::Eq, "paid")),
        XlsxRule::Cell { .. }
    ));
    assert!(matches!(
        map_rule(&rule(CondOp::Ne, "paid")),
        XlsxRule::Cell { .. }
    ));
}

#[test]
fn substring_and_blank_map_natively() {
    assert!(matches!(
        map_rule(&rule(CondOp::Contains, "x")),
        XlsxRule::Text { .. }
    ));
    assert!(matches!(
        map_rule(&rule(CondOp::NotContains, "x")),
        XlsxRule::Text { .. }
    ));
    assert_eq!(
        map_rule(&rule(CondOp::Empty, "")),
        XlsxRule::Blank { inverted: false }
    );
    assert_eq!(
        map_rule(&rule(CondOp::NotEmpty, "")),
        XlsxRule::Blank { inverted: true }
    );
}

#[test]
fn every_operator_is_handled() {
    // A new CondOp variant must make a deliberate choice here rather than
    // silently falling into a catch-all. With a numeric operand and
    // case-insensitive comparison every one of the ten operators is
    // expressible as a native rule, so none of them should bake.
    for op in CondOp::ALL {
        let mapped = map_rule(&rule(*op, "1"));
        assert!(
            !matches!(mapped, XlsxRule::Bake),
            "{op:?} baked unexpectedly with a numeric, case-insensitive operand"
        );
    }
}

#[test]
fn case_sensitive_eq_and_contains_bake() {
    // Excel's native `cellIs` equal/not-equal compiles to `=` and its
    // `containsText` compiles to `SEARCH()`; both are unconditionally
    // case-insensitive. A case-sensitive Octa rule would therefore colour
    // more cells as a native rule than it does on screen, so it bakes.
    assert_eq!(
        map_rule(&rule_cs(CondOp::Eq, "Paid", true)),
        XlsxRule::Bake,
        "case-sensitive Eq must bake"
    );
    assert_eq!(
        map_rule(&rule_cs(CondOp::Contains, "Paid", true)),
        XlsxRule::Bake,
        "case-sensitive Contains must bake"
    );
}

#[test]
fn case_insensitive_eq_and_contains_map_natively() {
    assert!(
        matches!(
            map_rule(&rule_cs(CondOp::Eq, "Paid", false)),
            XlsxRule::Cell { .. }
        ),
        "case-insensitive Eq must map to a native rule"
    );
    assert!(
        matches!(
            map_rule(&rule_cs(CondOp::Contains, "Paid", false)),
            XlsxRule::Text { .. }
        ),
        "case-insensitive Contains must map to a native rule"
    );
}

#[test]
fn mark_colours_are_opaque_rgb() {
    // The screen palette is translucent (alpha 90) so the grid shows through.
    // A spreadsheet fill has no alpha, so the exported colour is the opaque
    // form of the same hue.
    assert_eq!(mark_rgb(MarkColor::Red), 0xDC2626);
    assert_eq!(mark_rgb(MarkColor::Orange), 0xEA580C);
    assert_eq!(mark_rgb(MarkColor::Yellow), 0xFACC15);
    assert_eq!(mark_rgb(MarkColor::Green), 0x22C55E);
    assert_eq!(mark_rgb(MarkColor::Blue), 0x3B82F6);
    assert_eq!(mark_rgb(MarkColor::Purple), 0xA855F7);
}

#[test]
fn number_format_codes_match_the_display() {
    let two = NumberFormat {
        decimals: Some(2),
        rounding: RoundingMode::Normal,
    };
    assert_eq!(num_format_code(&two, false), "0.00");
    assert_eq!(num_format_code(&two, true), "#,##0.00");

    let zero = NumberFormat {
        decimals: Some(0),
        rounding: RoundingMode::Normal,
    };
    assert_eq!(num_format_code(&zero, true), "#,##0");

    // Negative decimals round before the point. Excel has no such code, so the
    // cell displays as a whole number and the rounding is applied to the value
    // by the existing round-on-save path.
    let hundreds = NumberFormat {
        decimals: Some(-2),
        rounding: RoundingMode::Normal,
    };
    assert_eq!(num_format_code(&hundreds, true), "#,##0");

    // Grouping-only (decimals: None) is cosmetic; without grouping there is
    // nothing to express and the cell needs no format at all.
    let auto = NumberFormat {
        decimals: None,
        rounding: RoundingMode::Normal,
    };
    assert_eq!(num_format_code(&auto, true), "#,##0.##########");
    assert_eq!(num_format_code(&auto, false), "");
}
