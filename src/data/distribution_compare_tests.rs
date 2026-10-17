use super::*;

fn cells(values: &[f64]) -> Vec<CellValue> {
    values.iter().map(|v| CellValue::Float(*v)).collect()
}

fn run(a: &[CellValue], b: &[CellValue]) -> Outcome {
    let ra: Vec<&CellValue> = a.iter().collect();
    let rb: Vec<&CellValue> = b.iter().collect();
    compare(&ra, &rb)
}

/// A deterministic spread, so the tests do not depend on a random seed.
fn ramp(n: usize, offset: f64, scale: f64) -> Vec<f64> {
    (0..n)
        .map(|i| offset + scale * (i as f64 / n as f64))
        .collect()
}

#[test]
fn the_same_distribution_reads_as_the_same_population() {
    let a = cells(&ramp(200, 0.0, 100.0));
    let b = cells(&ramp(200, 0.0, 100.0));
    match run(&a, &b) {
        Outcome::Compared(c) => {
            assert_eq!(c.kind, TestKind::Kolmogorov);
            assert!(c.same, "p was {}", c.p_value);
            assert_eq!(c.headline, Headline::Same);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_shifted_distribution_is_reported_with_its_direction() {
    let a = cells(&ramp(200, 100.0, 20.0));
    let b = cells(&ramp(200, 150.0, 20.0));
    match run(&a, &b) {
        Outcome::Compared(c) => {
            assert!(!c.same, "p was {}", c.p_value);
            match c.headline {
                Headline::Shifted { percent } => {
                    assert!(percent > 30.0 && percent < 60.0, "{percent}");
                    assert_eq!(c.headline.id(), "higher");
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_lower_sample_says_lower() {
    let a = cells(&ramp(200, 150.0, 20.0));
    let b = cells(&ramp(200, 100.0, 20.0));
    match run(&a, &b) {
        Outcome::Compared(c) => {
            assert_eq!(c.headline.id(), "lower");
            assert!(c.headline.sentence().contains("lower"));
        }
        other => panic!("{other:?}"),
    }
}

fn categories(pairs: &[(&str, usize)]) -> Vec<CellValue> {
    let mut out = Vec::new();
    for (name, n) in pairs {
        for _ in 0..*n {
            out.push(CellValue::String((*name).to_string()));
        }
    }
    out
}

#[test]
fn identical_category_mixes_read_as_the_same_population() {
    let a = categories(&[("red", 100), ("green", 60), ("blue", 40)]);
    let b = categories(&[("red", 100), ("green", 60), ("blue", 40)]);
    match run(&a, &b) {
        Outcome::Compared(c) => {
            assert_eq!(c.kind, TestKind::ChiSquare);
            assert!(c.same, "p was {}", c.p_value);
            assert_eq!(c.degrees_of_freedom, Some(2));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_moved_category_is_named() {
    let a = categories(&[("red", 150), ("green", 30), ("blue", 20)]);
    let b = categories(&[("red", 40), ("green", 80), ("blue", 80)]);
    match run(&a, &b) {
        Outcome::Compared(c) => {
            assert!(!c.same, "p was {}", c.p_value);
            match &c.headline {
                Headline::CategoryMoved { name, from, to } => {
                    assert_eq!(name, "red");
                    assert!(*from > *to, "red shrank");
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

/// A rare category would give chi-square a near-empty cell, which inflates
/// the statistic. Pooling keeps its rows in the totals.
#[test]
fn rare_categories_are_pooled_not_dropped() {
    let mut a = categories(&[("red", 100), ("green", 100)]);
    let mut b = categories(&[("red", 100), ("green", 100)]);
    a.push(CellValue::String("unicorn".into()));
    b.push(CellValue::String("unicorn".into()));
    match run(&a, &b) {
        Outcome::Compared(c) => {
            assert_eq!(c.n_a, 201, "the rare row still counts");
            assert!(c.same);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_short_sample_is_not_tested() {
    let a = cells(&ramp(10, 0.0, 1.0));
    let b = cells(&ramp(200, 0.0, 1.0));
    assert_eq!(run(&a, &b), Outcome::Skipped("na_too_few_values"));
}

#[test]
fn free_text_has_too_many_categories_to_test() {
    let a: Vec<CellValue> = (0..100)
        .map(|i| CellValue::String(format!("note {i}")))
        .collect();
    let b = a.clone();
    assert_eq!(run(&a, &b), Outcome::Skipped("na_too_many_categories"));
}

/// The chi-square tail against the critical values every table prints.
#[test]
fn the_chi_square_tail_matches_published_critical_values() {
    assert!((chi_square_p(3.841, 1) - 0.05).abs() < 0.001);
    assert!((chi_square_p(5.991, 2) - 0.05).abs() < 0.001);
    assert!((chi_square_p(11.070, 5) - 0.05).abs() < 0.001);
    assert!((chi_square_p(6.635, 1) - 0.01).abs() < 0.001);
    assert_eq!(chi_square_p(0.0, 3), 1.0);
}

/// Two samples drawn from the same ramp must not look different however
/// the ties fall, which is what the tie-stepping in `ks_two_sample` is for.
#[test]
fn ties_do_not_invent_a_difference() {
    let a: Vec<f64> = std::iter::repeat_n(1.0, 50)
        .chain(std::iter::repeat_n(2.0, 50))
        .collect();
    let b = a.clone();
    let (d, p) = ks_two_sample(&a, &b);
    assert_eq!(d, 0.0);
    assert_eq!(p, 1.0);
}
