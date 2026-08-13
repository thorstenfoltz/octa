//! Detection and parsing of numbers written in the English (`1,234.56`) and
//! European (`1.234,56`) conventions.
//!
//! Rust's `str::parse::<f64>()` accepts only the bare English form, so a German
//! or French export arrives as text and stays text: no sums, no sorting by
//! size, no arithmetic. This module decides per **column**, never per cell,
//! because a single `1,234` is genuinely ambiguous and only its neighbours can
//! resolve it.
//!
//! Shaped deliberately like `date_infer`: classify, infer for a column, apply.

use crate::data::{CellValue, DataTable};

/// Which convention a value or column is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberStyle {
    /// `1,234.56` - comma groups, dot decimal.
    English,
    /// `1.234,56` - dot (or space) groups, comma decimal.
    European,
}

impl NumberStyle {
    /// Label for the ambiguity dialog and the banner. ASCII only (egui font).
    pub fn label(self) -> &'static str {
        match self {
            NumberStyle::English => "1,234.56 (English)",
            NumberStyle::European => "1.234,56 (European)",
        }
    }
}

/// What a single value tells us on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueStyle {
    /// The value can only be read one way.
    Certain(NumberStyle),
    /// Numeric, but readable either way (`1,234`, `1.234`, `1234`).
    Ambiguous,
    /// Not a number under either convention.
    NotNumeric,
}

/// What to do with a whole column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberOutcome {
    /// Leave the column alone.
    Skip,
    /// Convert it, reading values in this style.
    Promote(NumberStyle),
    /// Numeric but undecidable: the GUI asks, headless callers leave it as text.
    Ambiguous,
}

/// Group separators that are not `.` or `,`: ordinary space, non-breaking
/// space (U+00A0) and thin space (U+2009), all three of which turn up in
/// French and Scandinavian exports.
const SPACE_SEPARATORS: [char; 3] = [' ', '\u{00a0}', '\u{2009}'];

/// Is `integer_part` a valid integer with `group` as its thousands separator?
///
/// Groups after the first must be exactly three digits and the first one to
/// three. This is the guard that keeps `31.12.2024` out of the number pass:
/// its groups are 2 and 4 digits, valid under no convention.
fn grouping_ok(integer_part: &str, group: char) -> bool {
    if integer_part.is_empty() {
        return false;
    }
    if !integer_part.contains(group) {
        return integer_part.chars().all(|c| c.is_ascii_digit());
    }
    let parts: Vec<&str> = integer_part.split(group).collect();
    if parts[0].is_empty() || parts[0].len() > 3 {
        return false;
    }
    parts
        .iter()
        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        && parts[1..].iter().all(|p| p.len() == 3)
}

/// Does the whole value parse as a number under `style`?
fn value_valid_for(cleaned: &str, style: NumberStyle) -> bool {
    let (group, decimal) = match style {
        NumberStyle::English => (',', '.'),
        NumberStyle::European => ('.', ','),
    };
    let mut parts = cleaned.splitn(2, decimal);
    let integer_part = parts.next().unwrap_or("");
    if let Some(fraction) = parts.next()
        && (fraction.is_empty() || !fraction.chars().all(|c| c.is_ascii_digit()))
    {
        return false;
    }
    grouping_ok(integer_part, group)
}

/// Classify one value. The rules, in order:
///
/// - Both separators present: the **last** one is the decimal mark.
/// - One separator repeated (`1.234.567`) is grouping, so the other
///   convention's decimal mark is implied.
/// - One separator followed by exactly three digits is ambiguous.
/// - One separator followed by anything else is that convention's decimal mark.
/// - Space grouping only ever pairs with a comma decimal.
pub fn classify_value(s: &str) -> ValueStyle {
    let t = s.trim();
    if t.is_empty() {
        return ValueStyle::NotNumeric;
    }
    let body = t.strip_prefix(['-', '+']).unwrap_or(t);
    let has_space = body.chars().any(|c| SPACE_SEPARATORS.contains(&c));
    let cleaned: String = body
        .chars()
        .filter(|c| !SPACE_SEPARATORS.contains(c))
        .collect();
    if cleaned.is_empty()
        || !cleaned
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
    {
        return ValueStyle::NotNumeric;
    }

    let dots = cleaned.matches('.').count();
    let commas = cleaned.matches(',').count();

    let candidate = match (dots, commas) {
        // A bare number carries no evidence either way.
        (0, 0) => return ValueStyle::Ambiguous,
        (d, c) if d > 0 && c > 0 => {
            if cleaned.rfind(',') > cleaned.rfind('.') {
                NumberStyle::European
            } else {
                NumberStyle::English
            }
        }
        (d, 0) if d > 1 => NumberStyle::European,
        (0, c) if c > 1 => NumberStyle::English,
        (1, 0) => {
            let tail = cleaned.rsplit('.').next().unwrap_or("");
            if tail.len() == 3 {
                // 1.234 reads as European grouping or English decimal.
                return if value_valid_for(&cleaned, NumberStyle::European) {
                    ValueStyle::Ambiguous
                } else {
                    ValueStyle::NotNumeric
                };
            }
            NumberStyle::English
        }
        (0, 1) => {
            let tail = cleaned.rsplit(',').next().unwrap_or("");
            if tail.len() == 3 {
                return if value_valid_for(&cleaned, NumberStyle::English) {
                    ValueStyle::Ambiguous
                } else {
                    ValueStyle::NotNumeric
                };
            }
            NumberStyle::European
        }
        _ => return ValueStyle::NotNumeric,
    };

    if has_space && candidate == NumberStyle::English {
        return ValueStyle::NotNumeric;
    }
    if value_valid_for(&cleaned, candidate) {
        ValueStyle::Certain(candidate)
    } else {
        ValueStyle::NotNumeric
    }
}

