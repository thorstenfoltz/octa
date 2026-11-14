use super::*;

fn orders() -> DataTable {
    // customer, date, amount, order
    let rows = [
        ("ann", "2024-03-01", "10.10", "o1"),
        ("ann", "2024-03-01", "0.20", "o1"),
        ("ann", "2024-03-20", "5", "o2"),
        ("bob", "2024-01-05", "100", "o3"),
        ("cy", "2024-03-30", "", "o4"),
        ("", "2024-03-30", "9", "o5"),
        ("dee", "not a date", "9", "o6"),
        ("eve", "2024-02-10", "0.10", "o7"),
        ("eve", "2024-02-11", "0.20", "o8"),
        ("eve", "2024-02-12", "0.30", "o9"),
    ];
    let mut t = DataTable::empty();
    t.columns = ["customer", "date", "amount", "order"]
        .map(|n| ColumnInfo {
            name: n.into(),
            data_type: "Utf8".into(),
        })
        .to_vec();
    t.rows = rows
        .iter()
        .map(|(c, d, a, o)| {
            [c, d, a, o]
                .map(|v| {
                    if v.is_empty() {
                        CellValue::Null
                    } else {
                        CellValue::String((*v).to_string())
                    }
                })
                .to_vec()
        })
        .collect();
    t
}

fn spec(order: bool) -> RfmSpec {
    RfmSpec {
        customer: 0,
        date: 1,
        amount: 2,
        order: order.then_some(3),
        as_of: None,
    }
}

fn col(t: &DataTable, name: &str) -> Vec<String> {
    let c = t.column_index(name).unwrap();
    t.rows.iter().map(|r| r[c].to_string()).collect()
}

#[test]
fn as_of_leaves_later_purchases_out_and_a_row_without_an_order_number_is_an_order() {
    let mut t = orders();
    // ann's 5.00 on 2024-03-20 has no order number: an order of its own.
    t.rows[2][3] = CellValue::Null;
    let ann = |totals: &Totals| totals.customers.iter().find(|c| c.id == "ann").cloned();
    let all = collect(&t, &spec(true));
    assert_eq!(ann(&all).unwrap().orders, 2, "o1 and the unnumbered row");

    let mut snapshot = spec(true);
    snapshot.as_of = NaiveDate::from_ymd_opt(2024, 3, 10);
    let then = collect(&t, &snapshot);
    let a = ann(&then).unwrap();
    assert_eq!(a.last, NaiveDate::from_ymd_opt(2024, 3, 1).unwrap());
    assert_eq!((a.orders, a.amount.to_string()), (1, "10.3".to_string()));
    // cy bought only on 2024-03-30: not a customer yet.
    assert!(then.customers.iter().all(|c| c.id != "cy"));
    let (table, _) = table(&then, snapshot.as_of, Names::Seven).unwrap();
    assert!(
        col(&table, "days_since")
            .iter()
            .all(|d| !d.starts_with('-')),
        "no negative days"
    );
}

#[test]
fn a_mark_counts_the_values_strictly_below_and_ties_share_it() {
    let ten: Vec<f64> = (0..10).map(f64::from).collect();
    assert_eq!(marks(&ten), [1, 1, 2, 2, 3, 3, 4, 4, 5, 5]);
    // Four equal values all get the first one's mark.
    assert_eq!(marks(&[1.0, 1.0, 1.0, 1.0, 9.0]), [1, 1, 1, 1, 5]);
    assert_eq!(marks(&[7.0]), [1]);
}

#[test]
fn order_lines_are_one_order_and_rows_without_a_customer_or_date_are_skipped() {
    let t = orders();
    let by_order = collect(&t, &spec(true));
    let ann = &by_order.customers[0];
    assert_eq!(ann.id, "ann");
    assert_eq!(ann.orders, 2, "o1 has two lines");
    assert_eq!(ann.last, NaiveDate::from_ymd_opt(2024, 3, 20).unwrap());
    // The empty customer and the unreadable date.
    assert_eq!(by_order.skipped, 2);
    // Without an order column every row is an order.
    assert_eq!(collect(&t, &spec(false)).customers[0].orders, 3);
}

#[test]
fn amounts_add_exactly_and_an_empty_amount_is_zero() {
    let totals = collect(&orders(), &spec(true));
    let amount = |id: &str| {
        totals
            .customers
            .iter()
            .find(|c| c.id == id)
            .unwrap()
            .amount
            .to_string()
    };
    assert_eq!(amount("eve"), "0.6", "not 0.6000000000000001");
    assert_eq!(amount("ann"), "15.3");
    assert_eq!(amount("cy"), "0");
}

