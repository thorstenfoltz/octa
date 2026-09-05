//! Benford's law: does a column's leading digits look like real measurements?
//!
//! In numbers that arise from measuring or accumulating things across several
//! orders of magnitude, a leading 1 turns up about 30% of the time and a
//! leading 9 under 5%. Invented, transcribed or capped figures usually do not
//! do that, which is why auditors reach for this.
//!
//! **The hard part is not the maths, it is saying when the test does not
//! apply**, and most columns it does not. A test that answers "nonconforming"
//! for a column of ages, postcodes or row numbers is worse than no test: every
//! one of those *should* break the law, and a verdict on them trains the reader
//! to ignore the column. So four gates come first, and a column that trips one
//! reports the reason instead of a verdict, spelled out in words rather than as
//! a code the reader has to look up.

use crate::data::CellValue;

/// The expected share of a leading digit under Benford's law.
///
/// Computed, not transcribed: the law *is* `log10(1 + 1/d)`, and a table of
/// nine literals is nine chances to get a digit wrong. (It also stops clippy
/// mistaking the first entry for `LOG10_2`, which it happens to equal.)
pub fn expected_share(digit: u32) -> f64 {
    (1.0 + 1.0 / digit as f64).log10()
}

/// Below this many usable values the digit shares are too noisy to read.
/// Nigrini's rule of thumb, and the reason a small file gets no verdict.
pub const MIN_VALUES: usize = 300;

/// Values must span at least one order of magnitude, i.e. `max / min >= 10`.
/// A column that does not is a bounded range - a percentage, an age, a rating -
/// and bounded ranges have no reason to follow the law.
pub const MIN_SPREAD: f64 = 10.0;

/// How densely packed the distinct integers have to be before the column reads
/// as an assigned sequence rather than a measurement. `1.1` allows a tenth of
/// the run to be missing and still count as one.
pub const IDENTIFIER_DENSITY: f64 = 1.1;

/// Why the test was not run. Each of these is a column that *should* break the
/// law, so reporting a verdict would be actively misleading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotApplicable {
    /// The column is not numeric.
    NotNumeric,
    /// Fewer than [`MIN_VALUES`] usable values.
    TooFewValues,
    /// Values span less than one order of magnitude: a bounded range.
    NarrowRange,
    /// Dense, distinct integers: a row number, an invoice number, an id.
    Identifier,
}

impl NotApplicable {
    /// The report cell. English and stable, like `pii_kind`'s, but written to
    /// be read: these land in a table a person looks at, and a reader who has
    /// to decode `na_narrow_range` has been given a puzzle rather than an
    /// answer. The `not tested:` lead keeps them from being mistaken for a
    /// finding, which is the whole job the `na_` prefix used to do.
    pub fn id(self) -> &'static str {
        match self {
            Self::NotNumeric => "not tested: not numbers",
            Self::TooFewValues => "not tested: too few values",
            Self::NarrowRange => "not tested: narrow range",
            Self::Identifier => "not tested: looks like ids",
        }
    }
}

/// How far the column's leading digits are from the expected shares.
///
/// The bands are Nigrini's for the first digit, in mean absolute deviation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// MAD below 0.006.
    Conforms,
    /// MAD 0.006 to 0.012.
    Acceptable,
    /// MAD 0.012 to 0.015.
    Marginal,
    /// MAD above 0.015: the digits do not look like measured quantities.
    Nonconforming,
}

impl Verdict {
    pub fn id(self) -> &'static str {
        match self {
            Self::Conforms => "conforms",
            Self::Acceptable => "acceptable",
            Self::Marginal => "marginal",
            Self::Nonconforming => "nonconforming",
        }
    }

    fn from_mad(mad: f64) -> Self {
        if mad < 0.006 {
            Self::Conforms
        } else if mad < 0.012 {
            Self::Acceptable
        } else if mad < 0.015 {
            Self::Marginal
        } else {
            Self::Nonconforming
        }
    }
}

