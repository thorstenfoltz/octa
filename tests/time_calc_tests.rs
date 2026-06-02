//! Integration coverage for the date/time calculation library used by the
//! "Date/Time calculation" dialog.

use octa::data::CellValue;
use octa::data::time_calc::{
    DateComponent, TimeCalcOp, TimeUnit, cell_arrow_type, evaluate_cell, result_type_name,
};

#[test]
fn difference_over_a_column_of_rows() {
    let starts = [
        CellValue::Date("2024-01-01".into()),
        CellValue::Date("2024-02-01".into()),
        CellValue::String("not a date".into()),
    ];
    let ends = [
        CellValue::Date("2024-01-08".into()),
        CellValue::Date("2024-02-15".into()),
        CellValue::Date("2024-03-01".into()),
    ];
    let op = TimeCalcOp::Difference {
        unit: TimeUnit::Days,
    };
    let out: Vec<Option<CellValue>> = starts
        .iter()
        .zip(ends.iter())
        .map(|(a, b)| evaluate_cell(op, a, Some(b)))
        .collect();
    assert_eq!(out[0], Some(CellValue::Float(7.0)));
    assert_eq!(out[1], Some(CellValue::Float(14.0)));
    assert_eq!(out[2], None); // bad row skipped
}

#[test]
fn result_types_are_stable() {
    assert_eq!(
        result_type_name(TimeCalcOp::Difference {
            unit: TimeUnit::Days
        }),
        "Float64"
    );
    assert_eq!(
        result_type_name(TimeCalcOp::Extract {
            component: DateComponent::Year
        }),
        "Int64"
    );
    assert_eq!(
        result_type_name(TimeCalcOp::ConvertDuration {
            from: TimeUnit::Seconds,
            to: TimeUnit::Minutes
        }),
        "Float64"
    );
    // Add/subtract whole-day amounts default to a date column.
    assert_eq!(
        result_type_name(TimeCalcOp::AddSubtract {
            unit: TimeUnit::Days,
            amount: 1
        }),
        "Date32"
    );
    assert_eq!(
        result_type_name(TimeCalcOp::AddSubtract {
            unit: TimeUnit::Hours,
            amount: 1
        }),
        "Timestamp(Microsecond, None)"
    );
}

#[test]
fn cell_arrow_type_matches_variant() {
    assert_eq!(
        cell_arrow_type(&CellValue::Date("2024-01-01".into())),
        "Date32"
    );
    assert_eq!(
        cell_arrow_type(&CellValue::DateTime("2024-01-01 00:00:00".into())),
        "Timestamp(Microsecond, None)"
    );
    assert_eq!(cell_arrow_type(&CellValue::Int(5)), "Int64");
    assert_eq!(cell_arrow_type(&CellValue::Float(1.5)), "Float64");
}
