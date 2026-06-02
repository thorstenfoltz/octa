//! Numeric display formatting: thousand separators and per-column rounding.
//!
//! This module is the single source of truth for how `Int` / `Float` cells
//! are rendered in the table view. It is purely a *display* concern - the
//! underlying `CellValue`s are never mutated by formatting, so Save / export
//! / CLI / MCP all keep full precision. The one exception is
//! [`round_value`], which the Save path uses to build an explicitly-rounded
//! snapshot when the user opts in.
//!
//! Both entry points are pure functions with no UI / settings dependency so
//! they're integration-testable.

use crate::data::CellValue;

/// How a per-column rounding format rounds at the requested decimal scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoundingMode {
    /// Round half away from zero (the usual "commercial" rounding).
    #[default]
    Normal,
    /// Always round toward positive infinity (ceil at the scale).
    Up,
    /// Always round toward negative infinity (floor at the scale).
    Down,
}

impl RoundingMode {
    pub fn label(self) -> &'static str {
        match self {
            RoundingMode::Normal => "Normal",
            RoundingMode::Up => "Up",
            RoundingMode::Down => "Down",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            RoundingMode::Normal => "enum2.round_normal",
            RoundingMode::Up => "enum2.round_up",
            RoundingMode::Down => "enum2.round_down",
        })
    }

    pub const ALL: &'static [RoundingMode] =
        &[RoundingMode::Normal, RoundingMode::Up, RoundingMode::Down];
}

/// Grouping + decimal-mark convention. English uses `1,234,567.89`;
/// European uses `1.234.567,89` (period groups, comma decimal). The choice
/// drives both the thousands separator and the decimal mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SeparatorStyle {
    #[default]
    English,
    European,
}

impl SeparatorStyle {
    pub fn label(self) -> &'static str {
        match self {
            SeparatorStyle::English => "English (1,234.56)",
            SeparatorStyle::European => "European (1.234,56)",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            SeparatorStyle::English => "enum.sep_english",
            SeparatorStyle::European => "enum.sep_european",
        })
    }

    pub const ALL: &'static [SeparatorStyle] = &[SeparatorStyle::English, SeparatorStyle::European];

    /// Thousands-grouping character.
    fn group(self) -> char {
        match self {
            SeparatorStyle::English => ',',
            SeparatorStyle::European => '.',
        }
    }

    /// Decimal-mark character.
    fn decimal(self) -> char {
        match self {
            SeparatorStyle::English => '.',
            SeparatorStyle::European => ',',
        }
    }
}

/// Per-column number format. `decimals: None` means "leave the natural
/// precision". `Some(n)` with `n >= 0` rounds to `n` digits after the decimal
/// point and pads with trailing zeros. `Some(n)` with `n < 0` rounds *before*
/// the decimal point, e.g. `-2` rounds to the nearest 100 and shows no
/// fractional digits.
///
/// Derives `Eq` (no raw floats) so it can live in maps / be compared cheaply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NumberFormat {
    pub decimals: Option<i32>,
    pub rounding: RoundingMode,
}

impl NumberFormat {
    /// Whether this format actually changes stored values when applied on
    /// save. Only a fixed-decimal format rounds; a format with `decimals:
    /// None` is purely cosmetic (grouping only).
    pub fn rounds_values(&self) -> bool {
        self.decimals.is_some()
    }
}

/// Round `v` to `fmt.decimals` places using `fmt.rounding`. A negative
/// `decimals` rounds before the decimal point (`-2` -> nearest 100). Returns
/// `v` unchanged when `decimals` is `None` or `v` is non-finite.
pub fn round_value(v: f64, fmt: NumberFormat) -> f64 {
    let Some(decimals) = fmt.decimals else {
        return v;
    };
    if !v.is_finite() {
        return v;
    }
    let scale = 10f64.powi(decimals);
    let scaled = v * scale;
    let rounded = match fmt.rounding {
        // `f64::round` already rounds half away from zero.
        RoundingMode::Normal => scaled.round(),
        RoundingMode::Up => scaled.ceil(),
        RoundingMode::Down => scaled.floor(),
    };
    rounded / scale
}

