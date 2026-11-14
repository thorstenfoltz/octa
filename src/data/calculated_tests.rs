use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn i(v: i64) -> CellValue {
    CellValue::Int(v)
}
fn f(v: f64) -> CellValue {
    CellValue::Float(v)
}
fn s(v: &str) -> CellValue {
    CellValue::String(v.into())
}
const N: CellValue = CellValue::Null;

/// region, day, amount
fn sales() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = [("region", "Utf8"), ("day", "Int64"), ("amount", "Int64")]
        .iter()
        .map(|(n, ty)| ColumnInfo {
            name: (*n).into(),
            data_type: (*ty).into(),
        })
        .collect();
    t.rows = vec![
        vec![s("north"), i(2), i(10)],
        vec![s("south"), i(1), i(40)],
        vec![s("north"), i(1), i(30)],
        vec![s("north"), i(3), i(30)],
        vec![s("south"), i(2), N],
    ];
    t
}

fn spec(
    kind: CalcKind,
    value: Option<usize>,
    order: Option<usize>,
    desc: bool,
    groups: Vec<usize>,
) -> CalcSpec {
    CalcSpec {
        kind,
        value,
        order,
        descending: desc,
        groups,
        extras: CalcExtras::default(),
    }
}

#[test]
fn rank_shares_a_place_and_leaves_a_gap_largest_first() {
    let t = sales();
    let out = compute(&t, &spec(CalcKind::Rank, Some(2), None, true, vec![])).unwrap();
    // 40 -> 1, 30 30 -> 2 2, 10 -> 4, empty last -> 5
    assert_eq!(out, vec![i(4), i(1), i(2), i(2), i(5)]);
}

#[test]
fn dense_rank_leaves_no_gap() {
    let t = sales();
    let out = compute(&t, &spec(CalcKind::DenseRank, Some(2), None, true, vec![])).unwrap();
    assert_eq!(out, vec![i(3), i(1), i(2), i(2), i(4)]);
}

#[test]
fn rank_restarts_for_each_group() {
    let t = sales();
    let out = compute(&t, &spec(CalcKind::Rank, Some(2), None, true, vec![0])).unwrap();
    // north: 10, 30, 30 -> 3, 1, 1; south: 40, empty -> 1, 2
    assert_eq!(out, vec![i(3), i(1), i(1), i(1), i(2)]);
}

#[test]
fn row_number_follows_the_order_and_keeps_table_order_on_ties() {
    let t = sales();
    let by_day = compute(&t, &spec(CalcKind::RowNumber, None, Some(1), false, vec![])).unwrap();
    // days 2,1,1,3,2 -> row 1 (day 1) first, then row 2 (day 1), row 0, row 4, row 3
    assert_eq!(by_day, vec![i(3), i(1), i(2), i(5), i(4)]);
    let table_order = compute(&t, &spec(CalcKind::RowNumber, None, None, false, vec![])).unwrap();
    assert_eq!(table_order, vec![i(1), i(2), i(3), i(4), i(5)]);
}

#[test]
fn running_total_carries_over_empty_values_per_group_in_order() {
    let t = sales();
    let out = compute(
        &t,
        &spec(CalcKind::RunningTotal, Some(2), Some(1), false, vec![0]),
    )
    .unwrap();
    // north by day: 30 (day1) 10 (day2) 30 (day3) -> 30, 40, 70; south: 40, then empty -> 40
    assert_eq!(out, vec![f(40.0), f(40.0), f(30.0), f(70.0), f(40.0)]);
}

#[test]
fn percent_of_total_is_a_percentage_per_group() {
    let t = sales();
    let out = compute(
        &t,
        &spec(CalcKind::PercentOfTotal, Some(2), None, false, vec![0]),
    )
    .unwrap();
    // north total 70, south total 40
    assert_eq!(out[0], f(10.0 / 70.0 * 100.0));
    assert_eq!(out[1], f(100.0));
    assert_eq!(out[4], N);
}

#[test]
fn a_zero_total_gives_empty_percentages() {
    let mut t = sales();
    for r in &mut t.rows {
        r[2] = i(0);
    }
    let out = compute(
        &t,
        &spec(CalcKind::PercentOfTotal, Some(2), None, false, vec![]),
    )
    .unwrap();
    assert!(out.iter().all(|v| *v == N));
}

#[test]
fn change_to_previous_row_is_empty_at_the_start_and_beside_empties() {
    let t = sales();
    let diff = compute(
        &t,
        &spec(CalcKind::DiffPrev, Some(2), Some(1), false, vec![0]),
    )
    .unwrap();
    // north by day: 30, 10, 30 -> -, -20, 20; south: 40, empty -> -, -
    assert_eq!(diff, vec![f(-20.0), N, N, f(20.0), N]);
    let pct = compute(
        &t,
        &spec(CalcKind::PctChangePrev, Some(2), Some(1), false, vec![0]),
    )
    .unwrap();
    assert_eq!(pct[0], f((10.0 - 30.0) / 30.0 * 100.0));
    assert_eq!(pct[3], f(200.0));
}

