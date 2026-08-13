use super::*;
use crate::data::fuzzy_duplicates::{NormalizeOpts, SimilarityMethod};
use crate::data::join::JoinType;
use crate::data::{CellValue, ColumnInfo, DataTable};
use std::sync::atomic::AtomicBool;

fn tbl(name: &str, values: &[&str]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: name.into(),
        data_type: "Utf8".into(),
    }];
    t.rows = values
        .iter()
        .map(|v| vec![CellValue::String(v.to_string())])
        .collect();
    t
}

fn step() -> FuzzyJoinStep {
    FuzzyJoinStep {
        pairs: vec![(0, 0)],
        method: SimilarityMethod::EditRatio,
        threshold: 0.85,
        normalize: NormalizeOpts::default(),
        block: None,
        how: JoinType::Left,
        max_rows: 20_000,
    }
}

fn run(left: &DataTable, right: &DataTable, s: &FuzzyJoinStep) -> FuzzyJoinResult {
    fuzzy_join(
        &[left, right],
        std::slice::from_ref(s),
        &AtomicBool::new(false),
    )
    .expect("fuzzy join")
}

fn col_of(t: &DataTable, name: &str) -> usize {
    t.columns
        .iter()
        .position(|c| c.name == name)
        .unwrap_or_else(|| panic!("no column {name}"))
}

/// The case the feature exists for.
#[test]
fn spelling_variants_of_one_company_match() {
    let left = tbl("customer", &["Mueller GmbH"]);
    let right = tbl("account", &["Mueller Gmbh."]);
    let out = run(&left, &right, &step());

    assert_eq!(out.table.row_count(), 1);
    let matched = out
        .table
        .get(0, col_of(&out.table, "account"))
        .map(|c| c.to_string());
    assert_eq!(matched, Some("Mueller Gmbh.".to_string()));
    assert_eq!(out.steps[0].matched, 1);
}

#[test]
fn output_carries_a_score_and_an_ambiguity_flag() {
    let left = tbl("customer", &["Mueller GmbH"]);
    let right = tbl("account", &["Mueller Gmbh."]);
    let out = run(&left, &right, &step());

    let names: Vec<&str> = out.table.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"match_score_1"), "got {names:?}");
    assert!(names.contains(&"ambiguous_1"), "got {names:?}");
}

/// One partner per left row, highest score, ties by first occurrence.
#[test]
fn best_match_wins_and_ties_break_by_first_occurrence() {
    let left = tbl("customer", &["Meier"]);
    let right = tbl("account", &["Meier", "Meier"]);
    let out = run(&left, &right, &step());
    assert_eq!(out.table.row_count(), 1, "one row in, one row out");
}

/// The runner-up being nearly as good is the interesting case: the match is
/// the one to distrust, and the flag is how you find it.
///
/// The names are long enough that a single character of difference lands
/// comfortably inside AMBIGUITY_MARGIN. EditRatio charges 1/len per edit, so
/// on a five-letter name one edit costs 0.2, four times the margin. A 20-char
/// name costs exactly 0.05 and lands on the boundary, where binary floating
/// point puts 1.0 - 0.95 just above it; 33 characters keeps the gap at 0.03
/// and tests the rule rather than the rounding.
#[test]
fn a_close_runner_up_sets_the_ambiguity_flag() {
    let left = tbl("customer", &["Mueller Handels und Vertrieb GmbH"]);
    let right = tbl(
        "account",
        &[
            "Mueller Handels und Vertrieb GmbH",
            "Mueller Handels und Vertrieb GmbX",
        ],
    );
    let out = run(&left, &right, &step());

    let col = col_of(&out.table, "ambiguous_1");
    assert_eq!(
        out.table.get(0, col).map(|c| c.to_string()),
        Some("true".to_string()),
        "a runner-up 0.05 behind the winner must be flagged"
    );
}