/// Format an `Int` / `Float` cell for display. Returns `None` for any other
/// variant so the caller falls back to its normal rendering.
///
/// `fmt` applies rounding + fixed-decimal padding (display only); `thousands`
/// toggles grouping of the integer part; `style` selects English vs European
/// grouping/decimal marks (and applies to the decimal mark even when
/// `thousands` is off).
pub fn format_cell_number(
    value: &CellValue,
    fmt: Option<NumberFormat>,
    thousands: bool,
    style: SeparatorStyle,
) -> Option<String> {
    let base = match value {
        CellValue::Int(n) => format_int(*n, fmt),
        CellValue::Float(f) => format_float(*f, fmt),
        _ => return None,
    };
    Some(apply_separators(&base, thousands, style))
}

fn format_int(n: i64, fmt: Option<NumberFormat>) -> String {
    match fmt {
        Some(nf) if nf.decimals.is_some() => format_rounded(n as f64, nf),
        // No fixed decimals: exact integer text (no f64 round-trip).
        _ => n.to_string(),
    }
}

fn format_float(f: f64, fmt: Option<NumberFormat>) -> String {
    if !f.is_finite() {
        // Match CellValue::Display's behaviour for non-finite floats.
        return f.to_string();
    }
    match fmt {
        Some(nf) if nf.decimals.is_some() => format_rounded(f, nf),
        _ => {
            // No fixed-decimal format: mirror CellValue::Display so the
            // grouped output matches the un-grouped baseline.
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{:.1}", f)
            } else {
                format!("{}", f)
            }
        }
    }
}

/// Round `v` and format it with `max(0, decimals)` fractional digits, using a
/// `.` decimal mark (separators are applied later by [`apply_separators`]).
fn format_rounded(v: f64, nf: NumberFormat) -> String {
    if !v.is_finite() {
        return v.to_string();
    }
    let rounded = round_value(v, nf);
    // `-0.00` is ugly; normalise a rounded-to-zero result.
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    let frac = nf.decimals.unwrap_or(0).max(0) as usize;
    format!("{:.*}", frac, rounded)
}

