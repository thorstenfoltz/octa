//! Unit tests for [`conditional_format`](conditional_format). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

fn rule(op: CondOp, value: &str) -> CondRule {
    CondRule {
        op,
        value: value.to_string(),
        ..CondRule::new()
    }
}

#[test]
fn numeric_ops_compare_numerically() {
    assert!(rule_matches(&rule(CondOp::Gt, "10"), "100"));
    assert!(!rule_matches(&rule(CondOp::Gt, "10"), "9"));
    assert!(rule_matches(&rule(CondOp::Le, "5"), "5"));
}

#[test]
fn text_ops_are_case_insensitive_by_default() {
    assert!(rule_matches(&rule(CondOp::Eq, "Done"), "done"));
    assert!(rule_matches(&rule(CondOp::Contains, "err"), "Fatal ERROR"));
}

#[test]
fn case_sensitive_when_requested() {
    let mut r = rule(CondOp::Eq, "Done");
    r.case_sensitive = true;
    assert!(!rule_matches(&r, "done"));
    assert!(rule_matches(&r, "Done"));
}

#[test]
fn empty_ops_ignore_value() {
    assert!(rule_matches(&rule(CondOp::Empty, ""), ""));
    assert!(rule_matches(&rule(CondOp::NotEmpty, ""), "x"));
}

#[test]
fn first_matching_rule_wins_and_column_scoping() {
    let rules = vec![
        CondRule {
            column: Some(1),
            op: CondOp::Eq,
            value: "x".into(),
            color: MarkColor::Red,
            case_sensitive: false,
        },
        CondRule {
            column: None,
            op: CondOp::Eq,
            value: "x".into(),
            color: MarkColor::Blue,
            case_sensitive: false,
        },
    ];
    // Column 0: the col-1 rule is skipped, the all-column rule matches.
    assert_eq!(match_color(&rules, 0, "x"), Some(MarkColor::Blue));
    // Column 1: the col-1 rule wins by order.
    assert_eq!(match_color(&rules, 1, "x"), Some(MarkColor::Red));
    assert_eq!(match_color(&rules, 0, "y"), None);
}