#[test]
fn percent_change_from_zero_is_empty() {
    let mut t = sales();
    t.rows[2][2] = i(0); // north day 1 becomes 0
    let pct = compute(
        &t,
        &spec(CalcKind::PctChangePrev, Some(2), Some(1), false, vec![0]),
    )
    .unwrap();
    assert_eq!(pct[0], N);
}

#[test]
fn numbers_stored_as_text_are_read_as_numbers() {
    let mut t = sales();
    for r in &mut t.rows {
        if let CellValue::Int(v) = r[2] {
            r[2] = s(&v.to_string());
        }
    }
    t.rows[0][2] = s("9");
    t.rows[1][2] = s("10");
    let out = compute(&t, &spec(CalcKind::Rank, Some(2), None, true, vec![])).unwrap();
    // 10 > 9 as numbers, not as text
    assert_eq!(out[1], i(3));
    assert_eq!(out[0], i(4));
}

#[test]
fn text_that_is_no_number_refuses_the_arithmetic_kinds() {
    let mut t = sales();
    t.rows[0][2] = s("lots");
    let err = compute(
        &t,
        &spec(CalcKind::RunningTotal, Some(2), None, false, vec![]),
    )
    .unwrap_err();
    assert!(err.contains("amount") && err.contains("lots"), "{err}");
    // Rank can still order text.
    assert!(compute(&t, &spec(CalcKind::Rank, Some(2), None, true, vec![])).is_ok());
}

#[test]
fn the_new_column_is_named_typed_placed_and_uniquified() {
    let mut t = sales();
    let (name, idx) = add_calculated_column(
        &mut t,
        &spec(CalcKind::Rank, Some(2), None, true, vec![]),
        None,
    )
    .unwrap();
    assert_eq!((name.as_str(), idx), ("amount_rank", 3));
    assert_eq!(t.columns[3].data_type, "Int64");
    let (again, _) = add_calculated_column(
        &mut t,
        &spec(CalcKind::Rank, Some(2), None, true, vec![]),
        None,
    )
    .unwrap();
    assert_eq!(again, "amount_rank_2");
    let (rn, rn_idx) = add_calculated_column(
        &mut t,
        &spec(CalcKind::RowNumber, None, None, false, vec![]),
        None,
    )
    .unwrap();
    assert_eq!((rn.as_str(), rn_idx), ("row_number", 0));
    let (pct, _) = add_calculated_column(
        &mut t,
        &spec(CalcKind::PercentOfTotal, Some(3), None, false, vec![]),
        Some("share"),
    )
    .unwrap();
    assert_eq!(pct, "share");
    assert_eq!(
        t.columns[t.column_index("share").unwrap()].data_type,
        "Float64"
    );
}

#[test]
fn by_name_resolves_and_defaults_rank_to_largest_first() {
    let t = sales();
    let by = CalcByName {
        kind: CalcKind::Rank,
        value: Some("amount".into()),
        order: None,
        descending: None,
        groups: vec!["region".into()],
        extras: CalcExtras::default(),
    };
    let sp = by.resolve(&t).unwrap();
    assert_eq!(
        (sp.value, sp.descending, sp.groups.clone()),
        (Some(2), true, vec![0])
    );
    let missing = CalcByName {
        value: Some("nope".into()),
        ..by
    };
    assert!(missing.resolve(&t).is_err());
}

#[test]
fn kind_ids_round_trip() {
    for k in CalcKind::ALL {
        assert_eq!(CalcKind::parse(k.id()), Some(k));
    }
    assert_eq!(
        CalcKind::parse("PCT_CHANGE_PREV"),
        Some(CalcKind::PctChangePrev)
    );
    assert_eq!(CalcKind::parse("median"), None);
}

/// A script that asks for `amount` and quietly gets `amount_2` reads the wrong column
/// next. A name you chose is yours or refused; only a default gets a number.
#[test]
fn a_name_you_chose_that_is_taken_is_refused() {
    let mut t = sales();
    let before = t.col_count();
    let err = add_calculated_column(
        &mut t,
        &spec(CalcKind::Rank, Some(2), None, true, vec![]),
        Some(" amount "),
    )
    .unwrap_err();
    assert!(format!("{err:#}").contains("amount"), "{err:#}");
    assert_eq!(t.col_count(), before);
}

