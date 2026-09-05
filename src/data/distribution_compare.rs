//! Do these two columns come from the same population?
//!
//! The question people actually have is "did something change" - last month
//! against this month, control against treatment, the file the vendor sent in
//! June against the one they sent in July. A mean and a standard deviation
//! answer that badly: two samples can share both and still be shaped nothing
//! alike.
//!
//! Two tests, picked by what the columns hold:
//!
//! - **Numbers**: the two-sample Kolmogorov-Smirnov test, which compares the
//!   whole shape rather than one summary of it.
//! - **Categories**: a chi-square test of homogeneity over the shared set of
//!   values.
//!
//! **The statistic is not the answer, it is the evidence.** `D = 0.14,
//! p = 0.02` tells almost nobody anything, so every comparison also produces a
//! [`Headline`]: the same population, or the second one skewed 12% higher, or
//! the category that moved. The number stays, underneath.

use std::collections::BTreeMap;

use crate::data::CellValue;

/// Below this many usable values in either sample there is nothing to test.
pub const MIN_SAMPLE: usize = 20;

/// More distinct categories than this and a chi-square is measuring noise:
/// almost every cell of the table would be near-empty.
pub const MAX_CATEGORIES: usize = 50;

/// Chi-square wants an expected count of at least this in every cell. Rarer
/// categories are pooled into one `(other)` bucket rather than dropped, so
/// their rows still count towards the totals.
pub const MIN_EXPECTED: f64 = 5.0;

/// The conventional threshold. Above it the samples are consistent with one
/// population; below it they are not.
pub const ALPHA: f64 = 0.05;

/// Which test ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestKind {
    /// Two-sample Kolmogorov-Smirnov, over numbers.
    Kolmogorov,
    /// Chi-square test of homogeneity, over categories.
    ChiSquare,
}

impl TestKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Kolmogorov => "kolmogorov_smirnov",
            Self::ChiSquare => "chi_square",
        }
    }
}

/// The plain-language answer, in front of the statistic.
///
/// A variant rather than a string so the GUI can localize it and the headless
/// surfaces can print [`Headline::sentence`] without a second code path.
#[derive(Debug, Clone, PartialEq)]
pub enum Headline {
    /// Consistent with one population.
    Same,
    /// Different, and the second sample's median sits `percent` away from the
    /// first's. Positive is higher.
    Shifted { percent: f64 },
    /// Different, and this category moved the most. Shares are 0.0 to 1.0.
    CategoryMoved { name: String, from: f64, to: f64 },
    /// Different, but no single number tells the story.
    Different,
}

impl Headline {
    /// English, for the CLI and the MCP server. The GUI renders the same
    /// variants through `i18n`.
    pub fn sentence(&self) -> String {
        match self {
            Self::Same => "These two look like the same population.".to_string(),
            Self::Shifted { percent } if *percent >= 0.0 => {
                format!("The second sample skews {percent:.0}% higher.")
            }
            Self::Shifted { percent } => {
                format!("The second sample skews {}% lower.", percent.abs().round())
            }
            Self::CategoryMoved { name, from, to } => format!(
                "The share of '{name}' moved from {:.0}% to {:.0}%.",
                from * 100.0,
                to * 100.0
            ),
            Self::Different => "These two are not the same population.".to_string(),
        }
    }

    /// Machine id, for the i18n key and for a structured response.
    pub fn id(&self) -> &'static str {
        match self {
            Self::Same => "same",
            Self::Shifted { percent } if *percent >= 0.0 => "higher",
            Self::Shifted { .. } => "lower",
            Self::CategoryMoved { .. } => "category_moved",
            Self::Different => "different",
        }
    }
}