/// What the column has to say, if anything.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    Skipped(NotApplicable),
    Tested {
        verdict: Verdict,
        /// Mean absolute deviation from the expected shares.
        mad: f64,
        /// How many values the verdict rests on.
        values: usize,
    },
}

impl Outcome {
    /// The report cell: a verdict, or the reason the test did not run. English
    /// and stable, the same convention `pii_flag` and `pii_kind` already use;
    /// the localized explanation is the cell's own hover text, from
    /// [`VALUE_HINTS`].
    pub fn id(self) -> &'static str {
        match self {
            Self::Skipped(reason) => reason.id(),
            Self::Tested { verdict, .. } => verdict.id(),
        }
    }
}

/// Every value `benford_verdict` can hold, each with the i18n key that says
/// what it means and what to do about it. The GUI hangs these on the cells as
/// hover text, so a verdict explains itself where it is read rather than in
/// documentation the reader has to go and find.
///
/// Spelled out rather than built from `id()`, because a const cannot call one.
/// `every_benford_value_is_explained` is what keeps the two in step: a new
/// verdict or a new gate that forgets its entry here fails the test rather
/// than shipping a cell nothing explains.
pub const VALUE_HINTS: &[(&str, &str)] = &[
    ("conforms", "quality.verdict_benford_conforms"),
    ("acceptable", "quality.verdict_benford_acceptable"),
    ("marginal", "quality.verdict_benford_marginal"),
    ("nonconforming", "quality.verdict_benford_nonconforming"),
    (
        "not tested: not numbers",
        "quality.verdict_benford_not_numbers",
    ),
    (
        "not tested: too few values",
        "quality.verdict_benford_too_few",
    ),
    ("not tested: narrow range", "quality.verdict_benford_narrow"),
    ("not tested: looks like ids", "quality.verdict_benford_ids"),
];

/// The leading significant digit of `v`, or `None` for zero and non-finite
/// values. Sign is irrelevant: a debit of -412 leads with a 4.
pub fn first_digit(v: f64) -> Option<u32> {
    let v = v.abs();
    if !v.is_finite() || v == 0.0 {
        return None;
    }
    // Divide down (or up) into [1, 10) rather than formatting the number:
    // `format!` would round 9.999 to "10" and hand back a 1.
    let mut x = v;
    while x >= 10.0 {
        x /= 10.0;
    }
    while x < 1.0 {
        x *= 10.0;
    }
    let d = x as u32;
    (1..=9).contains(&d).then_some(d)
}

/// Run the test over one column's cells.
pub fn analyse(cells: &[&CellValue], data_type: &str) -> Outcome {
    if !crate::data::is_numeric_data_type(data_type) {
        return Outcome::Skipped(NotApplicable::NotNumeric);
    }
    let values: Vec<f64> = cells.iter().filter_map(|c| numeric(c)).collect();
    if let Some(reason) = disqualify(&values) {
        return Outcome::Skipped(reason);
    }

    let mut counts = [0usize; 9];
    let mut total = 0usize;
    for v in &values {
        if let Some(d) = first_digit(*v) {
            counts[(d - 1) as usize] += 1;
            total += 1;
        }
    }
    // `disqualify` already established there are enough usable values, but a
    // column of zeros could still get here with none of them leading.
    if total < MIN_VALUES {
        return Outcome::Skipped(NotApplicable::TooFewValues);
    }

    let mad = counts
        .iter()
        .enumerate()
        .map(|(i, &c)| (c as f64 / total as f64 - expected_share(i as u32 + 1)).abs())
        .sum::<f64>()
        / 9.0;

    Outcome::Tested {
        verdict: Verdict::from_mad(mad),
        mad,
        values: total,
    }
}