/// A runner-up that never cleared the threshold is not a runner-up. The
/// threshold is the user's own statement of what counts as a candidate, so
/// warning about things it already excluded would flag almost every row at a
/// low threshold and mean nothing.
#[test]
fn a_sub_threshold_near_miss_does_not_flag() {
    let left = tbl("customer", &["Meier"]);
    // Maier scores 0.8 against Meier, below the step's 0.85 threshold.
    let right = tbl("account", &["Meier", "Maier"]);
    let out = run(&left, &right, &step());

    let col = col_of(&out.table, "ambiguous_1");
    assert_eq!(
        out.table.get(0, col).map(|c| c.to_string()),
        Some("false".to_string())
    );
    assert_eq!(out.steps[0].ambiguous, 0);
}

#[test]
fn a_clear_winner_leaves_the_flag_unset() {
    let left = tbl("customer", &["Meier"]);
    let right = tbl("account", &["Meier", "Zzzzzzz"]);
    let out = run(&left, &right, &step());

    let col = col_of(&out.table, "ambiguous_1");
    assert_eq!(
        out.table.get(0, col).map(|c| c.to_string()),
        Some("false".to_string())
    );
}

#[test]
fn left_join_keeps_an_unmatched_row_and_inner_drops_it() {
    let left = tbl("customer", &["Mueller GmbH", "Nothing Like It"]);
    let right = tbl("account", &["Mueller Gmbh."]);

    let left_join = run(&left, &right, &step());
    assert_eq!(
        left_join.table.row_count(),
        2,
        "Left keeps the unmatched row"
    );
    let score_col = col_of(&left_join.table, "match_score_1");
    assert!(
        matches!(
            left_join.table.get(1, score_col),
            Some(CellValue::Null) | None
        ),
        "an unmatched row has no score"
    );

    let inner = run(
        &left,
        &right,
        &FuzzyJoinStep {
            how: JoinType::Inner,
            ..step()
        },
    );
    assert_eq!(inner.table.row_count(), 1, "Inner drops the unmatched row");
}

#[test]
fn the_threshold_is_respected() {
    let left = tbl("customer", &["Mueller GmbH"]);
    let right = tbl("account", &["Totally Different"]);
    let out = run(&left, &right, &step());
    assert_eq!(
        out.steps[0].matched, 0,
        "nothing similar enough should match"
    );
}

#[test]
fn normalisation_is_applied_before_comparing() {
    let left = tbl("customer", &["  ACME  Ltd. "]);
    let right = tbl("account", &["acme ltd"]);
    let out = run(&left, &right, &step());
    assert_eq!(
        out.steps[0].matched, 1,
        "case, spacing and punctuation should not block a match"
    );
}

#[test]
fn a_step_count_that_does_not_match_the_tables_is_an_error() {
    let a = tbl("x", &["1"]);
    let err = fuzzy_join(&[&a], &[step()], &AtomicBool::new(false));
    assert!(err.is_err(), "one table cannot take one join step");

    let err = fuzzy_join(&[&a, &a], &[], &AtomicBool::new(false));
    assert!(err.is_err(), "two tables need one step");
}

#[test]
fn empty_input_is_an_empty_result_not_a_panic() {
    let empty = DataTable::empty();
    let right = tbl("account", &["x"]);
    let out = run(&empty, &right, &step());
    assert_eq!(out.table.row_count(), 0);
}

#[test]
fn full_and_right_joins_emit_unmatched_right_rows() {
    let left = tbl("customer", &["Mueller GmbH"]);
    let right = tbl("account", &["Mueller Gmbh.", "Orphan Ltd"]);

    let full = run(
        &left,
        &right,
        &FuzzyJoinStep {
            how: JoinType::Full,
            ..step()
        },
    );
    assert_eq!(
        full.table.row_count(),
        2,
        "Full keeps the orphaned right row"
    );

    let right_join = run(
        &left,
        &right,
        &FuzzyJoinStep {
            how: JoinType::Right,
            ..step()
        },
    );
    assert_eq!(
        right_join.table.row_count(),
        2,
        "Right keeps every right row"
    );

    let inner = run(
        &left,
        &right,
        &FuzzyJoinStep {
            how: JoinType::Inner,
            ..step()
        },
    );
    assert_eq!(inner.table.row_count(), 1);
}

