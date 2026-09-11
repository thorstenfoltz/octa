//! Translating Octa's on-screen formatting into what a spreadsheet can hold.
//!
//! Pure and separate from the writer so the mapping decisions are unit-tested
//! without producing a workbook. The writer in `excel_reader.rs` consumes
//! [`XlsxRule`] and does no deciding of its own.

use crate::data::MarkColor;
use crate::data::conditional_format::{CondOp, CondRule};
use crate::data::num_format::NumberFormat;
use crate::data::validation::{ValidationKind, ValidationRule};

/// What a [`CondRule`] becomes in a workbook.
///
/// `Bake` means the rule cannot be expressed as a live Excel rule and its
/// colour must instead be painted onto the cells that match it today.
#[derive(Debug, Clone, PartialEq)]
pub enum XlsxRule {
    /// A `cellIs` rule. `numeric` carries the operand when it parsed as a
    /// number; otherwise the operand is the rule's text.
    Cell {
        op: CondOp,
        numeric: Option<f64>,
        text: String,
    },
    /// A `containsText` / `notContainsText` rule.
    Text { contains: bool, needle: String },
    /// A blank / non-blank rule.
    Blank { inverted: bool },
    /// Not expressible; paint the matching cells instead.
    Bake,
}

/// What a [`ValidationRule`] becomes in a workbook.
///
/// Kinds Excel has no equivalent for map to `None` and are **skipped**, not
/// approximated: the same honesty that makes [`map_rule`] bake a rule it
/// cannot express rather than emit a wrong live one. A workbook that silently
/// enforces something other than what Octa checks is worse than one that
/// enforces nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum XlsxValidation {
    /// A number, open on a side whose bound is `None`.
    Decimal { min: Option<f64>, max: Option<f64> },
    /// Text of at most this many characters.
    MaxLength(u32),
    /// The cell must not be empty.
    NotBlank,
}

/// Decide how one validation rule crosses into Excel.
///
/// `Regex` and `Unique` have no Excel counterpart, and a `Range` with no bound
/// on either side constrains nothing, so all three return `None`.
pub fn map_validation(rule: &ValidationRule) -> Option<XlsxValidation> {
    match &rule.kind {
        ValidationKind::NotNull => Some(XlsxValidation::NotBlank),
        ValidationKind::Range { min, max } if min.is_some() || max.is_some() => {
            Some(XlsxValidation::Decimal {
                min: *min,
                max: *max,
            })
        }
        ValidationKind::Range { .. } => None,
        ValidationKind::MaxLength(n) => u32::try_from(*n).ok().map(XlsxValidation::MaxLength),
        ValidationKind::Regex(_) | ValidationKind::Unique => None,
    }
}

/// Decide how one rule crosses into Excel.
///
/// Ordering operators over **text** bake on purpose: Excel compares text by
/// locale collation while `conditional_format::rule_matches` compares by Rust
/// string ordering, so a native rule would colour different cells than Octa.
///
/// `Eq`/`Ne`/`Contains`/`NotContains` bake when `rule.case_sensitive` is true:
/// Octa's `rule_matches` lowercases both operands unless the rule asks for
/// case sensitivity, but Excel's native equivalents are unconditionally
/// case-insensitive (`cellIs` compiles to `=`, `containsText` to `SEARCH()`),
/// so a case-sensitive native rule would colour more cells than Octa does.
///
/// Accepted divergence: `Eq`/`Ne` export with a numeric operand when
/// `rule.value` parses as a number, while Octa itself always compares those
/// two operators as text. A cell whose text form differs from its numeric
/// form (`"42.0"` against a rule value of `"42"`) can therefore colour
/// differently in Excel. This is accepted rather than engineered around.
pub fn map_rule(rule: &CondRule) -> XlsxRule {
    let numeric = rule.value.trim().parse::<f64>().ok();
    match rule.op {
        CondOp::Eq | CondOp::Ne if rule.case_sensitive => XlsxRule::Bake,
        CondOp::Eq | CondOp::Ne => XlsxRule::Cell {
            op: rule.op,
            numeric,
            text: rule.value.clone(),
        },
        CondOp::Gt | CondOp::Lt | CondOp::Ge | CondOp::Le => match numeric {
            Some(_) => XlsxRule::Cell {
                op: rule.op,
                numeric,
                text: rule.value.clone(),
            },
            None => XlsxRule::Bake,
        },
        CondOp::Contains | CondOp::NotContains if rule.case_sensitive => XlsxRule::Bake,
        CondOp::Contains => XlsxRule::Text {
            contains: true,
            needle: rule.value.clone(),
        },
        CondOp::NotContains => XlsxRule::Text {
            contains: false,
            needle: rule.value.clone(),
        },
        CondOp::Empty => XlsxRule::Blank { inverted: false },
        CondOp::NotEmpty => XlsxRule::Blank { inverted: true },
    }
}

