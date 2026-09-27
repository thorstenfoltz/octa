use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn table_with(rows: usize, text: &str) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "c".into(),
        data_type: "Utf8".into(),
    }];
    t.rows = (0..rows)
        .map(|_| vec![CellValue::String(text.to_string())])
        .collect();
    t
}

#[test]
fn the_estimate_grows_with_rows_and_with_string_length() {
    let small = estimate_bytes(&table_with(10, "a"));
    let more_rows = estimate_bytes(&table_with(100, "a"));
    let longer = estimate_bytes(&table_with(10, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    assert!(more_rows > small, "{more_rows} should exceed {small}");
    assert!(longer > small, "{longer} should exceed {small}");
}

#[test]
fn an_empty_table_is_not_free_but_is_small() {
    let e = estimate_bytes(&DataTable::empty());
    assert!(e < 4096, "an empty table estimated at {e} bytes");
}

#[test]
fn the_overlay_maps_count_towards_the_estimate() {
    let mut t = table_with(10, "a");
    let before = estimate_bytes(&t);
    for row in 0..10 {
        t.edits.insert((row, 0), CellValue::String("edited".into()));
    }
    assert!(estimate_bytes(&t) > before, "pending edits hold memory too");
}

/// A column type conversion snapshots the column twice onto the undo stack.
/// A tab that looks idle can be holding those copies, which is exactly the
/// thing the dialog exists to reveal.
#[test]
fn an_undo_snapshot_counts_towards_the_estimate() {
    let mut t = table_with(50, "123");
    let before = estimate_bytes(&t);
    crate::data::retype::apply_retype(&mut t, 0, crate::data::retype::TargetType::Integer);
    assert!(!t.undo_stack.is_empty(), "the conversion was recorded");
    assert!(
        estimate_bytes(&t) > before,
        "a column conversion snapshots the column twice onto the undo stack, \
         so the tab is holding more than it was: {} then {}",
        before,
        estimate_bytes(&t)
    );
}

#[test]
fn the_format_is_human_readable() {
    assert_eq!(format_estimate(512), "512 B");
    assert_eq!(format_estimate(2048), "2.0 KB");
    assert_eq!(format_estimate(5 * 1024 * 1024), "5.0 MB");
    assert_eq!(format_estimate(3 * 1024 * 1024 * 1024), "3.0 GB");
}