/// Worked out on paper, as of 2024-03-30 (the latest date). A mark is
/// 1 + 5 * (customers strictly below) / 4, recency on days negated.
///
/// | customer | days | orders | amount | R | F | M | FM (rounded up) | seven names     |
/// |----------|------|--------|--------|---|---|---|-----------------|-----------------|
/// | bob      | 85   | 1      | 100    | 1 | 1 | 4 | 3               | Slipping away   |
/// | ann      | 10   | 2      | 15.3   | 3 | 3 | 3 | 3               | Needs attention |
/// | eve      | 47   | 3      | 0.6    | 2 | 4 | 2 | 3               | Slipping away   |
/// | cy       | 0    | 1      | 0      | 4 | 1 | 1 | 1               | New             |
#[test]
fn the_marks_and_names_of_a_small_shop() {
    let totals = collect(&orders(), &spec(true));
    let (t, as_of) = table(&totals, None, Names::Seven).unwrap();
    assert_eq!(as_of, NaiveDate::from_ymd_opt(2024, 3, 30).unwrap());
    assert_eq!(
        t.columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        COLUMNS.map(|(n, _)| n)
    );
    // Largest amount first.
    assert_eq!(col(&t, "customer"), ["bob", "ann", "eve", "cy"]);
    assert_eq!(col(&t, "days_since"), ["85", "10", "47", "0"]);
    assert_eq!(col(&t, "orders"), ["1", "2", "3", "1"]);
    // Recency: values below (days negated) are 0, 2, 1, 3 of four.
    assert_eq!(col(&t, "recency"), ["1", "3", "2", "4"]);
    // Frequency: 1, 2, 3, 1 -> below 0, 2, 3, 0.
    assert_eq!(col(&t, "frequency"), ["1", "3", "4", "1"]);
    // Monetary: 100, 15.3, 0.6, 0 -> below 3, 2, 1, 0.
    assert_eq!(col(&t, "monetary"), ["4", "3", "2", "1"]);
    // FM rounded up: (1+4)/2 = 3, (3+3)/2 = 3, (4+2)/2 = 3, (1+1)/2 = 1.
    assert_eq!(
        col(&t, "segment"),
        ["Slipping away", "Needs attention", "Slipping away", "New"]
    );
    assert_eq!(t.columns[1].data_type, "Date32");
}

#[test]
fn as_of_moves_the_days_and_no_names_drops_the_segment() {
    let totals = collect(&orders(), &spec(true));
    let later = NaiveDate::from_ymd_opt(2024, 4, 9).unwrap();
    let (t, as_of) = table(&totals, Some(later), Names::None).unwrap();
    assert_eq!(as_of, later);
    assert_eq!(col(&t, "days_since"), ["95", "20", "57", "10"]);
    assert_eq!(t.col_count(), 8);
    assert!(t.column_index("segment").is_err());
}

#[test]
fn no_customer_with_a_date_is_an_error_in_words() {
    assert_eq!(
        table(&Totals::default(), None, Names::Seven).unwrap_err(),
        RfmError::NoRows
    );
    assert!(!crate::i18n::t(RfmError::NoRows.key()).contains("rfm."));
    // With an As of before every purchase it says so.
    let day = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
    let err = table(&Totals::default(), Some(day), Names::Seven).unwrap_err();
    assert_eq!(err, RfmError::NoneBy(day));
    assert!(err.message().contains("2020-01-01"), "{}", err.message());
}

#[test]
fn every_grid_cell_has_a_name_in_both_sets() {
    for (names, distinct) in [(Names::Seven, 7), (Names::Eleven, 11)] {
        let mut seen = HashSet::new();
        for r in 1..=5 {
            for fm in 1..=5 {
                seen.insert(names.name(r, fm).expect("a name"));
            }
        }
        assert_eq!(seen.len(), distinct, "{names:?}");
    }
    assert_eq!(Names::None.name(5, 5), None);
}

#[test]
fn every_name_is_explained() {
    let named: HashSet<&str> = [Names::Seven, Names::Eleven]
        .iter()
        .flat_map(|n| (1..=5).flat_map(move |r| (1..=5).filter_map(move |fm| n.name(r, fm))))
        .collect();
    let hinted: HashSet<&str> = VALUE_HINTS.iter().map(|(n, _)| *n).collect();
    assert_eq!(named, hinted);
    for (_, key) in VALUE_HINTS {
        assert!(!crate::i18n::t(key).starts_with("rfm."), "{key}");
    }
}

#[test]
fn the_names_words_read_both_ways() {
    for n in Names::ALL {
        assert_eq!(Names::parse(n.id()), Some(n));
    }
    assert_eq!(Names::parse(" Eleven "), Some(Names::Eleven));
    assert_eq!(Names::parse("twelve"), None);
}