/// Opaque RGB for a mark colour.
///
/// The screen palette is translucent (alpha 90) so grid lines show through a
/// marked cell. A spreadsheet fill has no alpha, so the export uses the opaque
/// form of the same hue, which is what `ThemeColors` already paints for the
/// solid swatches.
pub fn mark_rgb(color: MarkColor) -> u32 {
    match color {
        MarkColor::Red => 0xDC_26_26,
        MarkColor::Orange => 0xEA_58_0C,
        MarkColor::Yellow => 0xFA_CC_15,
        MarkColor::Green => 0x22_C5_5E,
        MarkColor::Blue => 0x3B_82_F6,
        MarkColor::Purple => 0xA8_55_F7,
    }
}

/// Excel number-format code for a column format.
///
/// Returns an empty string when the format expresses nothing a spreadsheet can
/// hold, in which case the writer leaves the cell unformatted.
///
/// A negative `decimals` rounds before the decimal point. Excel has no code for
/// that, so the displayed format is a plain whole number; the value itself is
/// already rounded by the existing round-on-save path when the user asks for
/// it.
pub fn num_format_code(fmt: &NumberFormat, thousands: bool) -> String {
    let group = if thousands { "#,##0" } else { "0" };
    match fmt.decimals {
        Some(d) if d > 0 => format!("{group}.{}", "0".repeat(d as usize)),
        Some(_) => group.to_string(),
        None if thousands => "#,##0.##########".to_string(),
        None => String::new(),
    }
}

/// Make a worksheet name Excel will accept, unique within one workbook.
///
/// Excel's rules: at most 31 characters, none of `[ ] : * ? / \`, not empty,
/// and unique. `taken` accumulates the names already used, so collisions are
/// numbered in input order and the caller does not have to track them.
///
/// Length is counted in characters, not bytes: 31 CJK characters are 93 bytes,
/// and truncating by bytes would both undershoot the limit and risk splitting a
/// character.
pub fn sanitize_sheet_name(raw: &str, taken: &mut Vec<String>) -> String {
    const FORBIDDEN: [char; 7] = ['[', ']', ':', '*', '?', '/', '\\'];
    const LIMIT: usize = 31;

    let cleaned: String = raw
        .trim()
        .chars()
        .map(|c| if FORBIDDEN.contains(&c) { '_' } else { c })
        .collect();
    let base: String = if cleaned.trim().is_empty() {
        "Sheet".to_string()
    } else {
        cleaned.chars().take(LIMIT).collect()
    };

    let mut candidate = base.clone();
    let mut n = 1usize;
    while taken.contains(&candidate) {
        n += 1;
        let suffix = format!("_{n}");
        let room = LIMIT.saturating_sub(suffix.chars().count());
        let stem: String = base.chars().take(room).collect();
        candidate = format!("{stem}{suffix}");
    }
    taken.push(candidate.clone());
    candidate
}

#[cfg(test)]
#[path = "xlsx_style_tests.rs"]
mod tests;
