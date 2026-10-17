use super::*;
use crate::data::ColumnInfo;

#[test]
fn iqr_flags_extreme_value() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "v".into(),
        data_type: "Int64".into(),
    }];
    t.rows = (1..=10).map(|i| vec![CellValue::Int(i)]).collect();
    t.rows.push(vec![CellValue::Int(1000)]);
    let flagged = detect_outliers(&t, &[0], OutlierMethod::Iqr, 1.5);
    assert!(flagged.contains(&(10, 0)));
    assert!(!flagged.contains(&(0, 0)));
}

#[test]
fn zscore_flags_extreme_value() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "v".into(),
        data_type: "Float64".into(),
    }];
    // Nine values near 0, one extreme outlier
    t.rows = (0..9).map(|_| vec![CellValue::Float(0.0)]).collect();
    t.rows.push(vec![CellValue::Float(100.0)]);
    let flagged = detect_outliers(&t, &[0], OutlierMethod::ZScore, 2.0);
    assert!(flagged.contains(&(9, 0)));
    assert!(!flagged.contains(&(0, 0)));
}

#[test]
fn skips_column_with_fewer_than_four_values() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "v".into(),
        data_type: "Int64".into(),
    }];
    t.rows = vec![
        vec![CellValue::Int(1)],
        vec![CellValue::Int(2)],
        vec![CellValue::Int(1000)],
    ];
    let flagged = detect_outliers(&t, &[0], OutlierMethod::Iqr, 1.5);
    assert!(flagged.is_empty());
}

#[test]
fn no_outliers_in_uniform_column() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "v".into(),
        data_type: "Int64".into(),
    }];
    t.rows = (0..10).map(|_| vec![CellValue::Int(5)]).collect();
    // All same value: IQR = 0, so bounds collapse; none should be flagged
    // (sd = 0 path for ZScore skips the column)
    let flagged = detect_outliers(&t, &[0], OutlierMethod::ZScore, 1.5);
    assert!(flagged.is_empty());
}

#[test]
fn a_nan_column_is_analysed_instead_of_crashing() {
    // R writes NaN and pandas writes NaN for a missing number, so a real
    // CSV brings them in. They used to reach `partial_cmp().unwrap()` and
    // panic the app on the UI thread.
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "v".into(),
        data_type: "Float64".into(),
    }];
    t.rows = (1..=10).map(|i| vec![CellValue::Int(i)]).collect();
    t.rows.push(vec![CellValue::String("NaN".into())]);
    t.rows.push(vec![CellValue::Float(f64::NAN)]);
    t.rows.push(vec![CellValue::Int(1000)]);

    let flagged = detect_outliers(&t, &[0], OutlierMethod::Iqr, 1.5);
    assert!(
        flagged.contains(&(12, 0)),
        "the real outlier is still found"
    );
    assert!(
        !flagged.contains(&(10, 0)),
        "NaN is missing, not an outlier"
    );
    assert!(!flagged.contains(&(11, 0)));
}
