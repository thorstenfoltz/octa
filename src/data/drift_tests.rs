use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn table(col: &str, ty: &str, vals: Vec<CellValue>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: col.into(),
        data_type: ty.into(),
    }];
    t.rows = vals.into_iter().map(|v| vec![v]).collect();
    t
}

#[test]
fn reports_a_null_rate_increase() {
    let a = table(
        "amount",
        "Int64",
        vec![CellValue::Int(1), CellValue::Int(2)],
    );
    let b = table("amount", "Int64", vec![CellValue::Int(1), CellValue::Null]);
    let r = compare_profiles(&a, &b, &DriftOptions::default()).unwrap();
    let null_row = r
        .rows
        .iter()
        .find(|d| d.column == "amount" && d.metric == "null_rate")
        .expect("null_rate row missing");
    assert_eq!(null_row.before, Some(0.0));
    assert_eq!(null_row.after, Some(0.5));
}

#[test]
fn reports_added_and_removed_columns() {
    let mut a = DataTable::empty();
    a.columns = vec![
        ColumnInfo {
            name: "keep".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "gone".into(),
            data_type: "Utf8".into(),
        },
    ];
    a.rows = vec![vec![
        CellValue::String("x".into()),
        CellValue::String("y".into()),
    ]];
    let mut b = DataTable::empty();
    b.columns = vec![
        ColumnInfo {
            name: "keep".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "fresh".into(),
            data_type: "Utf8".into(),
        },
    ];
    b.rows = vec![vec![
        CellValue::String("x".into()),
        CellValue::String("z".into()),
    ]];

    let r = compare_profiles(&a, &b, &DriftOptions::default()).unwrap();
    assert_eq!(r.removed_columns, vec!["gone".to_string()]);
    assert_eq!(r.added_columns, vec!["fresh".to_string()]);
}

#[test]
fn reports_new_and_vanished_categories() {
    let a = table(
        "country",
        "Utf8",
        vec![
            CellValue::String("DE".into()),
            CellValue::String("PL".into()),
        ],
    );
    let b = table(
        "country",
        "Utf8",
        vec![
            CellValue::String("DE".into()),
            CellValue::String("UA".into()),
        ],
    );
    let r = compare_profiles(&a, &b, &DriftOptions::default()).unwrap();
    let new = r.rows.iter().find(|d| d.metric == "new_values").unwrap();
    assert!(new.after_text.contains("UA"), "got {}", new.after_text);
    let gone = r
        .rows
        .iter()
        .find(|d| d.metric == "vanished_values")
        .unwrap();
    assert!(gone.before_text.contains("PL"), "got {}", gone.before_text);
}

#[test]
fn a_breached_threshold_fails_the_report() {
    let a = table(
        "amount",
        "Int64",
        vec![CellValue::Int(1), CellValue::Int(2)],
    );
    let b = table("amount", "Int64", vec![CellValue::Int(1), CellValue::Null]);
    let opts = DriftOptions {
        thresholds: parse_thresholds("null_rate:0.05").unwrap(),
        ..Default::default()
    };
    let r = compare_profiles(&a, &b, &opts).unwrap();
    assert!(r.failed, "0.0 -> 0.5 must breach a 0.05 threshold");
    assert!(r.rows.iter().any(|d| d.breached));
}

#[test]
fn thresholds_reject_a_malformed_spec() {
    assert!(parse_thresholds("null_rate").is_err());
    assert!(parse_thresholds("null_rate:abc").is_err());
    let ok = parse_thresholds("null_rate:0.05,rows:0.1").unwrap();
    assert_eq!(ok.len(), 2);
}

#[test]
fn identical_tables_have_no_breach_and_equal_row_counts() {
    let a = table("x", "Int64", vec![CellValue::Int(1)]);
    let r = compare_profiles(&a, &a, &DriftOptions::default()).unwrap();
    assert!(!r.failed);
    assert_eq!(r.rows_before, r.rows_after);
}
