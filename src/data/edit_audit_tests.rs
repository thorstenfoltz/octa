use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn two_by_two() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "integer".into(),
        },
        ColumnInfo {
            name: "city".into(),
            data_type: "text".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::Int(1), CellValue::String("Aachen".into())],
        vec![CellValue::Int(2), CellValue::String("Bonn".into())],
    ];
    t
}

#[test]
fn entries_are_sorted_and_carry_before_and_after() {
    let mut t = two_by_two();
    t.edits.insert((1, 1), CellValue::String("Koeln".into()));
    t.edits
        .insert((0, 1), CellValue::String("Duesseldorf".into()));

    let e = audit_entries(&t);
    assert_eq!(e.len(), 2);
    assert_eq!((e[0].row, e[0].col), (0, 1), "sorted by row then column");
    assert_eq!(e[0].column_name, "city");
    assert_eq!(e[0].before, "Aachen");
    assert_eq!(e[0].after, "Duesseldorf");
    assert_eq!((e[1].row, e[1].col), (1, 1));
    assert_eq!(e[1].before, "Bonn");
}

#[test]
fn an_unedited_table_has_an_empty_trail() {
    assert!(audit_entries(&two_by_two()).is_empty());
}

#[test]
fn an_edit_pointing_past_the_table_is_dropped_not_panicked_on() {
    let mut t = two_by_two();
    t.edits.insert((99, 1), CellValue::String("ghost".into()));
    assert!(
        audit_entries(&t).is_empty(),
        "stale edit indices are ignored"
    );
}