/// The three gates, in the order that costs least to check.
fn disqualify(values: &[f64]) -> Option<NotApplicable> {
    let usable: Vec<f64> = values
        .iter()
        .copied()
        .filter(|v| v.is_finite() && *v != 0.0)
        .collect();
    if usable.len() < MIN_VALUES {
        return Some(NotApplicable::TooFewValues);
    }

    let mut min = f64::INFINITY;
    let mut max: f64 = 0.0;
    for v in &usable {
        let a = v.abs();
        min = min.min(a);
        max = max.max(a);
    }
    if min <= 0.0 || max / min < MIN_SPREAD {
        return Some(NotApplicable::NarrowRange);
    }

    if looks_like_identifier(&usable) {
        return Some(NotApplicable::Identifier);
    }
    None
}

/// Whether the values read as an assigned sequence: every one a distinct whole
/// number, packed densely enough into its own range that it is a counter
/// rather than a measurement.
///
/// Density is the test, not distinctness alone. Fibonacci numbers are whole and
/// distinct and famously *do* follow the law; they are also spread across their
/// range by a factor of thousands, so they stay in.
fn looks_like_identifier(values: &[f64]) -> bool {
    if !values.iter().all(|v| v.fract() == 0.0) {
        return false;
    }
    let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for v in values {
        if *v > i64::MAX as f64 || *v < i64::MIN as f64 || !seen.insert(*v as i64) {
            return false;
        }
    }
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let span = max - min + 1.0;
    span <= values.len() as f64 * IDENTIFIER_DENSITY
}

