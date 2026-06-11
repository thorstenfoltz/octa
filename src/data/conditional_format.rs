//! Conditional formatting: rule-based cell colouring. A list of [`CondRule`]s
//! is evaluated against each visible cell; the first matching rule decides the
//! cell's background colour (reusing the [`MarkColor`] palette so the renderer
//! treats a conditional colour exactly like a manual mark).
//!
//! The logic is pure and lives here so it stays testable and could be reused
//! (e.g. from the CLI/MCP later). The session-scoped rule list lives on the
//! GUI's `TabState`; the dialog in `src/app/dialogs/conditional_format.rs`
//! edits it.

use crate::data::MarkColor;

/// Comparison operator for a conditional-formatting rule. Numeric operators
/// (`Gt`/`Lt`/`Ge`/`Le`) compare numerically when both the cell and the rule
/// value parse as numbers, otherwise they fall back to a string comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CondOp {
    Eq,
    Ne,
    Contains,
    NotContains,
    Gt,
    Lt,
    Ge,
    Le,
    Empty,
    NotEmpty,
}

impl CondOp {
    pub const ALL: &'static [CondOp] = &[
        CondOp::Eq,
        CondOp::Ne,
        CondOp::Contains,
        CondOp::NotContains,
        CondOp::Gt,
        CondOp::Lt,
        CondOp::Ge,
        CondOp::Le,
        CondOp::Empty,
        CondOp::NotEmpty,
    ];

    /// Stable English label (the dialog shows `label_t` for localisation).
    pub fn label(self) -> &'static str {
        match self {
            CondOp::Eq => "equals",
            CondOp::Ne => "does not equal",
            CondOp::Contains => "contains",
            CondOp::NotContains => "does not contain",
            CondOp::Gt => "greater than",
            CondOp::Lt => "less than",
            CondOp::Ge => "greater or equal",
            CondOp::Le => "less or equal",
            CondOp::Empty => "is empty",
            CondOp::NotEmpty => "is not empty",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            CondOp::Eq => "cond_op.eq",
            CondOp::Ne => "cond_op.ne",
            CondOp::Contains => "cond_op.contains",
            CondOp::NotContains => "cond_op.not_contains",
            CondOp::Gt => "cond_op.gt",
            CondOp::Lt => "cond_op.lt",
            CondOp::Ge => "cond_op.ge",
            CondOp::Le => "cond_op.le",
            CondOp::Empty => "cond_op.empty",
            CondOp::NotEmpty => "cond_op.not_empty",
        })
    }

    /// Whether this operator uses the rule's `value` field. `Empty`/`NotEmpty`
    /// ignore it, so the dialog can grey out the value box.
    pub fn uses_value(self) -> bool {
        !matches!(self, CondOp::Empty | CondOp::NotEmpty)
    }
}

/// One conditional-formatting rule.
#[derive(Debug, Clone)]
pub struct CondRule {
    /// Column this rule applies to, or `None` to apply to every column.
    pub column: Option<usize>,
    pub op: CondOp,
    /// Comparison operand (ignored for `Empty`/`NotEmpty`).
    pub value: String,
    /// Background colour painted on matching cells.
    pub color: MarkColor,
    /// Case-sensitive text comparison when `true` (default `false`).
    pub case_sensitive: bool,
}

impl CondRule {
    pub fn new() -> Self {
        Self {
            column: None,
            op: CondOp::Eq,
            value: String::new(),
            color: MarkColor::Yellow,
            case_sensitive: false,
        }
    }
}

impl Default for CondRule {
    fn default() -> Self {
        Self::new()
    }
}

/// Does `cell` satisfy `rule`? `cell` is the cell's textual value.
pub fn rule_matches(rule: &CondRule, cell: &str) -> bool {
    match rule.op {
        CondOp::Empty => cell.is_empty(),
        CondOp::NotEmpty => !cell.is_empty(),
        CondOp::Gt | CondOp::Lt | CondOp::Ge | CondOp::Le => {
            // Numeric comparison when both sides parse as f64, else string.
            match (cell.trim().parse::<f64>(), rule.value.trim().parse::<f64>()) {
                (Ok(a), Ok(b)) => match rule.op {
                    CondOp::Gt => a > b,
                    CondOp::Lt => a < b,
                    CondOp::Ge => a >= b,
                    CondOp::Le => a <= b,
                    _ => unreachable!(),
                },
                _ => {
                    let (a, b) = norm(cell, &rule.value, rule.case_sensitive);
                    match rule.op {
                        CondOp::Gt => a > b,
                        CondOp::Lt => a < b,
                        CondOp::Ge => a >= b,
                        CondOp::Le => a <= b,
                        _ => unreachable!(),
                    }
                }
            }
        }
        CondOp::Eq | CondOp::Ne | CondOp::Contains | CondOp::NotContains => {
            let (a, b) = norm(cell, &rule.value, rule.case_sensitive);
            match rule.op {
                CondOp::Eq => a == b,
                CondOp::Ne => a != b,
                CondOp::Contains => a.contains(&b),
                CondOp::NotContains => !a.contains(&b),
                _ => unreachable!(),
            }
        }
    }
}

/// Lowercase both operands unless the rule is case-sensitive.
fn norm(cell: &str, value: &str, case_sensitive: bool) -> (String, String) {
    if case_sensitive {
        (cell.to_string(), value.to_string())
    } else {
        (cell.to_lowercase(), value.to_lowercase())
    }
}

/// First rule (in list order) that matches `cell` in column `col`, returning
/// its colour. A rule with `column == None` applies to every column.
pub fn match_color(rules: &[CondRule], col: usize, cell: &str) -> Option<MarkColor> {
    rules
        .iter()
        .find(|r| r.column.map(|c| c == col).unwrap_or(true) && rule_matches(r, cell))
        .map(|r| r.color)
}

#[cfg(test)]
mod tests {
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
}
