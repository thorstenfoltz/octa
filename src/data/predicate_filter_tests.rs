use super::*;
use crate::data::{CellValue, ColumnInfo};

fn f(col: usize, op: CondOp, value: &str) -> PredicateFilter {
    PredicateFilter {
        col,
        op,
        value: value.to_string(),
        case_sensitive: false,
    }
}

#[test]
fn numeric_comparison_uses_numbers_not_strings() {
    assert!(matches(&f(0, CondOp::Gt, "1000"), "1500"));
    assert!(!matches(&f(0, CondOp::Gt, "1000"), "900"));
    // As strings "900" sorts after "1000"; the shared engine coerces both
    // sides to f64, so this is a numeric comparison.
    assert!(matches(&f(0, CondOp::Le, "1000"), "1000"));
}

#[test]
fn text_operators_still_work() {
    assert!(matches(&f(0, CondOp::Contains, "err"), "server error"));
    assert!(matches(&f(0, CondOp::Eq, "DE"), "de"));
    let sensitive = PredicateFilter {
        case_sensitive: true,
        ..f(0, CondOp::Eq, "DE")
    };
    assert!(!matches(&sensitive, "de"));
}

#[test]
fn all_filters_must_pass() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "amount".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "country".into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::Int(1500), CellValue::String("DE".into())],
        vec![CellValue::Int(1500), CellValue::String("FR".into())],
        vec![CellValue::Int(100), CellValue::String("DE".into())],
    ];
    let filters = vec![f(0, CondOp::Gt, "1000"), f(1, CondOp::Eq, "DE")];
    assert!(row_passes(&filters, &t, 0));
    assert!(!row_passes(&filters, &t, 1));
    assert!(!row_passes(&filters, &t, 2));
    // No filters means no filtering.
    assert!(row_passes(&[], &t, 2));
}

#[test]
fn out_of_range_column_never_passes() {
    let t = DataTable::empty();
    assert!(!row_passes(&[f(9, CondOp::Eq, "x")], &t, 0));
}

#[test]
fn label_names_the_column_and_operator() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "amount".into(),
        data_type: "Int64".into(),
    }];
    let label = f(0, CondOp::Gt, "1000").label(&t);
    assert!(label.starts_with("amount "), "{label}");
    assert!(label.ends_with(" 1000"), "{label}");
    // Operators with no operand do not print a dangling value.
    let empty = f(0, CondOp::Empty, "").label(&t);
    assert!(!empty.ends_with(' '), "{empty}");
}
