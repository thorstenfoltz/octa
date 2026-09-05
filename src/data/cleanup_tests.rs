use std::sync::atomic::AtomicBool;

use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn table(cols: &[(&str, &str)], rows: Vec<Vec<CellValue>>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|(n, ty)| ColumnInfo {
            name: (*n).to_string(),
            data_type: (*ty).to_string(),
        })
        .collect();
    t.rows = rows;
    t
}

fn scan(t: &DataTable) -> Vec<Suggestion> {
    suggest_cleanups(t, &CleanupLimits::default(), &AtomicBool::new(false))
}

#[test]
fn flags_trailing_whitespace() {
    let t = table(
        &[("city", "Utf8")],
        vec![
            vec![CellValue::String("Tokyo ".into())],
            vec![CellValue::String("Cologne".into())],
        ],
    );
    let found = scan(&t);
    assert!(
        found
            .iter()
            .any(|s| matches!(s.kind, CleanupKind::TrimWhitespace) && s.column == Some(0)),
        "expected a whitespace suggestion, got {found:?}"
    );
}

#[test]
fn flags_a_text_column_that_is_really_numeric() {
    let t = table(
        &[("amount", "Utf8")],
        (0..20)
            .map(|i| vec![CellValue::String(i.to_string())])
            .collect(),
    );
    let found = scan(&t);
    let hit = found
        .iter()
        .find(|s| matches!(s.kind, CleanupKind::TypeMismatch { .. }))
        .expect("expected a type-mismatch suggestion");
    assert_eq!(hit.column, Some(0));
}

#[test]
fn does_not_flag_a_mostly_textual_column_as_numeric() {
    // Nine words and one number is not a number column. Below the 0.90
    // threshold, so it must stay quiet rather than suggest a lossy cast.
    let mut rows: Vec<Vec<CellValue>> = (0..9)
        .map(|_| vec![CellValue::String("north".into())])
        .collect();
    rows.push(vec![CellValue::String("7".into())]);
    let t = table(&[("region", "Utf8")], rows);
    assert!(
        !scan(&t)
            .iter()
            .any(|s| matches!(s.kind, CleanupKind::TypeMismatch { .. })),
        "a 10% numeric column must not be suggested for casting"
    );
}

#[test]
fn flags_duplicate_rows() {
    let t = table(
        &[("id", "Int64")],
        vec![
            vec![CellValue::Int(1)],
            vec![CellValue::Int(1)],
            vec![CellValue::Int(2)],
        ],
    );
    let found = scan(&t);
    let hit = found
        .iter()
        .find(|s| matches!(s.kind, CleanupKind::DuplicateRows))
        .expect("expected a duplicate-rows suggestion");
    // Count comes straight from `find_duplicate_rows`, whose exact contract
    // (every row in a duplicate group, or only the repeats) is its own to
    // define. Assert only that something was counted.
    assert!(hit.affected >= 1, "{hit:?}");
    assert_eq!(hit.column, None, "duplicates are a whole-table problem");
}

#[test]
fn flags_a_fully_empty_column() {
    let t = table(
        &[("id", "Int64"), ("notes", "Utf8")],
        vec![
            vec![CellValue::Int(1), CellValue::Null],
            vec![CellValue::Int(2), CellValue::Null],
        ],
    );
    assert!(
        scan(&t)
            .iter()
            .any(|s| matches!(s.kind, CleanupKind::EmptyColumn) && s.column == Some(1)),
        "an all-null column should be suggested for dropping"
    );
}

#[test]
fn an_empty_column_is_not_also_reported_as_missing_values() {
    // Otherwise every empty column produces two competing suggestions.
    let t = table(
        &[("notes", "Utf8")],
        vec![vec![CellValue::Null], vec![CellValue::Null]],
    );
    assert!(
        !scan(&t)
            .iter()
            .any(|s| matches!(s.kind, CleanupKind::MissingValues)),
        "EmptyColumn supersedes MissingValues"
    );
}

#[test]
fn a_clean_table_produces_nothing() {
    let t = table(
        &[("id", "Int64"), ("city", "Utf8")],
        vec![
            vec![CellValue::Int(1), CellValue::String("Tokyo".into())],
            vec![CellValue::Int(2), CellValue::String("Cologne".into())],
        ],
    );
    assert!(scan(&t).is_empty(), "clean table: {:?}", scan(&t));
}

#[test]
fn an_empty_table_produces_nothing() {
    assert!(scan(&DataTable::empty()).is_empty());
}

#[test]
fn results_are_ordered_by_severity_then_affected() {
    let t = table(
        &[("city", "Utf8"), ("notes", "Utf8")],
        vec![
            vec![CellValue::String("Tokyo ".into()), CellValue::Null],
            vec![CellValue::String("Bonn ".into()), CellValue::Null],
        ],
    );
    let found = scan(&t);
    for pair in found.windows(2) {
        assert!(
            (pair[0].severity, pair[0].affected) >= (pair[1].severity, pair[1].affected),
            "not sorted: {found:?}"
        );
    }
}

#[test]
fn a_cancelled_scan_returns_early() {
    let t = table(
        &[("city", "Utf8")],
        vec![vec![CellValue::String("Tokyo ".into())]],
    );
    let cancel = AtomicBool::new(true);
    assert!(
        suggest_cleanups(&t, &CleanupLimits::default(), &cancel).is_empty(),
        "a pre-cancelled scan must not do work"
    );
}

