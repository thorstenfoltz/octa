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

/// A Duration is an amount, so it is tested like a number; a clock time wraps
/// at midnight (23:59 sits beside 00:01), so it is not.
#[test]
fn a_duration_column_is_tested_and_a_time_column_is_not() {
    let h = 3_600_000_000_000i64;
    let mut rows: Vec<Vec<CellValue>> = [8, 8, 7, 9, 8, 8, 7, 9]
        .iter()
        .map(|x| vec![CellValue::Duration(x * h), CellValue::Time(x * h)])
        .collect();
    rows.push(vec![CellValue::Duration(100 * h), CellValue::Time(23 * h)]);
    let table = DataTable {
        columns: vec![
            crate::data::ColumnInfo {
                name: "d".into(),
                data_type: "Duration(µs)".into(),
            },
            crate::data::ColumnInfo {
                name: "t".into(),
                data_type: "Time64(µs)".into(),
            },
        ],
        rows,
        ..DataTable::empty()
    };
    let found = detect_outliers(&table, &[0, 1], OutlierMethod::Iqr, 1.5);
    assert_eq!(found, HashSet::from([(8, 0)]));
}

/// day (Date), value
fn daily(values: &[f64]) -> DataTable {
    let start = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(); // a Monday
    let mut t = DataTable::empty();
    t.columns = [("day", "Date32"), ("value", "Float64")]
        .iter()
        .map(|(n, ty)| ColumnInfo {
            name: (*n).into(),
            data_type: (*ty).into(),
        })
        .collect();
    t.rows = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let day = start + chrono::Duration::days(i as i64);
            vec![
                CellValue::Date(day.format("%Y-%m-%d").to_string()),
                CellValue::Float(*v),
            ]
        })
        .collect();
    t
}

fn neighbours(window: usize, same_weekday: bool) -> OutlierMethod {
    OutlierMethod::Neighbours {
        order: 0,
        window,
        same_weekday,
    }
}

#[test]
fn a_dip_inside_a_trend_is_found_by_its_neighbours_and_not_by_the_column() {
    // Rising from 0 to about 600; day 30 should be near 300 and reads 100.
    let mut values: Vec<f64> = (0..60).map(|i| (i * 10 + i % 3) as f64).collect();
    values[30] = 100.0;
    let t = daily(&values);
    assert!(
        detect_outliers(&t, &[1], OutlierMethod::Iqr, 1.5).is_empty(),
        "100 is an ordinary value for the column as a whole"
    );
    let flagged = detect_outliers(&t, &[0, 1], neighbours(DEFAULT_WINDOW, false), 3.0);
    assert_eq!(flagged, HashSet::from([(30, 1)]));
}

#[test]
fn rows_need_not_be_in_date_order() {
    let mut values: Vec<f64> = (0..60).map(|i| (i * 10 + i % 3) as f64).collect();
    values[30] = 100.0;
    let mut t = daily(&values);
    t.rows.reverse();
    let flagged = detect_outliers(&t, &[1], neighbours(DEFAULT_WINDOW, false), 3.0);
    assert_eq!(flagged, HashSet::from([(29, 1)]));
}

#[test]
fn a_sunday_is_compared_with_sundays() {
    // Eight weeks: weekdays near 100, Sundays near 10, and one Sunday at 100.
    let mut values: Vec<f64> = (0..56)
        .map(|i| {
            if i % 7 == 6 {
                (10 + (i / 7) % 3) as f64
            } else {
                (100 + i % 5) as f64
            }
        })
        .collect();
    values[27] = 100.0; // the fourth Sunday
    let t = daily(&values);
    let flagged = detect_outliers(&t, &[1], neighbours(DEFAULT_WINDOW, true), 3.0);
    assert_eq!(flagged, HashSet::from([(27, 1)]));
    // Against all neighbours every ordinary Sunday stands out instead.
    let plain = detect_outliers(&t, &[1], neighbours(DEFAULT_WINDOW, false), 3.0);
    assert!(
        plain.contains(&(6, 1)) && !plain.contains(&(27, 1)),
        "{plain:?}"
    );
}

#[test]
fn a_flat_series_flags_only_the_value_that_differs() {
    let mut values = vec![5.0; 20];
    values[9] = 6.0;
    let t = daily(&values);
    let flagged = detect_outliers(&t, &[1], neighbours(3, false), 3.0);
    assert_eq!(flagged, HashSet::from([(9, 1)]));
}

#[test]
fn a_row_without_a_date_is_left_out() {
    let mut values: Vec<f64> = (0..30).map(|i| (i % 4) as f64).collect();
    values[10] = 500.0;
    let mut t = daily(&values);
    t.rows[10][0] = CellValue::Null;
    assert!(detect_outliers(&t, &[1], neighbours(DEFAULT_WINDOW, false), 3.0).is_empty());
}
