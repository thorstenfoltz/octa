use super::*;
use octa::data::{CellValue, ColumnInfo, DataTable};

fn edited_table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "city".into(),
        data_type: "text".into(),
    }];
    t.rows = vec![vec![CellValue::String("Aachen".into())]];
    t.edits.insert((0, 0), CellValue::String("Bonn".into()));
    t
}

/// Reverting one edit restores exactly that cell and leaves the rest of
/// the overlay alone.
#[test]
fn revert_one_removes_only_that_edit() {
    let mut t = edited_table();
    t.edits.insert((0, 0), CellValue::String("Bonn".into()));
    revert_one(&mut t, 0, 0);
    assert!(t.edits.is_empty(), "the reverted edit is gone");
    assert_eq!(
        t.get(0, 0).map(|c| c.to_string()).as_deref(),
        Some("Aachen")
    );
}

/// Read-only mode blocks the revert but never the list.
#[test]
fn readonly_blocks_revert_but_not_viewing() {
    let t = edited_table();
    assert_eq!(octa::data::edit_audit::audit_entries(&t).len(), 1);
    assert!(!revert_allowed(true), "read-only refuses the revert");
    assert!(revert_allowed(false));
}
