//! Unit tests for [`lookups`](super). Included via `#[path]`.

use super::*;
use std::sync::atomic::AtomicBool;

/// Orders with customer details copied onto every row. Customer 2 has two
/// spellings of its name on one of its three rows.
fn orders() -> DataTable {
    let s = |v: &str| CellValue::String(v.into());
    let rows = [
        ("1", "c1", "Acme", "Berlin", "10"),
        ("2", "c1", "Acme", "Berlin", "20"),
        ("3", "c2", "Mueller", "Munich", "30"),
        ("4", "c2", "Mueller", "Munich", "40"),
        ("5", "c2", "Muller", "Munich", "50"),
        ("6", "c3", "Zeta", "Hamburg", "60"),
        ("7", "c3", "Zeta", "Hamburg", "70"),
        ("8", "c1", "Acme", "Berlin", "80"),
    ];
    let mut t = DataTable::empty();
    t.columns = ["order", "customer", "name", "city", "amount"]
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).into(),
            data_type: "Utf8".into(),
        })
        .collect();
    t.rows = rows
        .iter()
        .map(|r| vec![s(r.0), s(r.1), s(r.2), s(r.3), s(r.4)])
        .collect();
    t
}

#[test]
fn finds_the_customer_lookup_with_its_one_breaking_row() {
    let found = find_lookups(&orders(), 0.8, &AtomicBool::new(false));
    let customer = found
        .iter()
        .find(|f| f.key == 1)
        .expect("customer is a key");
    assert_eq!(customer.keys, 3);
    let city = customer.dependents.iter().find(|d| d.col == 3).unwrap();
    assert_eq!(city.consistency, 1.0);
    let name = customer.dependents.iter().find(|d| d.col == 2).unwrap();
    assert_eq!(name.breaking_rows, 1);
    assert_eq!(name.conflicting_keys, 1);
    assert!((name.consistency - 7.0 / 8.0).abs() < 1e-9);
    // Unique columns (order, amount) are never keys.
    assert!(found.iter().all(|f| f.key != 0 && f.key != 4));
}

#[test]
fn a_stricter_threshold_drops_the_inconsistent_dependent() {
    let found = find_lookups(&orders(), 0.95, &AtomicBool::new(false));
    let customer = found.iter().find(|f| f.key == 1).unwrap();
    assert!(customer.dependents.iter().all(|d| d.col != 2));
}

#[test]
fn breaking_rows_are_every_row_of_the_conflicting_key() {
    assert_eq!(breaking_rows(&orders(), 1, &[2]), vec![2, 3, 4]);
    assert!(breaking_rows(&orders(), 1, &[3]).is_empty());
}

#[test]
fn split_out_builds_lookup_and_slim_main_and_counts_resolved_keys() {
    let split = split_out(&orders(), 1, &[2, 3]);
    let names: Vec<&str> = split
        .lookup
        .columns
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["customer", "name", "city"]);
    assert_eq!(split.lookup.rows.len(), 3);
    assert_eq!(split.lookup.rows[1][1], CellValue::String("Mueller".into()));
    assert_eq!(split.resolved_keys, 1);
    let main: Vec<&str> = split.main.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(main, ["order", "customer", "amount"]);
    assert_eq!(split.main.rows.len(), 8);
}

#[test]
fn cancel_stops_the_scan() {
    assert!(find_lookups(&orders(), 0.8, &AtomicBool::new(true)).is_empty());
}

#[test]
fn dependent_from_counts_applies_the_budget() {
    // 100 covered rows at 95%: 5 breaking rows pass, 6 do not.
    let d = dependent_from_counts(3, 100, 5, 2, 0.95).expect("within budget");
    assert_eq!((d.col, d.breaking_rows, d.conflicting_keys), (3, 5, 2));
    assert!((d.consistency - 0.95).abs() < 1e-12);
    assert!(dependent_from_counts(3, 100, 6, 2, 0.95).is_none());
    assert!(dependent_from_counts(3, 0, 0, 0, 0.95).is_none());
}
