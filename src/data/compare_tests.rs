//! Unit tests for [`compare`](compare). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

fn table(headers: &[&str], rows: &[&[&str]]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = headers
        .iter()
        .map(|h| ColumnInfo {
            name: (*h).to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    t.rows = rows
        .iter()
        .map(|r| r.iter().map(|s| CellValue::String(s.to_string())).collect())
        .collect();
    t
}

#[test]
fn ordered_reports_changed_cells_and_trailing_rows() {
    let a = table(&["id", "v"], &[&["1", "x"], &["2", "y"], &["3", "z"]]);
    let b = table(&["id", "v"], &[&["1", "x"], &["2", "Y"]]);
    let r = compare_ordered(&a, &b, &CompareOptions::default());
    assert_eq!(r.unchanged, 1); // row 0 identical
    assert_eq!(r.changed.len(), 1);
    assert_eq!(r.changed[0].row_a, 1);
    assert_eq!(r.changed[0].changed_columns, vec!["v".to_string()]);
    assert_eq!(r.only_in_a, vec![2]); // trailing "3,z"
    assert!(r.only_in_b.is_empty());
}

#[test]
fn join_matches_added_removed_changed() {
    let a = table(&["id", "v"], &[&["1", "x"], &["2", "y"], &["3", "z"]]);
    let b = table(&["id", "v"], &[&["2", "Y"], &["3", "z"], &["4", "w"]]);
    let r = compare_join(&a, &b, &["id".to_string()], &CompareOptions::default()).unwrap();
    assert_eq!(r.only_in_a, vec![0]); // id 1
    assert_eq!(r.only_in_b, vec![2]); // id 4
    assert_eq!(r.changed.len(), 1); // id 2: v changed
    assert_eq!(r.changed[0].changed_columns, vec!["v".to_string()]);
    assert_eq!(r.unchanged, 1); // id 3 identical
}

#[test]
fn join_handles_columns_in_different_order() {
    let a = table(&["id", "v"], &[&["1", "x"]]);
    let b = table(&["v", "id"], &[&["x", "1"]]);
    let r = compare_join(&a, &b, &["id".to_string()], &CompareOptions::default()).unwrap();
    assert_eq!(r.unchanged, 1);
    assert!(r.changed.is_empty());
}

#[test]
fn join_duplicate_keys_pair_in_order() {
    let a = table(&["id", "v"], &[&["1", "a"], &["1", "b"]]);
    let b = table(&["id", "v"], &[&["1", "a"], &["1", "c"]]);
    let r = compare_join(&a, &b, &["id".to_string()], &CompareOptions::default()).unwrap();
    assert_eq!(r.unchanged, 1); // first 1,a pair
    assert_eq!(r.changed.len(), 1); // second 1,b vs 1,c
}

#[test]
fn join_missing_key_column_errors() {
    let a = table(&["id"], &[&["1"]]);
    let b = table(&["other"], &[&["1"]]);
    assert!(compare_join(&a, &b, &["id".to_string()], &CompareOptions::default()).is_err());
}

#[test]
fn build_table_emits_paired_changed_rows() {
    let a = table(&["id", "v"], &[&["1", "x"]]);
    let b = table(&["id", "v"], &[&["1", "X"]]);
    let r = compare_ordered(&a, &b, &CompareOptions::default());
    let out = build_compare_table(&a, &b, &r);
    assert_eq!(out.col_count(), 4); // status, changed_columns, id, v
    assert_eq!(out.row_count(), 2); // changed_a + changed_b
    assert_eq!(out.get(0, 0).unwrap().to_string(), "changed_a");
    assert_eq!(out.get(1, 0).unwrap().to_string(), "changed_b");
    assert_eq!(out.get(0, 1).unwrap().to_string(), "v");
}

fn opts() -> CompareOptions {
    CompareOptions::default()
}

#[test]
fn numbers_within_the_absolute_tolerance_pass_and_beyond_it_fail() {
    let a = table(&["v"], &[&["1.0"], &["1.0"]]);
    let b = table(&["v"], &[&["1.0099"], &["1.0101"]]);
    let o = CompareOptions {
        number_abs: Some(0.01),
        ..opts()
    };
    let r = compare_ordered(&a, &b, &o);
    assert_eq!(r.within_tolerance, 1);
    assert_eq!(r.changed.len(), 1);
    assert_eq!(r.changed[0].row_a, 1);
}

#[test]
fn percent_is_of_the_larger_value_and_nothing_divides_by_zero() {
    let a = table(&["v"], &[&["100"], &["0"], &["0"]]);
    let b = table(&["v"], &[&["104"], &["0.001"], &["0"]]);
    let o = CompareOptions {
        number_pct: Some(5.0),
        ..opts()
    };
    let r = compare_ordered(&a, &b, &o);
    // 100 vs 104: 4 <= 5% of 104. 0 vs 0.001: 0.001 > 5% of 0.001. 0 vs 0: same.
    assert_eq!(r.within_tolerance, 1);
    assert_eq!(r.changed.len(), 1);
    assert_eq!(r.changed[0].row_a, 1);
    assert_eq!(r.unchanged, 2);
}

#[test]
fn either_number_tolerance_is_enough() {
    let a = table(&["v"], &[&["1000"]]);
    let b = table(&["v"], &[&["1009"]]);
    let o = CompareOptions {
        number_abs: Some(1.0),
        number_pct: Some(1.0),
        ..opts()
    };
    assert!(compare_ordered(&a, &b, &o).changed.is_empty());
}

#[test]
fn big_integers_and_infinity_are_not_hidden_by_a_tolerance() {
    let o = CompareOptions {
        number_abs: Some(0.01),
        number_pct: Some(1.0),
        ..opts()
    };
    let gap = |a: CellValue, b: CellValue| cells_equal(Some(&a), Some(&b), &o);
    // 2^53 + 1 and 2^53 are one f64 apart from nothing; typed or as text.
    let (big, next) = (9_007_199_254_740_993_i64, 9_007_199_254_740_992_i64);
    assert_eq!(
        gap(CellValue::Int(big), CellValue::Int(next)),
        Equality::WithinTolerance,
        "1 in 9e15 is inside 1 percent"
    );
    let abs_only = CompareOptions {
        number_abs: Some(0.01),
        ..opts()
    };
    for (a, b) in [
        (CellValue::Int(big), CellValue::Int(next)),
        (CellValue::String(big.to_string()), CellValue::Int(next)),
        (CellValue::Int(i64::MIN), CellValue::Int(i64::MAX)),
    ] {
        assert_eq!(
            cells_equal(Some(&a), Some(&b), &abs_only),
            Equality::Different
        );
    }
    assert_eq!(
        gap(CellValue::Float(f64::INFINITY), CellValue::Float(5.0)),
        Equality::Different
    );
    assert_eq!(
        gap(
            CellValue::String("inf".into()),
            CellValue::String("-inf".into())
        ),
        Equality::Different
    );
    assert_eq!(
        gap(CellValue::Int(100), CellValue::Float(100.5)),
        Equality::WithinTolerance
    );
}

#[test]
fn null_is_never_within_tolerance_of_a_number() {
    let mut a = table(&["v"], &[&["0"]]);
    a.rows[0][0] = CellValue::Null;
    let b = table(&["v"], &[&["0"]]);
    let o = CompareOptions {
        number_abs: Some(1.0),
        ..opts()
    };
    assert_eq!(compare_ordered(&a, &b, &o).changed.len(), 1);
}

#[test]
fn times_within_the_seconds_pass_and_a_date_counts_as_midnight() {
    let a = table(
        &["t"],
        &[
            &["2024-01-02 10:00:00"],
            &["2024-01-02"],
            &["2024-01-02 10:00:00"],
        ],
    );
    let b = table(
        &["t"],
        &[
            &["2024-01-02 10:00:30"],
            &["2024-01-02 00:00:20"],
            &["2024-01-02 10:01:01"],
        ],
    );
    let o = CompareOptions {
        seconds: Some(30.0),
        ..opts()
    };
    let r = compare_ordered(&a, &b, &o);
    assert_eq!(r.within_tolerance, 2);
    assert_eq!(r.changed.len(), 1);
    assert_eq!(r.changed[0].row_a, 2);
}

#[test]
fn case_and_spaces_can_be_ignored() {
    let a = table(&["v"], &[&["Mueller  GmbH "]]);
    let b = table(&["v"], &[&["mueller gmbh"]]);
    assert_eq!(compare_ordered(&a, &b, &opts()).changed.len(), 1);
    let o = CompareOptions {
        ignore_case: true,
        ignore_spaces: true,
        ..opts()
    };
    let r = compare_ordered(&a, &b, &o);
    assert!(r.changed.is_empty());
    assert_eq!(r.within_tolerance, 1);
}

#[test]
fn ignored_columns_are_left_out() {
    let a = table(&["id", "updated_at"], &[&["1", "mon"]]);
    let b = table(&["id", "updated_at"], &[&["1", "tue"]]);
    let o = CompareOptions {
        ignore_columns: vec!["updated_at".into()],
        ..opts()
    };
    assert!(compare_ordered(&a, &b, &o).changed.is_empty());
    assert!(
        compare_join(&a, &b, &["id".to_string()], &o)
            .unwrap()
            .changed
            .is_empty()
    );
}

#[test]
fn join_keys_match_across_case_and_spaces_when_asked() {
    let a = table(&["id", "v"], &[&["ABC ", "1"]]);
    let b = table(&["id", "v"], &[&["abc", "1"]]);
    let keys = ["id".to_string()];
    assert_eq!(
        compare_join(&a, &b, &keys, &opts())
            .unwrap()
            .only_in_a
            .len(),
        1
    );
    let o = CompareOptions {
        ignore_case: true,
        ignore_spaces: true,
        ..opts()
    };
    let r = compare_join(&a, &b, &keys, &o).unwrap();
    assert!(r.only_in_a.is_empty() && r.only_in_b.is_empty());
    assert_eq!(r.unchanged, 1);
}

#[test]
fn number_tolerance_never_pairs_keys() {
    let a = table(&["id", "v"], &[&["1", "x"]]);
    let b = table(&["id", "v"], &[&["1.001", "x"]]);
    let o = CompareOptions {
        number_abs: Some(1.0),
        ..opts()
    };
    let r = compare_join(&a, &b, &["id".to_string()], &o).unwrap();
    assert_eq!(r.only_in_a.len(), 1);
}

#[test]
fn ignoring_a_key_column_is_an_error() {
    let a = table(&["id"], &[&["1"]]);
    let o = CompareOptions {
        ignore_columns: vec!["id".into()],
        ..opts()
    };
    assert!(compare_join(&a, &a, &["id".to_string()], &o).is_err());
}

#[test]
fn the_default_stays_exact() {
    let a = table(&["v"], &[&["1.0"]]);
    let b = table(&["v"], &[&["1.001"]]);
    assert!(CompareOptions::default().is_exact());
    assert_eq!(compare_ordered(&a, &b, &opts()).changed.len(), 1);
}

#[test]
fn set_mode_refuses_tolerance_and_negatives_are_refused() {
    let tol = CompareOptions {
        number_abs: Some(0.1),
        ..opts()
    };
    assert!(tol.check(CompareMode::Set).is_err());
    assert!(tol.check(CompareMode::Join).is_ok());
    assert!(opts().check(CompareMode::Set).is_ok());
    let negative = CompareOptions {
        seconds: Some(-1.0),
        ..opts()
    };
    assert!(negative.check(CompareMode::Ordered).is_err());
}

/// The seconds tolerance covers clock times and durations too.
#[test]
fn times_and_durations_within_the_seconds_tolerance() {
    let s = 1_000_000_000i64;
    let opts = CompareOptions {
        seconds: Some(2.0),
        ..Default::default()
    };
    let (ten, ten_01) = (CellValue::Time(36_000 * s), CellValue::Time(36_001 * s));
    assert_eq!(
        cells_equal(Some(&ten), Some(&ten_01), &opts),
        Equality::WithinTolerance
    );
    let (five, eight) = (CellValue::Duration(5 * s), CellValue::Duration(8 * s));
    assert_eq!(
        cells_equal(Some(&five), Some(&eight), &opts),
        Equality::Different
    );
    let text = CellValue::String("10:00:02".into());
    assert_eq!(
        cells_equal(Some(&ten), Some(&text), &opts),
        Equality::WithinTolerance
    );
}
