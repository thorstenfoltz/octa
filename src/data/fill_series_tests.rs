use std::collections::HashSet;

use super::*;

fn s(v: &str) -> CellValue {
    CellValue::String(v.into())
}

#[test]
fn two_numbers_give_their_step() {
    let g = guess(&[CellValue::Int(1), CellValue::Int(3), CellValue::Null]);
    assert_eq!(
        (g.kind, g.start.as_str(), g.step.as_str()),
        (FillKind::Numbers, "1", "2")
    );
    assert_eq!(values(&g, &[], 4).unwrap(), ["1", "3", "5", "7"]);
}

#[test]
fn one_number_counts_by_one() {
    let g = guess(&[CellValue::Int(10), CellValue::Null]);
    assert_eq!(values(&g, &[], 3).unwrap(), ["10", "11", "12"]);
}

#[test]
fn decimal_and_negative_steps() {
    let spec = FillSpec {
        kind: FillKind::Numbers,
        start: "1.5".into(),
        step: "-0.25".into(),
        ..FillSpec::default()
    };
    assert_eq!(values(&spec, &[], 3).unwrap(), ["1.50", "1.25", "1.00"]);
}

#[test]
fn an_id_counts_on_and_keeps_its_zeros() {
    let g = guess(&[s("INV-0998"), CellValue::Null]);
    assert_eq!(g.kind, FillKind::Counter);
    assert_eq!(
        values(&g, &[], 3).unwrap(),
        ["INV-0998", "INV-0999", "INV-1000"]
    );
    let spec = FillSpec {
        step: "-1".into(),
        start: "A-0001".into(),
        ..g
    };
    assert_eq!(values(&spec, &[], 3), Err(FillError::BelowZero));
}

#[test]
fn month_steps_hold_the_month_end() {
    let spec = FillSpec {
        kind: FillKind::Dates,
        start: "2024-01-31".into(),
        step: "1".into(),
        unit: DateUnit::Month,
        ..FillSpec::default()
    };
    assert_eq!(
        values(&spec, &[], 3).unwrap(),
        ["2024-01-31", "2024-02-29", "2024-03-31"]
    );
}

#[test]
fn workdays_skip_the_weekend() {
    // 2024-11-01 is a Friday.
    let spec = FillSpec {
        kind: FillKind::Dates,
        start: "2024-11-01".into(),
        step: "1".into(),
        unit: DateUnit::Workday,
        ..FillSpec::default()
    };
    assert_eq!(
        values(&spec, &[], 3).unwrap(),
        ["2024-11-01", "2024-11-04", "2024-11-05"]
    );
}

#[test]
fn dates_guess_week_and_month_steps() {
    let g = guess(&[
        CellValue::Date("2024-01-01".into()),
        CellValue::Date("2024-01-15".into()),
    ]);
    assert_eq!(
        (g.kind, g.unit, g.step.as_str()),
        (FillKind::Dates, DateUnit::Week, "2")
    );
    let g = guess(&[
        CellValue::Date("2024-01-31".into()),
        CellValue::Date("2024-04-30".into()),
    ]);
    assert_eq!((g.unit, g.step.as_str()), (DateUnit::Day, "90"));
    let g = guess(&[
        CellValue::Date("2024-01-15".into()),
        CellValue::Date("2025-01-15".into()),
    ]);
    assert_eq!((g.unit, g.step.as_str()), (DateUnit::Year, "1"));
}

#[test]
fn a_timestamp_keeps_its_time() {
    let g = guess(&[CellValue::DateTime("2024-03-01 08:30:00".into())]);
    assert_eq!(
        values(&g, &[], 2).unwrap(),
        ["2024-03-01 08:30:00", "2024-03-02 08:30:00"]
    );
}

#[test]
fn other_text_repeats_its_pattern() {
    let own = [
        s("a"),
        s("b"),
        s("c"),
        CellValue::Null,
        CellValue::Null,
        CellValue::Null,
        CellValue::Null,
    ];
    let g = guess(&own);
    assert_eq!((g.kind, g.pattern), (FillKind::Repeat, 3));
    assert_eq!(
        values(&g, &own, 7).unwrap(),
        ["a", "b", "c", "a", "b", "c", "a"]
    );
    let one = FillSpec { pattern: 1, ..g };
    assert_eq!(values(&one, &own, 2).unwrap(), ["a", "a"]);
}

#[test]
fn targets_follow_the_display_order_and_skip_hidden_rows() {
    let cells: HashSet<(usize, usize)> = [(5, 1), (2, 1), (9, 1), (3, 0)].into_iter().collect();
    // Row 9 is filtered out; display order is 5, 2, 3.
    let t = targets(&HashSet::new(), &cells, &[5, 2, 3]);
    assert_eq!(t, vec![(1, vec![5, 2])]);
    let cols: HashSet<usize> = [0].into_iter().collect();
    assert_eq!(targets(&cols, &cells, &[5, 2, 3]), vec![(0, vec![5, 2, 3])]);
}
