use super::*;
use crate::data::ColumnInfo;

fn col(vals: Vec<CellValue>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "v".into(),
        data_type: "Float64".into(),
    }];
    t.rows = vals.into_iter().map(|v| vec![v]).collect();
    t
}

#[test]
fn mean_fills_nulls() {
    let t = col(vec![
        CellValue::Float(2.0),
        CellValue::Null,
        CellValue::Float(4.0),
    ]);
    let out = impute_column(&t, 0, &ImputeStrategy::Mean).unwrap();
    assert_eq!(out[1], CellValue::Float(3.0));
    assert_eq!(out[0], CellValue::Float(2.0));
}

#[test]
fn constant_fills_nulls() {
    let t = col(vec![CellValue::Null, CellValue::Float(1.0)]);
    let out = impute_column(&t, 0, &ImputeStrategy::Constant("9".into())).unwrap();
    assert_eq!(out[0], CellValue::Int(9));
}

#[test]
fn mean_on_text_errors() {
    let mut t = col(vec![CellValue::String("x".into())]);
    t.columns[0].data_type = "Utf8".into();
    assert!(impute_column(&t, 0, &ImputeStrategy::Mean).is_err());
}

#[test]
fn median_even_count() {
    let t = col(vec![
        CellValue::Float(1.0),
        CellValue::Float(3.0),
        CellValue::Float(5.0),
        CellValue::Float(7.0),
    ]);
    let out = impute_column(&t, 0, &ImputeStrategy::Median).unwrap();
    // No nulls, values unchanged
    assert_eq!(out[0], CellValue::Float(1.0));
}

#[test]
fn median_fills_null_with_middle_value() {
    let t = col(vec![
        CellValue::Float(1.0),
        CellValue::Null,
        CellValue::Float(3.0),
    ]);
    let out = impute_column(&t, 0, &ImputeStrategy::Median).unwrap();
    // median of [1,3] = 2.0
    assert_eq!(out[1], CellValue::Float(2.0));
}

#[test]
fn mode_fills_null_with_most_frequent() {
    let t = col(vec![
        CellValue::Float(5.0),
        CellValue::Float(5.0),
        CellValue::Float(3.0),
        CellValue::Null,
    ]);
    let out = impute_column(&t, 0, &ImputeStrategy::Mode).unwrap();
    assert_eq!(out[3], CellValue::Float(5.0));
}

#[test]
fn constant_string_fills_null() {
    let t = col(vec![CellValue::Null, CellValue::Float(1.0)]);
    let out = impute_column(&t, 0, &ImputeStrategy::Constant("hello".into())).unwrap();
    assert_eq!(out[0], CellValue::String("hello".into()));
}

#[test]
fn forward_fill_delegates_to_fill_down() {
    let t = col(vec![
        CellValue::Float(1.0),
        CellValue::Null,
        CellValue::Null,
    ]);
    let out = impute_column(&t, 0, &ImputeStrategy::ForwardFill).unwrap();
    assert_eq!(out[1], CellValue::Float(1.0));
    assert_eq!(out[2], CellValue::Float(1.0));
}

#[test]
fn backward_fill_delegates_to_fill_up() {
    let t = col(vec![
        CellValue::Null,
        CellValue::Null,
        CellValue::Float(9.0),
    ]);
    let out = impute_column(&t, 0, &ImputeStrategy::BackwardFill).unwrap();
    assert_eq!(out[0], CellValue::Float(9.0));
    assert_eq!(out[1], CellValue::Float(9.0));
}

#[test]
fn a_nan_is_treated_as_missing_rather_than_crashing_the_median() {
    let t = col(vec![
        CellValue::Float(1.0),
        CellValue::Float(f64::NAN),
        CellValue::Float(3.0),
        CellValue::Null,
        CellValue::String("NaN".into()),
    ]);
    let out = impute_column(&t, 0, &ImputeStrategy::Median).unwrap();
    assert_eq!(out[3], CellValue::Float(2.0), "median of 1 and 3");
    let out = impute_column(&t, 0, &ImputeStrategy::Mean).unwrap();
    assert_eq!(
        out[3],
        CellValue::Float(2.0),
        "NaN must not poison the mean"
    );
}