/// Parse `s` reading it in `style`. Returns `None` when it is not a number.
pub fn parse_number(s: &str, style: NumberStyle) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (group, decimal) = match style {
        NumberStyle::English => (',', '.'),
        NumberStyle::European => ('.', ','),
    };
    let mut normalised = String::with_capacity(t.len());
    for c in t.chars() {
        if SPACE_SEPARATORS.contains(&c) || c == group {
            continue;
        }
        if c == decimal {
            normalised.push('.');
        } else {
            normalised.push(c);
        }
    }
    normalised.parse::<f64>().ok()
}

/// Parse without knowing the style: the plain English form first so existing
/// behaviour is untouched, then the European reading. Used by the manual
/// "convert this column to a number" paths, where the user has already said
/// they want a number.
pub fn parse_number_relaxed(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(v) = t.parse::<f64>() {
        return Some(v);
    }
    match classify_value(t) {
        ValueStyle::Certain(style) => parse_number(t, style),
        ValueStyle::Ambiguous => parse_number(t, NumberStyle::European),
        ValueStyle::NotNumeric => None,
    }
}

/// A column worth scanning: a text column. Typed columns (`Float64`, `Date32`,
/// ...) are already numbers or dates and must not be touched.
pub fn column_is_candidate(table: &DataTable, col: usize) -> bool {
    table
        .columns
        .get(col)
        .is_some_and(|c| c.data_type == "Utf8" || c.data_type == "String")
}

/// Convert every parseable cell of `col` to a number, reading values in
/// `style`, and set the column's type. Values that do not parse become
/// `CellValue::Null`, matching what the readers do with unparseable numerics.
///
/// A column whose values are all whole numbers becomes `Int64` rather than
/// `Float64`, so `1.234` (European) reads as the integer 1234 and displays
/// without a spurious `.0`.
pub fn apply_style(table: &mut DataTable, col: usize, style: NumberStyle) {
    let mut parsed: Vec<Option<f64>> = Vec::with_capacity(table.rows.len());
    for row in &table.rows {
        parsed.push(match row.get(col) {
            Some(CellValue::String(s)) if !s.trim().is_empty() => parse_number(s, style),
            _ => None,
        });
    }
    let all_integral = parsed
        .iter()
        .flatten()
        .all(|v| v.fract() == 0.0 && v.abs() < i64::MAX as f64);

    for (row, value) in parsed.into_iter().enumerate() {
        let Some(cell) = table.rows[row].get(col) else {
            continue;
        };
        // Leave non-string cells (nulls, already-typed values) alone.
        if !matches!(cell, CellValue::String(_)) {
            continue;
        }
        table.rows[row][col] = match value {
            Some(v) if all_integral => CellValue::Int(v as i64),
            Some(v) => CellValue::Float(v),
            None => CellValue::Null,
        };
    }
    if let Some(c) = table.columns.get_mut(col) {
        c.data_type = if all_integral { "Int64" } else { "Float64" }.to_string();
    }
}

/// Promote every column whose style is certain. Ambiguous columns are left as
/// text, which is the only safe answer when there is no user to ask.
/// Returns the indices that were promoted.
pub fn promote_certain_columns(table: &mut DataTable) -> Vec<usize> {
    let mut promoted = Vec::new();
    for col in 0..table.columns.len() {
        if !column_is_candidate(table, col) {
            continue;
        }
        // Scoped so the immutable borrow ends before `apply_style` takes a
        // mutable one. `collect_column_strings` returns an empty vec for any
        // column holding a non-string cell, which `infer_column` skips.
        let outcome = {
            let collected = crate::data::date_infer::collect_column_strings(table, col);
            infer_column(&collected)
        };
        if let NumberOutcome::Promote(style) = outcome {
            apply_style(table, col, style);
            promoted.push(col);
        }
    }
    promoted
}