fn numeric(cell: &CellValue) -> Option<f64> {
    match cell {
        CellValue::Int(i) => Some(*i as f64),
        CellValue::Float(f) => Some(*f),
        CellValue::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(values: &[f64]) -> Vec<CellValue> {
        values.iter().map(|v| CellValue::Float(*v)).collect()
    }

    fn analyse_values(values: &[f64]) -> Outcome {
        let owned = cells(values);
        let refs: Vec<&CellValue> = owned.iter().collect();
        analyse(&refs, "Float64")
    }

    #[test]
    fn every_benford_value_is_explained() {
        // VALUE_HINTS is spelled out rather than derived from `id()`, so this
        // is what stops the two drifting: every verdict and every gate has to
        // appear, and nothing may appear that no code can emit.
        let listed: Vec<&str> = VALUE_HINTS.iter().map(|(v, _)| *v).collect();
        let emitted = [
            Verdict::Conforms.id(),
            Verdict::Acceptable.id(),
            Verdict::Marginal.id(),
            Verdict::Nonconforming.id(),
            NotApplicable::NotNumeric.id(),
            NotApplicable::TooFewValues.id(),
            NotApplicable::NarrowRange.id(),
            NotApplicable::Identifier.id(),
        ];
        for value in emitted {
            assert!(listed.contains(&value), "{value} has no hint");
        }
        assert_eq!(listed.len(), emitted.len(), "VALUE_HINTS has a stray entry");
    }

    #[test]
    fn no_verdict_reads_as_a_finding() {
        // The `na_` prefix used to keep the four gates apart from the four real
        // verdicts. Words do that job now, so the lead has to stay.
        for (value, _) in VALUE_HINTS {
            let is_gate = value.starts_with("not tested: ");
            let is_verdict = matches!(
                *value,
                "conforms" | "acceptable" | "marginal" | "nonconforming"
            );
            assert!(is_gate != is_verdict, "{value} is neither, or both");
        }
    }

    #[test]
    fn leading_digit_ignores_sign_and_scale() {
        assert_eq!(first_digit(412.0), Some(4));
        assert_eq!(first_digit(-412.0), Some(4));
        assert_eq!(first_digit(0.00072), Some(7));
        assert_eq!(first_digit(9.999), Some(9), "must not round up to 10");
        assert_eq!(first_digit(0.0), None);
        assert_eq!(first_digit(f64::NAN), None);
    }

    /// Fibonacci is the textbook conforming sequence, and it doubles as the
    /// proof that the identifier gate does not swallow real data: the values
    /// are whole and distinct, but spread over their range, not packed into it.
    fn fibonacci(n: usize) -> Vec<f64> {
        let mut out = vec![1.0, 1.0];
        while out.len() < n {
            let next = out[out.len() - 1] + out[out.len() - 2];
            out.push(next);
        }
        out.truncate(n);
        out
    }

    #[test]
    fn a_conforming_distribution_conforms() {
        let outcome = analyse_values(&fibonacci(400));
        match outcome {
            Outcome::Tested {
                verdict,
                mad,
                values,
            } => {
                assert_eq!(verdict, Verdict::Conforms, "mad {mad}");
                assert_eq!(values, 400);
            }
            other => panic!("Fibonacci must be testable, got {other:?}"),
        }
    }

    /// Equal counts per leading digit is the clearest possible non-conformance.
    /// Fractional so the identifier gate cannot fire on it.
    fn flat_digits(per_digit: usize) -> Vec<f64> {
        let mut out = Vec::new();
        for d in 1..=9u32 {
            for i in 0..per_digit {
                let scale = 10f64.powi((i % 4) as i32);
                out.push((d as f64 + 0.5) * scale + i as f64 * 0.01);
            }
        }
        out
    }

    #[test]
    fn a_flat_distribution_does_not_conform() {
        match analyse_values(&flat_digits(40)) {
            Outcome::Tested { verdict, .. } => {
                assert_eq!(verdict, Verdict::Nonconforming);
            }
            other => panic!("expected a verdict, got {other:?}"),
        }
    }

    #[test]
    fn a_short_column_gets_no_verdict() {
        assert_eq!(
            analyse_values(&fibonacci(299)),
            Outcome::Skipped(NotApplicable::TooFewValues)
        );
    }

    /// A bounded range - a percentage, a rating, an age - has no reason to
    /// follow the law, so answering "nonconforming" for one would be a lie.
    #[test]
    fn a_bounded_range_gets_no_verdict() {
        let values: Vec<f64> = (0..500).map(|i| 20.0 + (i % 60) as f64).collect();
        assert_eq!(
            analyse_values(&values),
            Outcome::Skipped(NotApplicable::NarrowRange)
        );
    }

    #[test]
    fn a_row_number_column_gets_no_verdict() {
        let values: Vec<f64> = (1..=1000).map(|i| i as f64).collect();
        assert_eq!(
            analyse_values(&values),
            Outcome::Skipped(NotApplicable::Identifier)
        );
    }

    /// A sequence with a tenth of its numbers missing is still a sequence.
    #[test]
    fn a_gappy_sequence_still_reads_as_an_identifier() {
        let values: Vec<f64> = (1..=1000)
            .filter(|i| i % 20 != 0)
            .map(|i| i as f64)
            .collect();
        assert_eq!(
            analyse_values(&values),
            Outcome::Skipped(NotApplicable::Identifier)
        );
    }

    #[test]
    fn a_text_column_is_not_tested() {
        let owned = [CellValue::String("x".into())];
        let refs: Vec<&CellValue> = owned.iter().collect();
        assert_eq!(
            analyse(&refs, "Utf8"),
            Outcome::Skipped(NotApplicable::NotNumeric)
        );
    }

    #[test]
    fn every_outcome_has_a_stable_id() {
        assert_eq!(
            Outcome::Skipped(NotApplicable::NarrowRange).id(),
            "not tested: narrow range"
        );
        assert_eq!(
            Outcome::Tested {
                verdict: Verdict::Conforms,
                mad: 0.0,
                values: 300
            }
            .id(),
            "conforms"
        );
    }

    /// The expected shares are a probability distribution; if they ever stop
    /// summing to one the MAD is measured against nothing.
    #[test]
    fn the_expected_shares_sum_to_one() {
        let total: f64 = (1..=9).map(expected_share).sum();
        assert!((total - 1.0).abs() < 1e-12, "{total}");
        // And the shape everyone quotes: a leading 1 about 30% of the time.
        assert!((expected_share(1) - 0.301).abs() < 0.001);
        assert!(expected_share(9) < 0.05);
    }
}
