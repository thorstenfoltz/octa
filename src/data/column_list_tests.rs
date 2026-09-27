use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};
use std::collections::HashSet;

fn table_of(names: &[&str]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = names
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).to_string(),
            data_type: "text".into(),
        })
        .collect();
    t.rows = vec![names.iter().map(|_| CellValue::Null).collect()];
    t
}

#[test]
fn entries_carry_hidden_and_frozen_state() {
    let t = table_of(&["id", "name", "amount"]);
    let hidden: HashSet<usize> = [1].into_iter().collect();
    let e = column_entries(&t, &hidden, 1);
    assert_eq!(e.len(), 3);
    assert!(!e[0].hidden && e[0].frozen, "col 0 is frozen, not hidden");
    assert!(e[1].hidden && !e[1].frozen, "col 1 is hidden, not frozen");
    assert!(!e[2].hidden && !e[2].frozen);
}

#[test]
fn search_is_case_insensitive_and_matches_anywhere() {
    let t = table_of(&["customer_id", "Name", "amount"]);
    let e = column_entries(&t, &HashSet::new(), 0);
    let hits = filter_entries(&e, "AM");
    let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, vec!["Name", "amount"], "matches anywhere, any case");
    assert_eq!(filter_entries(&e, "").len(), 3, "empty query keeps all");
}

#[test]
fn moving_a_column_shifts_only_what_is_between() {
    // 0,1,2,3,4 with 1 moved to 3 becomes 0,2,3,1,4.
    assert_eq!(move_column(5, 1, 3), vec![0, 2, 3, 1, 4]);
    // Backwards: 3 moved to 1 becomes 0,3,1,2,4.
    assert_eq!(move_column(5, 3, 1), vec![0, 3, 1, 2, 4]);
    // No-op and out-of-range are identity, never a panic.
    assert_eq!(move_column(3, 1, 1), vec![0, 1, 2]);
    assert_eq!(move_column(3, 9, 1), vec![0, 1, 2]);
    // A drag that ends past the last row: `from` is always a real row, so it
    // is `to` that goes out of range. Without a bound on `to` this panics in
    // `Vec::insert`, which is exactly what the panel would do on a sloppy drop.
    assert_eq!(move_column(3, 1, 9), vec![0, 1, 2]);
}