/// One promoted column: its index, the style it was read in, and the strings
/// it held beforehand so a Dismiss can put them back.
pub type NumberPromotion = (usize, NumberStyle, Vec<Option<String>>);

/// GUI variant of [`promote_certain_columns`]: converts the certain columns and
/// hands back the original strings for the banner's Dismiss, plus the indices
/// of the columns that need a question asked.
pub fn promote_columns_with_snapshot(table: &mut DataTable) -> (Vec<NumberPromotion>, Vec<usize>) {
    let mut promoted = Vec::new();
    let mut ambiguous = Vec::new();
    for col in 0..table.columns.len() {
        if !column_is_candidate(table, col) {
            continue;
        }
        let outcome = {
            let collected = crate::data::date_infer::collect_column_strings(table, col);
            infer_column(&collected)
        };
        match outcome {
            NumberOutcome::Promote(style) => {
                let snapshot: Vec<Option<String>> = table
                    .rows
                    .iter()
                    .map(|r| match r.get(col) {
                        Some(CellValue::String(s)) => Some(s.clone()),
                        _ => None,
                    })
                    .collect();
                apply_style(table, col, style);
                promoted.push((col, style, snapshot));
            }
            NumberOutcome::Ambiguous => ambiguous.push(col),
            NumberOutcome::Skip => {}
        }
    }
    (promoted, ambiguous)
}

