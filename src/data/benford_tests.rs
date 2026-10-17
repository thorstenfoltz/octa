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