#[test]
fn previous_and_next_hand_the_neighbours_value_on_inside_a_group() {
    let t = sales();
    // north by day: 30 (day 1), 10 (day 2), 30 (day 3); south: 40, empty.
    let prev = compute(
        &t,
        &spec(CalcKind::Previous, Some(2), Some(1), false, vec![0]),
    )
    .unwrap();
    assert_eq!(prev, vec![i(30), N, N, i(10), i(40)]);
    let next = compute(&t, &spec(CalcKind::Next, Some(2), Some(1), false, vec![0])).unwrap();
    assert_eq!(next, vec![i(30), N, i(10), N, N]);
    let two = CalcSpec {
        extras: CalcExtras {
            offset: Some(2),
            ..CalcExtras::default()
        },
        ..spec(CalcKind::Previous, Some(2), Some(1), false, vec![0])
    };
    assert_eq!(compute(&t, &two).unwrap(), vec![N, N, N, i(30), N]);
    let none = CalcSpec {
        extras: CalcExtras {
            offset: Some(0),
            ..CalcExtras::default()
        },
        ..two
    };
    assert!(compute(&t, &none).is_err());
}

#[test]
fn previous_keeps_the_value_columns_type() {
    let mut t = sales();
    let sp = spec(CalcKind::Previous, Some(0), None, false, vec![]);
    let (name, idx) = add_calculated_column(&mut t, &sp, None).unwrap();
    assert_eq!(name, "region_previous");
    assert_eq!(t.columns[idx].data_type, "Utf8");
    assert_eq!(t.get(1, idx), Some(&s("north")));
}

/// user, time
fn visits() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = [("user", "Utf8"), ("at", "Utf8")]
        .iter()
        .map(|(n, ty)| ColumnInfo {
            name: (*n).into(),
            data_type: (*ty).into(),
        })
        .collect();
    t.rows = vec![
        vec![s("a"), s("2026-01-01 10:40:00")],
        vec![s("a"), s("2026-01-01 10:00:00")],
        vec![s("b"), s("2026-01-01 10:05:00")],
        vec![s("a"), s("2026-01-01 10:30:00")],
        vec![s("a"), s("2026-01-01 11:10:01")],
        vec![s("a"), N],
        vec![s("a"), CellValue::Date("2026-01-02".into())],
    ];
    t
}

#[test]
fn a_session_starts_after_a_pause_longer_than_the_gap() {
    let t = visits();
    let sp = spec(CalcKind::Session, Some(1), None, false, vec![0]);
    // a: 10:00, 10:30 (exactly the gap, same visit), 10:40, then 11:10:01
    // (one second over), then the next day. Rows need not be in time order.
    assert_eq!(
        compute(&t, &sp).unwrap(),
        vec![i(1), i(1), i(1), i(1), i(2), N, i(3)]
    );
    let tight = CalcSpec {
        extras: CalcExtras {
            gap_minutes: Some(5.0),
            ..CalcExtras::default()
        },
        ..sp
    };
    assert_eq!(
        compute(&t, &tight).unwrap(),
        vec![i(3), i(1), i(1), i(2), i(4), N, i(5)]
    );
}

#[test]
fn a_session_refuses_text_that_is_no_time() {
    let mut t = visits();
    t.rows[0][1] = s("soon");
    let e = compute(&t, &spec(CalcKind::Session, Some(1), None, false, vec![])).unwrap_err();
    assert!(e.contains("soon"), "{e}");
}

#[test]
fn abc_classes_follow_the_running_share_and_the_limits_can_be_set() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "revenue".into(),
        data_type: "Int64".into(),
    }];
    // Total 100. Shares before each row, largest first: 0, 50, 80, 90, 95, 99.
    t.rows = [1, 50, 5, 30, 10, 4].iter().map(|&v| vec![i(v)]).collect();
    t.rows.push(vec![N]);
    let sp = spec(CalcKind::Abc, Some(0), None, false, vec![]);
    assert_eq!(
        compute(&t, &sp).unwrap(),
        vec![s("C"), s("A"), s("B"), s("A"), s("B"), s("C"), N]
    );
    let own = CalcSpec {
        extras: CalcExtras {
            abc_a: Some(50.0),
            abc_b: Some(91.0),
            ..CalcExtras::default()
        },
        ..sp.clone()
    };
    assert_eq!(
        compute(&t, &own).unwrap(),
        vec![s("C"), s("A"), s("B"), s("B"), s("B"), s("C"), N]
    );
    let wrong = CalcSpec {
        extras: CalcExtras {
            abc_a: Some(95.0),
            abc_b: Some(80.0),
            ..CalcExtras::default()
        },
        ..sp.clone()
    };
    assert!(compute(&t, &wrong).is_err());
    t.rows[0][0] = i(-1);
    assert!(compute(&t, &sp).unwrap_err().contains("negative"));
}

#[test]
fn the_extra_settings_ride_flat_in_a_recipe_and_default_when_absent() {
    let by: CalcByName =
        toml::from_str("kind = \"abc\"\nvalue = \"revenue\"\nabc_a = 70.0").unwrap();
    assert_eq!(by.extras.abc(), (70.0, 95.0));
    assert_eq!(by.extras.offset(), 1);
    let text = toml::to_string(&by).unwrap();
    assert!(text.contains("abc_a") && !text.contains("offset"), "{text}");
}