/// Decide what to do with a whole column. `values` is one entry per row,
/// `None` for null or non-string cells.
///
/// A column is promoted when at least one value is certain, no value carries
/// the opposite certainty, and every non-null value is numeric under some
/// reading. A column whose values are all ambiguous is `Ambiguous`. Anything
/// else is `Skip`, including columns of plain integers, which the readers
/// already type correctly on their own.
pub fn infer_column(values: &[Option<&str>]) -> NumberOutcome {
    let mut seen_european = false;
    let mut seen_english = false;
    let mut seen_ambiguous = false;
    let mut seen_any = false;

    for v in values.iter().flatten() {
        if v.trim().is_empty() {
            continue;
        }
        seen_any = true;
        match classify_value(v) {
            ValueStyle::Certain(NumberStyle::European) => seen_european = true,
            ValueStyle::Certain(NumberStyle::English) => seen_english = true,
            ValueStyle::Ambiguous => seen_ambiguous = true,
            ValueStyle::NotNumeric => return NumberOutcome::Skip,
        }
    }

    if !seen_any {
        return NumberOutcome::Skip;
    }
    match (seen_european, seen_english) {
        (true, true) => NumberOutcome::Skip,
        (true, false) => NumberOutcome::Promote(NumberStyle::European),
        (false, true) => NumberOutcome::Promote(NumberStyle::English),
        (false, false) if seen_ambiguous => {
            // All values ambiguous. A column of bare integers (`1234`) is not
            // worth a prompt: the readers already type those. Only separator
            // ambiguity (`1,234`) is worth asking about.
            if values.iter().flatten().any(|v| v.contains(['.', ','])) {
                NumberOutcome::Ambiguous
            } else {
                NumberOutcome::Skip
            }
        }
        _ => NumberOutcome::Skip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn european_decimal_is_certain() {
        assert_eq!(
            classify_value("3,14"),
            ValueStyle::Certain(NumberStyle::European)
        );
        assert_eq!(
            classify_value("1.234,56"),
            ValueStyle::Certain(NumberStyle::European)
        );
        assert_eq!(
            classify_value("1.234.567"),
            ValueStyle::Certain(NumberStyle::European)
        );
    }

    #[test]
    fn english_decimal_is_certain() {
        assert_eq!(
            classify_value("3.14"),
            ValueStyle::Certain(NumberStyle::English)
        );
        assert_eq!(
            classify_value("1,234.56"),
            ValueStyle::Certain(NumberStyle::English)
        );
        assert_eq!(
            classify_value("1,234,567"),
            ValueStyle::Certain(NumberStyle::English)
        );
    }

    #[test]
    fn lone_separator_with_three_digits_is_ambiguous() {
        assert_eq!(classify_value("1,234"), ValueStyle::Ambiguous);
        assert_eq!(classify_value("1.234"), ValueStyle::Ambiguous);
    }

    #[test]
    fn plain_and_non_numeric_values() {
        assert_eq!(classify_value("1234"), ValueStyle::Ambiguous);
        assert_eq!(classify_value("-42"), ValueStyle::Ambiguous);
        assert_eq!(classify_value("abc"), ValueStyle::NotNumeric);
        assert_eq!(classify_value(""), ValueStyle::NotNumeric);
        assert_eq!(classify_value("2024-01-31"), ValueStyle::NotNumeric);
    }

    /// The guard that keeps German dates out of the number pass: 31.12.2024 has
    /// groups of 2 and 4 digits, which is not valid grouping under either style.
    #[test]
    fn dotted_date_is_not_a_number() {
        assert_eq!(classify_value("31.12.2024"), ValueStyle::NotNumeric);
        assert_eq!(classify_value("1.12.2024"), ValueStyle::NotNumeric);
    }

    #[test]
    fn parses_both_styles() {
        assert_eq!(
            parse_number("1.234,56", NumberStyle::European),
            Some(1234.56)
        );
        // 3,25 rather than 3,14: clippy reads the latter as an approximation
        // of PI and denies the literal.
        assert_eq!(parse_number("3,25", NumberStyle::European), Some(3.25));
        assert_eq!(
            parse_number("1,234.56", NumberStyle::English),
            Some(1234.56)
        );
        assert_eq!(parse_number("1234", NumberStyle::European), Some(1234.0));
        assert_eq!(
            parse_number("-1.234,5", NumberStyle::European),
            Some(-1234.5)
        );
        assert_eq!(parse_number("abc", NumberStyle::English), None);
    }

    /// French exports separate groups with a space, including U+00A0 and U+2009.
    #[test]
    fn space_grouping_is_european() {
        assert_eq!(
            classify_value("1 234,56"),
            ValueStyle::Certain(NumberStyle::European)
        );
        assert_eq!(
            parse_number("1\u{00a0}234,56", NumberStyle::European),
            Some(1234.56)
        );
        assert_eq!(
            parse_number("1\u{2009}234,56", NumberStyle::European),
            Some(1234.56)
        );
    }

    #[test]
    fn relaxed_parse_prefers_plain_then_european() {
        // Unchanged from today's behaviour: a bare English decimal still wins.
        assert_eq!(parse_number_relaxed("1.234"), Some(1.234));
        assert_eq!(parse_number_relaxed("3,25"), Some(3.25));
        assert_eq!(parse_number_relaxed("1.234,56"), Some(1234.56));
        assert_eq!(parse_number_relaxed("nope"), None);
    }

    #[test]
    fn snapshot_promotion_reports_promoted_and_ambiguous() {
        use crate::data::ColumnInfo;
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "amount".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "code".into(),
                data_type: "Utf8".into(),
            },
        ];
        t.rows = vec![
            vec![
                CellValue::String("1.234,56".into()),
                CellValue::String("1,234".into()),
            ],
            vec![
                CellValue::String("9,90".into()),
                CellValue::String("2,345".into()),
            ],
        ];

        let (promoted, ambiguous) = promote_columns_with_snapshot(&mut t);

        assert_eq!(promoted.len(), 1);
        assert_eq!(promoted[0].0, 0);
        assert_eq!(promoted[0].1, NumberStyle::European);
        assert_eq!(promoted[0].2[0], Some("1.234,56".to_string()));
        assert_eq!(t.get(0, 0), Some(&CellValue::Float(1234.56)));
        assert_eq!(ambiguous, vec![1]);
        // The ambiguous column is untouched until the user answers.
        assert_eq!(t.columns[1].data_type, "Utf8");
    }

    #[test]
    fn column_inference_resolves_by_majority_of_certainty() {
        let european = [Some("1.234,56"), Some("1,20"), Some("1.234"), None];
        assert_eq!(
            infer_column(&european),
            NumberOutcome::Promote(NumberStyle::European)
        );

        let english = [Some("1,234.56"), Some("3.14"), None];
        assert_eq!(
            infer_column(&english),
            NumberOutcome::Promote(NumberStyle::English)
        );

        let ambiguous = [Some("1,234"), Some("2,345"), None];
        assert_eq!(infer_column(&ambiguous), NumberOutcome::Ambiguous);

        // A column with both certainties is a mixed mess: leave it alone.
        let conflicting = [Some("1.234,56"), Some("1,234.56")];
        assert_eq!(infer_column(&conflicting), NumberOutcome::Skip);

        // Nothing numeric, or already plain integers only: nothing to do.
        assert_eq!(
            infer_column(&[Some("abc"), Some("def")]),
            NumberOutcome::Skip
        );
        assert_eq!(infer_column(&[Some("12"), Some("34")]), NumberOutcome::Skip);
        assert_eq!(infer_column(&[]), NumberOutcome::Skip);

        // One stray word disqualifies the column: a real number column has no prose.
        assert_eq!(
            infer_column(&[Some("1.234,56"), Some("n/a")]),
            NumberOutcome::Skip
        );
    }
}