/// Insert thousands separators and the locale decimal mark into a numeric
/// string produced with a `.` decimal mark. Handles a leading sign and an
/// optional fractional part; the fractional digits are left untouched (only
/// the decimal mark changes).
fn apply_separators(s: &str, thousands: bool, style: SeparatorStyle) -> String {
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", s),
    };
    let (int_part, frac_part) = match rest.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (rest, None),
    };

    // Only touch pure-digit integer parts (guards against NaN / inf / exp).
    if int_part.is_empty() || !int_part.bytes().all(|b| b.is_ascii_digit()) {
        return s.to_string();
    }

    let grouped = if thousands {
        let mut g = String::with_capacity(int_part.len() + int_part.len() / 3);
        for (i, c) in int_part.chars().enumerate() {
            if i > 0 && (int_part.len() - i).is_multiple_of(3) {
                g.push(style.group());
            }
            g.push(c);
        }
        g
    } else {
        int_part.to_string()
    };

    let mut out =
        String::with_capacity(sign.len() + grouped.len() + 1 + frac_part.map_or(0, |f| f.len()));
    out.push_str(sign);
    out.push_str(&grouped);
    if let Some(f) = frac_part {
        out.push(style.decimal());
        out.push_str(f);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nf(decimals: Option<i32>, rounding: RoundingMode) -> NumberFormat {
        NumberFormat { decimals, rounding }
    }

    const EN: SeparatorStyle = SeparatorStyle::English;
    const EU: SeparatorStyle = SeparatorStyle::European;

    #[test]
    fn groups_integers() {
        assert_eq!(
            format_cell_number(&CellValue::Int(1234567), None, true, EN).unwrap(),
            "1,234,567"
        );
        assert_eq!(
            format_cell_number(&CellValue::Int(-42000), None, true, EN).unwrap(),
            "-42,000"
        );
        assert_eq!(
            format_cell_number(&CellValue::Int(999), None, true, EN).unwrap(),
            "999"
        );
    }

    #[test]
    fn european_style() {
        assert_eq!(
            format_cell_number(&CellValue::Float(1234567.89), None, true, EU).unwrap(),
            "1.234.567,89"
        );
        // Decimal mark switches even with grouping off.
        assert_eq!(
            format_cell_number(&CellValue::Float(1234.5), None, false, EU).unwrap(),
            "1234,5"
        );
    }

    #[test]
    fn grouping_off_leaves_plain() {
        assert_eq!(
            format_cell_number(&CellValue::Int(1234567), None, false, EN).unwrap(),
            "1234567"
        );
    }

    #[test]
    fn groups_float_integer_part_only() {
        assert_eq!(
            format_cell_number(&CellValue::Float(1234567.89), None, true, EN).unwrap(),
            "1,234,567.89"
        );
    }

    #[test]
    fn fixed_decimals_pad_with_zeros() {
        let f = nf(Some(2), RoundingMode::Normal);
        assert_eq!(
            format_cell_number(&CellValue::Float(2.5), Some(f), false, EN).unwrap(),
            "2.50"
        );
        assert_eq!(
            format_cell_number(&CellValue::Int(3), Some(f), false, EN).unwrap(),
            "3.00"
        );
    }

    #[test]
    fn rounding_modes() {
        let normal = nf(Some(2), RoundingMode::Normal);
        let up = nf(Some(2), RoundingMode::Up);
        let down = nf(Some(2), RoundingMode::Down);
        assert_eq!(round_value(1.45678, normal), 1.46);
        assert_eq!(round_value(1.45678, up), 1.46);
        assert_eq!(round_value(1.45123, up), 1.46);
        assert_eq!(round_value(1.45678, down), 1.45);
        // Negative numbers: Up = toward +inf, Down = toward -inf.
        assert_eq!(round_value(-1.231, up), -1.23);
        assert_eq!(round_value(-1.231, down), -1.24);
        // Half away from zero.
        assert_eq!(round_value(2.5, nf(Some(0), RoundingMode::Normal)), 3.0);
        assert_eq!(round_value(-2.5, nf(Some(0), RoundingMode::Normal)), -3.0);
    }

    #[test]
    fn negative_decimals_round_before_point() {
        // Round to the nearest 100.
        let f = nf(Some(-2), RoundingMode::Normal);
        assert_eq!(round_value(1234.5, f), 1200.0);
        assert_eq!(
            format_cell_number(&CellValue::Float(1234.5), Some(f), true, EN).unwrap(),
            "1,200"
        );
        assert_eq!(
            format_cell_number(&CellValue::Int(1789), Some(f), false, EN).unwrap(),
            "1800"
        );
    }

    #[test]
    fn rounding_and_grouping_compose() {
        let f = nf(Some(2), RoundingMode::Normal);
        assert_eq!(
            format_cell_number(&CellValue::Float(1234567.899), Some(f), true, EN).unwrap(),
            "1,234,567.90"
        );
    }

    #[test]
    fn non_numeric_returns_none() {
        assert!(format_cell_number(&CellValue::String("hi".into()), None, true, EN).is_none());
        assert!(format_cell_number(&CellValue::Null, None, true, EN).is_none());
    }

    #[test]
    fn non_finite_floats_pass_through() {
        assert_eq!(
            format_cell_number(&CellValue::Float(f64::NAN), None, true, EN).unwrap(),
            "NaN"
        );
        assert_eq!(
            round_value(f64::INFINITY, nf(Some(2), RoundingMode::Normal)),
            f64::INFINITY
        );
    }
}