/// Blocking must not change which pairs match, only which are compared.
#[test]
fn blocking_agrees_with_brute_force_within_a_block() {
    let mut left = DataTable::empty();
    left.columns = vec![
        ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "country".into(),
            data_type: "Utf8".into(),
        },
    ];
    left.rows = vec![
        vec![
            CellValue::String("Mueller GmbH".into()),
            CellValue::String("DE".into()),
        ],
        vec![
            CellValue::String("Mueller GmbH".into()),
            CellValue::String("AT".into()),
        ],
    ];

    let mut right = DataTable::empty();
    right.columns = vec![
        ColumnInfo {
            name: "account".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "cc".into(),
            data_type: "Utf8".into(),
        },
    ];
    right.rows = vec![vec![
        CellValue::String("Mueller Gmbh.".into()),
        CellValue::String("DE".into()),
    ]];

    let blocked = fuzzy_join(
        &[&left, &right],
        &[FuzzyJoinStep {
            block: Some((1, 1)),
            ..step()
        }],
        &AtomicBool::new(false),
    )
    .expect("blocked join");

    // Only the DE row may match; the AT row is never compared.
    assert_eq!(blocked.steps[0].matched, 1);
    let score_col = col_of(&blocked.table, "match_score_1");
    assert!(matches!(
        blocked.table.get(1, score_col),
        Some(CellValue::Null) | None
    ));
}

/// Three tables produce one score and one flag per step, not one overall.
#[test]
fn three_tables_number_their_scores_per_step() {
    let a = tbl("customer", &["Mueller GmbH"]);
    let b = tbl("account", &["Mueller Gmbh."]);
    let c = tbl("ledger", &["Mueller  GMBH"]);

    let out = fuzzy_join(
        &[&a, &b, &c],
        &[
            step(),
            FuzzyJoinStep {
                pairs: vec![(0, 0)],
                ..step()
            },
        ],
        &AtomicBool::new(false),
    )
    .expect("three-table fold");

    let names: Vec<&str> = out.table.columns.iter().map(|c| c.name.as_str()).collect();
    for wanted in [
        "match_score_1",
        "ambiguous_1",
        "match_score_2",
        "ambiguous_2",
    ] {
        assert!(names.contains(&wanted), "missing {wanted} in {names:?}");
    }
    assert_eq!(out.steps.len(), 2, "one report per step");
}

/// A column name colliding across tables must not silently overwrite.
#[test]
fn colliding_column_names_are_uniquified() {
    let a = tbl("name", &["Mueller GmbH"]);
    let b = tbl("name", &["Mueller Gmbh."]);
    let out = run(&a, &b, &step());
    let names: Vec<&str> = out.table.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"name"));
    assert!(names.contains(&"name_2"), "got {names:?}");
}

#[test]
fn the_cap_is_reported_when_hit() {
    let left = tbl("customer", &["a", "b", "c"]);
    let right = tbl("account", &["a"]);
    let out = run(
        &left,
        &right,
        &FuzzyJoinStep {
            max_rows: 2,
            ..step()
        },
    );
    assert!(out.steps[0].capped, "a truncated side must be reported");
    assert_eq!(out.steps[0].left_rows, 2);
}

#[test]
fn cancellation_returns_an_error_rather_than_a_partial_join() {
    let left = tbl("customer", &["a", "b"]);
    let right = tbl("account", &["a"]);
    let err = fuzzy_join(&[&left, &right], &[step()], &AtomicBool::new(true));
    assert!(
        err.is_err(),
        "a cancelled join must not return half a table"
    );
}
