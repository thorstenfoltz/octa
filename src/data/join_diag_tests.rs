//! Unit tests for [`join_diag`](join_diag). Split out and included via
//! `#[path]`, matching `schema_drift` and `mojibake`.

use super::*;
use crate::data::ColumnInfo;

fn one_col(name: &str, vals: &[&str]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: name.to_string(),
        data_type: "Utf8".to_string(),
    }];
    t.rows = vals
        .iter()
        .map(|v| vec![CellValue::String((*v).to_string())])
        .collect();
    t
}

fn fix(d: &JoinDiagnosis, kind: FixKind) -> Option<&SuggestedFix> {
    d.fixes.iter().find(|f| f.kind == kind)
}

#[test]
fn trailing_whitespace_is_named_as_the_culprit() {
    let l = one_col("id", &["a ", "b ", "c "]);
    let r = one_col("id", &["a", "b", "c"]);
    let d = diagnose(&l, 0, &r, 0, 10_000);
    assert_eq!(d.matched_left, 0, "nothing matches before trimming");
    let f = fix(&d, FixKind::TrimWhitespace).expect("expected a TrimWhitespace fix");
    assert_eq!(f.would_match, 3);
}

#[test]
fn case_mismatch_is_named() {
    let l = one_col("id", &["ABC", "DEF"]);
    let r = one_col("id", &["abc", "def"]);
    let d = diagnose(&l, 0, &r, 0, 10_000);
    assert_eq!(d.matched_left, 0);
    assert_eq!(fix(&d, FixKind::IgnoreCase).map(|f| f.would_match), Some(2));
}

#[test]
fn leading_zeros_are_named() {
    let l = one_col("id", &["007", "042"]);
    let r = one_col("id", &["7", "42"]);
    let d = diagnose(&l, 0, &r, 0, 10_000);
    assert_eq!(
        fix(&d, FixKind::StripLeadingZeros).map(|f| f.would_match),
        Some(2)
    );
}

#[test]
fn genuinely_unrelated_columns_suggest_nothing() {
    let l = one_col("id", &["alpha", "beta"]);
    let r = one_col("id", &["gamma", "delta"]);
    let d = diagnose(&l, 0, &r, 0, 10_000);
    assert_eq!(d.matched_left, 0);
    assert!(d.fixes.is_empty(), "got {:?}", d.fixes);
}

#[test]
fn an_already_working_join_suggests_nothing() {
    let l = one_col("id", &["a", "b"]);
    let r = one_col("id", &["a", "b"]);
    let d = diagnose(&l, 0, &r, 0, 10_000);
    assert_eq!(d.matched_left, 2);
    assert!(
        d.fixes.is_empty(),
        "a fix must strictly improve on the status quo, got {:?}",
        d.fixes
    );
}

#[test]
fn reports_counts_and_unmatched_samples() {
    let l = one_col("id", &["a", "b", "zz"]);
    let r = one_col("id", &["a", "b", "yy"]);
    let d = diagnose(&l, 0, &r, 0, 10_000);
    assert_eq!(d.left_rows, 3);
    assert_eq!(d.right_rows, 3);
    assert_eq!(d.distinct_left, 3);
    assert_eq!(d.matched_left, 2);
    assert_eq!(d.unmatched_left, vec!["zz".to_string()]);
    assert_eq!(d.unmatched_right, vec!["yy".to_string()]);
    assert!(!d.capped);
}

#[test]
fn unmatched_samples_are_capped_and_deterministic() {
    let left: Vec<String> = (0..20).map(|i| format!("l{i:02}")).collect();
    let l = one_col("id", &left.iter().map(String::as_str).collect::<Vec<_>>());
    let r = one_col("id", &["nothing"]);
    let a = diagnose(&l, 0, &r, 0, 10_000);
    let b = diagnose(&l, 0, &r, 0, 10_000);
    assert_eq!(a.unmatched_left.len(), MAX_SAMPLES);
    assert_eq!(a.unmatched_left, b.unmatched_left, "same input, same order");
}

#[test]
fn sampling_is_reported() {
    let left: Vec<String> = (0..10).map(|i| i.to_string()).collect();
    let l = one_col("id", &left.iter().map(String::as_str).collect::<Vec<_>>());
    let r = one_col("id", &["0"]);
    let d = diagnose(&l, 0, &r, 0, 5);
    assert!(d.capped, "left has 10 rows but the sample was 5");
}

#[test]
fn empty_values_are_not_treated_as_keys() {
    // A blank on both sides is not evidence that the join works.
    let l = one_col("id", &["", "  ", "a"]);
    let r = one_col("id", &["", "b"]);
    let d = diagnose(&l, 0, &r, 0, 10_000);
    assert_eq!(d.distinct_left, 1, "only `a` counts");
    assert_eq!(d.matched_left, 0);
}