/// What the comparison found.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The test could not run. The payload is a machine id.
    Skipped(&'static str),
    Compared(Comparison),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub kind: TestKind,
    /// `D` for Kolmogorov-Smirnov, chi-square for the other.
    pub statistic: f64,
    pub p_value: f64,
    pub degrees_of_freedom: Option<usize>,
    pub n_a: usize,
    pub n_b: usize,
    /// True when the samples are consistent with one population.
    pub same: bool,
    pub headline: Headline,
}

/// Compare two columns' values.
///
/// The test is chosen by the data, not by the caller: if both samples parse as
/// numbers throughout, the shapes are compared; otherwise the values are
/// treated as categories. Mixed columns therefore compare as categories, which
/// is the honest reading of a column that is not really numeric.
pub fn compare(a: &[&CellValue], b: &[&CellValue]) -> Outcome {
    let (va, vb) = (usable(a), usable(b));
    if va.len() < MIN_SAMPLE || vb.len() < MIN_SAMPLE {
        return Outcome::Skipped("na_too_few_values");
    }
    match (numbers(&va), numbers(&vb)) {
        (Some(na), Some(nb)) => Outcome::Compared(compare_numeric(&na, &nb)),
        _ => compare_categorical(&va, &vb),
    }
}

/// Non-null values as their display text. Nulls are absence, not a category:
/// counting them would make "this column got emptier" read as a distribution
/// change, which is what `null_percentage` is for.
fn usable(cells: &[&CellValue]) -> Vec<String> {
    cells
        .iter()
        .filter(|c| !matches!(c, CellValue::Null))
        .map(|c| c.to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn numbers(values: &[String]) -> Option<Vec<f64>> {
    values
        .iter()
        .map(|s| s.parse::<f64>().ok().filter(|f| f.is_finite()))
        .collect()
}

fn compare_numeric(a: &[f64], b: &[f64]) -> Comparison {
    let (d, p) = ks_two_sample(a, b);
    let same = p >= ALPHA;
    let (ma, mb) = (median(a), median(b));
    let headline = if same {
        Headline::Same
    } else if ma.abs() > f64::EPSILON {
        Headline::Shifted {
            percent: (mb - ma) / ma.abs() * 100.0,
        }
    } else {
        Headline::Different
    };
    Comparison {
        kind: TestKind::Kolmogorov,
        statistic: d,
        p_value: p,
        degrees_of_freedom: None,
        n_a: a.len(),
        n_b: b.len(),
        same,
        headline,
    }
}

fn compare_categorical(a: &[String], b: &[String]) -> Outcome {
    let mut counts: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    for v in a {
        counts.entry(v.as_str()).or_insert((0.0, 0.0)).0 += 1.0;
    }
    for v in b {
        counts.entry(v.as_str()).or_insert((0.0, 0.0)).1 += 1.0;
    }
    if counts.len() > MAX_CATEGORIES {
        return Outcome::Skipped("na_too_many_categories");
    }
    let (total_a, total_b) = (a.len() as f64, b.len() as f64);
    let total = total_a + total_b;

    // Pool the categories too rare for a chi-square cell into one bucket
    // rather than dropping them: their rows still belong to the totals.
    let mut cells: Vec<(String, f64, f64)> = Vec::new();
    let mut pooled = (0.0, 0.0);
    for (name, (ca, cb)) in &counts {
        let row_total = ca + cb;
        let expected_min = (row_total * total_a / total).min(row_total * total_b / total);
        if expected_min < MIN_EXPECTED {
            pooled.0 += ca;
            pooled.1 += cb;
        } else {
            cells.push(((*name).to_string(), *ca, *cb));
        }
    }
    if pooled.0 + pooled.1 > 0.0 {
        cells.push(("(other)".to_string(), pooled.0, pooled.1));
    }
    if cells.len() < 2 {
        return Outcome::Skipped("na_too_few_categories");
    }

    let mut chi2 = 0.0;
    for (_, ca, cb) in &cells {
        let row_total = ca + cb;
        for (observed, column_total) in [(*ca, total_a), (*cb, total_b)] {
            let expected = row_total * column_total / total;
            if expected > 0.0 {
                chi2 += (observed - expected).powi(2) / expected;
            }
        }
    }
    let dof = cells.len() - 1;
    let p = chi_square_p(chi2, dof);
    let same = p >= ALPHA;

    // The category whose share moved most, which is the one a reader wants
    // named. Pooled leftovers are not a category anyone can act on.
    let headline = if same {
        Headline::Same
    } else {
        cells
            .iter()
            .filter(|(name, _, _)| name != "(other)")
            .map(|(name, ca, cb)| {
                let (fa, fb) = (ca / total_a, cb / total_b);
                (name.clone(), fa, fb, (fb - fa).abs())
            })
            .max_by(|x, y| x.3.total_cmp(&y.3))
            .map(|(name, from, to, _)| Headline::CategoryMoved { name, from, to })
            .unwrap_or(Headline::Different)
    };

    Outcome::Compared(Comparison {
        kind: TestKind::ChiSquare,
        statistic: chi2,
        p_value: p,
        degrees_of_freedom: Some(dof),
        n_a: a.len(),
        n_b: b.len(),
        same,
        headline,
    })
}

/// Compare column `ca` of one table with column `cb` of another. The two may
/// be the same table; nothing here cares.
pub fn compare_columns(
    ta: &crate::data::DataTable,
    ca: usize,
    tb: &crate::data::DataTable,
    cb: usize,
) -> Outcome {
    let left: Vec<&CellValue> = (0..ta.row_count()).filter_map(|r| ta.get(r, ca)).collect();
    let right: Vec<&CellValue> = (0..tb.row_count()).filter_map(|r| tb.get(r, cb)).collect();
    compare(&left, &right)
}

/// The result as a one-row-per-fact table, so the GUI tab, the CLI and the MCP
/// server all render the same answer rather than three formattings of it.
///
/// Vertical rather than one wide row: the headline is a sentence and the
/// statistic is a number, and a table two columns wide reads at any width.
pub fn result_table(a_label: &str, b_label: &str, outcome: &Outcome) -> crate::data::DataTable {
    use crate::data::{ColumnInfo, DataTable};
    let mut t = DataTable::empty();
    t.columns = ["field", "value"]
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    let mut push = |k: &str, v: String| {
        t.rows
            .push(vec![CellValue::String(k.to_string()), CellValue::String(v)]);
    };
    push("first", a_label.to_string());
    push("second", b_label.to_string());
    match outcome {
        Outcome::Skipped(reason) => {
            push("result", (*reason).to_string());
        }
        Outcome::Compared(c) => {
            push("headline", c.headline.sentence());
            push(
                "verdict",
                if c.same { "same" } else { "different" }.to_string(),
            );
            push("test", c.kind.id().to_string());
            push("statistic", format!("{:.4}", c.statistic));
            push("p_value", format!("{:.4}", c.p_value));
            if let Some(dof) = c.degrees_of_freedom {
                push("degrees_of_freedom", dof.to_string());
            }
            push("first_rows", c.n_a.to_string());
            push("second_rows", c.n_b.to_string());
        }
    }
    t
}

// ---------------------------------------------------------------------------
// The statistics
// ---------------------------------------------------------------------------

/// Two-sample Kolmogorov-Smirnov: the largest vertical distance between the
/// two empirical distributions, and the probability of seeing one that large
/// if both came from the same population.
pub fn ks_two_sample(a: &[f64], b: &[f64]) -> (f64, f64) {
    let mut sa: Vec<f64> = a.to_vec();
    let mut sb: Vec<f64> = b.to_vec();
    sa.sort_by(f64::total_cmp);
    sb.sort_by(f64::total_cmp);
    let (na, nb) = (sa.len(), sb.len());
    if na == 0 || nb == 0 {
        return (0.0, 1.0);
    }

    // Walk both sorted samples together, stepping past every tie before
    // measuring: taking the distance mid-tie would report a gap that the
    // distributions do not have.
    let (mut i, mut j) = (0usize, 0usize);
    let mut d: f64 = 0.0;
    while i < na && j < nb {
        let x = sa[i].min(sb[j]);
        while i < na && sa[i] <= x {
            i += 1;
        }
        while j < nb && sb[j] <= x {
            j += 1;
        }
        let diff = (i as f64 / na as f64) - (j as f64 / nb as f64);
        d = d.max(diff.abs());
    }

    let ne = (na as f64 * nb as f64) / (na as f64 + nb as f64);
    (d, kolmogorov_p(ne.sqrt(), d))
}

/// The asymptotic Kolmogorov distribution, with the small-sample correction
/// that makes it usable well below the limit.
fn kolmogorov_p(sqrt_ne: f64, d: f64) -> f64 {
    let lambda = (sqrt_ne + 0.12 + 0.11 / sqrt_ne) * d;
    if lambda <= 0.0 {
        return 1.0;
    }
    let mut sum = 0.0;
    for k in 1..=100 {
        let term = (-2.0 * (k * k) as f64 * lambda * lambda).exp();
        sum += if k % 2 == 1 { term } else { -term };
        if term < 1e-12 {
            break;
        }
    }
    (2.0 * sum).clamp(0.0, 1.0)
}

/// Upper tail of the chi-square distribution: `P(X > chi2)` with `dof` degrees
/// of freedom, which is the regularized upper incomplete gamma `Q(dof/2, chi2/2)`.
pub fn chi_square_p(chi2: f64, dof: usize) -> f64 {
    if dof == 0 || !chi2.is_finite() || chi2 <= 0.0 {
        return 1.0;
    }
    gamma_q(dof as f64 / 2.0, chi2 / 2.0)
}

/// Regularized upper incomplete gamma. Series below the crossover, continued
/// fraction above it, which is where each converges quickly.
fn gamma_q(s: f64, x: f64) -> f64 {
    if x < s + 1.0 {
        1.0 - gamma_p_series(s, x)
    } else {
        gamma_q_fraction(s, x)
    }
}

fn gamma_p_series(s: f64, x: f64) -> f64 {
    let mut term = 1.0 / s;
    let mut sum = term;
    let mut n = s;
    for _ in 0..1000 {
        n += 1.0;
        term *= x / n;
        sum += term;
        if term.abs() < sum.abs() * 1e-15 {
            break;
        }
    }
    (sum * (-x + s * x.ln() - ln_gamma(s)).exp()).clamp(0.0, 1.0)
}

fn gamma_q_fraction(s: f64, x: f64) -> f64 {
    const TINY: f64 = 1e-300;
    let mut b = x + 1.0 - s;
    let mut c = 1.0 / TINY;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..1000 {
        let an = -(i as f64) * (i as f64 - s);
        b += 2.0;
        d = an * d + b;
        if d.abs() < TINY {
            d = TINY;
        }
        c = b + an / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < 1e-15 {
            break;
        }
    }
    ((-x + s * x.ln() - ln_gamma(s)).exp() * h).clamp(0.0, 1.0)
}

/// Lanczos approximation of `ln(gamma(x))` for `x > 0`.
fn ln_gamma(x: f64) -> f64 {
    const COEF: [f64; 6] = [
        76.180_091_729_471_46,
        -86.505_320_329_416_77,
        24.014_098_240_830_91,
        -1.231_739_572_450_155,
        0.001_208_650_973_866_179,
        -0.000_005_395_239_384_953,
    ];
    let mut y = x;
    let tmp = x + 5.5 - (x + 0.5) * (x + 5.5).ln();
    let mut ser = 1.000_000_000_190_015;
    for c in COEF {
        y += 1.0;
        ser += c / y;
    }
    -tmp + (2.506_628_274_631_000_5 * ser / x).ln()
}

fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

#[cfg(test)]
mod tests {
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
}