#[test]
fn flags_untidy_headers() {
    let t = table(
        &[("Order ID", "Utf8")],
        vec![vec![CellValue::String("a".into())]],
    );
    let found = scan(&t);
    let hit = found
        .iter()
        .find(|s| matches!(s.kind, CleanupKind::UntidyHeaders))
        .expect("expected an untidy-headers suggestion");
    assert_eq!(hit.affected, 1);
    assert_eq!(hit.column, None, "headers are a whole-table problem");
}

#[test]
fn whitespace_examples_are_quoted_so_the_spaces_are_visible() {
    let t = table(
        &[("city", "Utf8")],
        vec![
            vec![CellValue::String("Tokyo ".into())],
            vec![CellValue::String(" Bonn".into())],
        ],
    );
    let found = scan(&t);
    let hit = found
        .iter()
        .find(|s| matches!(s.kind, CleanupKind::TrimWhitespace))
        .expect("expected a whitespace suggestion");
    assert_eq!(hit.examples, vec!["\"Tokyo \"", "\" Bonn\""]);
}

#[test]
fn untidy_header_examples_show_the_rename() {
    let t = table(
        &[("Order ID", "Utf8")],
        vec![vec![CellValue::String("a".into())]],
    );
    let found = scan(&t);
    let hit = found
        .iter()
        .find(|s| matches!(s.kind, CleanupKind::UntidyHeaders))
        .expect("expected an untidy-headers suggestion");
    assert_eq!(hit.examples, vec!["Order ID -> order_id"]);
}

#[test]
fn kinds_without_a_value_to_show_carry_no_examples() {
    // An empty cell and an empty column have nothing to display, so the panel
    // must not offer a Show button for them.
    let t = table(
        &[("notes", "Utf8"), ("mixed", "Utf8")],
        vec![
            vec![CellValue::Null, CellValue::String("a".into())],
            vec![CellValue::Null, CellValue::Null],
        ],
    );
    for s in scan(&t) {
        if matches!(
            s.kind,
            CleanupKind::EmptyColumn | CleanupKind::MissingValues
        ) {
            assert!(s.examples.is_empty(), "{s:?}");
        }
    }
}

#[test]
fn examples_are_capped_and_stable_across_scans() {
    let rows: Vec<Vec<CellValue>> = (0..10)
        .map(|i| vec![CellValue::String(format!("v{i} "))])
        .collect();
    let t = table(&[("city", "Utf8")], rows);
    let first = scan(&t);
    let second = scan(&t);
    let hit = first
        .iter()
        .find(|s| matches!(s.kind, CleanupKind::TrimWhitespace))
        .expect("expected a whitespace suggestion");
    assert_eq!(hit.examples.len(), 3, "capped at three examples");
    assert_eq!(first, second, "a rescan must produce identical output");
}

#[test]
fn tidy_snake_case_headers_are_left_alone() {
    let t = table(
        &[("order_id", "Utf8"), ("city2", "Utf8")],
        vec![vec![
            CellValue::String("a".into()),
            CellValue::String("b".into()),
        ]],
    );
    assert!(
        !scan(&t)
            .iter()
            .any(|s| matches!(s.kind, CleanupKind::UntidyHeaders)),
        "snake_case headers are already tidy"
    );
}

#[test]
fn flags_a_column_with_one_value_all_the_way_down() {
    let t = table(
        &[("region", "Utf8"), ("amount", "Int64")],
        vec![
            vec![CellValue::String("EU".into()), CellValue::Int(1)],
            vec![CellValue::String("EU".into()), CellValue::Int(2)],
            vec![CellValue::String("EU".into()), CellValue::Int(3)],
        ],
    );
    let found = scan(&t);
    let constant: Vec<&Suggestion> = found
        .iter()
        .filter(|s| matches!(s.kind, CleanupKind::ConstantColumn))
        .collect();
    assert_eq!(constant.len(), 1, "{found:?}");
    assert_eq!(constant[0].column, Some(0));
    assert_eq!(constant[0].affected, 3);
    assert_eq!(constant[0].detail, "EU");
}

/// The distinction that keeps the two suggestions apart: a column of one value
/// *and some nulls* is a column with missing values, not a constant one, and
/// dropping it would throw away the fact that some rows had nothing.
#[test]
fn a_column_of_one_value_and_nulls_is_not_constant() {
    let mut rows = vec![vec![CellValue::String("EU".into())]; 9];
    rows.push(vec![CellValue::Null]);
    let t = table(&[("region", "Utf8")], rows);
    assert!(
        !scan(&t)
            .iter()
            .any(|s| matches!(s.kind, CleanupKind::ConstantColumn)),
        "nulls mean the column is incomplete, not constant"
    );
}

/// A one-row table has every column "constant" by accident, which is not a
/// finding anyone can act on.
#[test]
fn a_single_row_is_not_a_constant_column() {
    let t = table(
        &[("region", "Utf8")],
        vec![vec![CellValue::String("EU".into())]],
    );
    assert!(
        !scan(&t)
            .iter()
            .any(|s| matches!(s.kind, CleanupKind::ConstantColumn))
    );
}

/// An empty column is empty, not constant: `EmptyColumn` already covers it and
/// two suggestions for one column would be noise.
#[test]
fn an_empty_column_is_not_also_reported_as_constant() {
    let t = table(&[("note", "Utf8")], vec![vec![CellValue::Null]; 5]);
    let found = scan(&t);
    assert!(
        found
            .iter()
            .any(|s| matches!(s.kind, CleanupKind::EmptyColumn))
    );
    assert!(
        !found
            .iter()
            .any(|s| matches!(s.kind, CleanupKind::ConstantColumn))
    );
}
