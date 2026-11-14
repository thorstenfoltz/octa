use super::*;

fn events(rows: &[(&str, &str, &str)]) -> DataTable {
    DataTable {
        columns: ["who", "event", "when"]
            .iter()
            .map(|n| ColumnInfo {
                name: (*n).into(),
                data_type: "Utf8".into(),
            })
            .collect(),
        rows: rows
            .iter()
            .map(|(w, e, t)| {
                vec![
                    CellValue::String((*w).into()),
                    CellValue::String((*e).into()),
                    CellValue::String((*t).into()),
                ]
            })
            .collect(),
        ..DataTable::empty()
    }
}

fn spec(steps: &[&str], when: bool, within_days: Option<u32>) -> FunnelSpec {
    FunnelSpec {
        who: 0,
        event: 1,
        when: when.then_some(2),
        steps: steps.iter().map(|s| (*s).to_string()).collect(),
        within_days,
    }
}

fn shop() -> DataTable {
    events(&[
        // a goes all the way, in order.
        ("a", "view", "2024-01-01 10:00:00"),
        ("a", "cart", "2024-01-01 10:05:00"),
        ("a", "pay", "2024-01-02 09:00:00"),
        // b pays before the cart: the cart counts, the payment does not.
        ("b", "view", "2024-01-01 11:00:00"),
        ("b", "pay", "2024-01-01 11:01:00"),
        ("b", "cart", "2024-01-01 11:02:00"),
        // c only looks, twice.
        ("c", "view", "2024-01-03 08:00:00"),
        ("c", "view", "2024-01-04 08:00:00"),
        // d never viewed, so the cart is not part of a funnel.
        ("d", "cart", "2024-01-01 12:00:00"),
        // No who, and no readable time.
        ("", "view", "2024-01-01 12:00:00"),
        ("e", "view", "soon"),
    ])
}

#[test]
fn people_are_counted_per_step_in_order() {
    let c = collect(&shop(), &spec(&["view", "cart", "pay"], true, None)).unwrap();
    assert_eq!(c.users, [3, 2, 1]);
    assert_eq!(c.skipped, 2);
}

#[test]
fn the_order_follows_the_time_not_the_rows() {
    let t = events(&[("a", "pay", "2024-01-02"), ("a", "view", "2024-01-01")]);
    let by_time = collect(&t, &spec(&["view", "pay"], true, None)).unwrap();
    assert_eq!(by_time.users, [1, 1]);
    let by_row = collect(&t, &spec(&["view", "pay"], false, None)).unwrap();
    assert_eq!(by_row.users, [1, 0]);
    assert_eq!(by_row.skipped, 0, "without When no time has to read");
}

#[test]
fn a_step_at_the_same_moment_counts() {
    let t = events(&[
        ("a", "cart", "2024-01-01 10:00:00"),
        ("a", "view", "2024-01-01 10:00:00"),
    ]);
    let c = collect(&t, &spec(&["view", "cart"], true, None)).unwrap();
    assert_eq!(c.users, [1, 1]);
}

#[test]
fn within_days_counts_from_the_first_step() {
    let steps = ["view", "cart", "pay"];
    // a pays 23 hours after the first view: inside one day.
    let one = collect(&shop(), &spec(&steps, true, Some(1))).unwrap();
    assert_eq!(one.users, [3, 2, 1]);
    let t = events(&[
        ("a", "view", "2024-01-01"),
        ("a", "cart", "2024-01-02"),
        ("a", "pay", "2024-01-04"),
    ]);
    assert_eq!(
        collect(&t, &spec(&steps, true, Some(2))).unwrap().users,
        [1, 1, 0]
    );
    assert_eq!(
        collect(&t, &spec(&steps, true, Some(3))).unwrap().users,
        [1, 1, 1]
    );
}

#[test]
fn bad_requests_are_refused() {
    let t = shop();
    assert_eq!(
        collect(&t, &spec(&["view"], true, None)),
        Err(FunnelError::TooFewSteps)
    );
    assert_eq!(
        collect(&t, &spec(&["view", "cart", "view"], true, None)),
        Err(FunnelError::RepeatedStep)
    );
    assert_eq!(
        collect(&t, &spec(&["view", "cart"], false, Some(3))),
        Err(FunnelError::WithinNeedsWhen)
    );
}

#[test]
fn the_table_has_shares_and_drops() {
    let out = table(&Counts {
        steps: vec!["view".into(), "cart".into(), "pay".into()],
        users: vec![3, 2, 1],
        skipped: 0,
    });
    let names: Vec<&str> = out.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, COLUMNS);
    assert_eq!(
        out.rows[0],
        [
            CellValue::String("view".into()),
            CellValue::Int(3),
            CellValue::Float(100.0),
            CellValue::Float(100.0),
            CellValue::Int(0),
        ]
    );
    assert_eq!(
        out.rows[2][1..],
        [
            CellValue::Int(1),
            CellValue::Float(33.3),
            CellValue::Float(50.0),
            CellValue::Int(1),
        ]
    );
}

#[test]
fn nobody_at_a_step_leaves_the_shares_empty() {
    let out = table(&Counts {
        steps: vec!["a".into(), "b".into(), "c".into()],
        users: vec![0, 0, 0],
        skipped: 0,
    });
    assert_eq!(out.rows[1][2], CellValue::Null);
    assert_eq!(out.rows[2][3], CellValue::Null);
}

#[test]
fn event_values_come_in_first_seen_order_and_stop_at_the_cap() {
    let t = shop();
    assert_eq!(
        event_values(&t, 1, 10),
        (vec!["view".to_string(), "cart".into(), "pay".into()], false)
    );
    assert_eq!(
        event_values(&t, 1, 2),
        (vec!["view".to_string(), "cart".into()], true)
    );
    assert_eq!(steps_from(["a", " b ", ""]), ["a", "b"]);
}
