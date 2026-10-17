use super::*;
use octa::data::{CellValue, ColumnInfo};

fn three_edits() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "a".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "b".into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = vec![
        vec![
            CellValue::String("a0".into()),
            CellValue::String("b0".into()),
        ],
        vec![
            CellValue::String("a1".into()),
            CellValue::String("b1".into()),
        ],
    ];
    t.edits.insert((0, 0), CellValue::String("EDIT-00".into()));
    t.edits.insert((1, 0), CellValue::String("EDIT-10".into()));
    t.edits.insert((0, 1), CellValue::String("EDIT-01".into()));
    t
}

/// Reverting ONE edit reverts exactly one. Reported from real use as
/// "it reverts everything", so this pins the count and the survivors,
/// not just that the clicked one went.
#[test]
fn reverting_one_edit_leaves_the_others_alone() {
    let mut t = three_edits();
    assert_eq!(audit_entries(&t).len(), 3);

    revert_one(&mut t, 1, 0);

    let left = audit_entries(&t);
    assert_eq!(left.len(), 2, "exactly one edit went: {left:?}");
    assert_eq!(
        t.get(1, 0),
        Some(&CellValue::String("a1".into())),
        "the reverted cell is back to its original"
    );
    assert_eq!(
        t.get(0, 0),
        Some(&CellValue::String("EDIT-00".into())),
        "another edit in the same column survived"
    );
    assert_eq!(
        t.get(0, 1),
        Some(&CellValue::String("EDIT-01".into())),
        "another edit in the same row survived"
    );
}

/// Reverting every edit one at a time empties the trail, and does it
/// one at a time rather than all at once on the first click.
#[test]
fn reverting_each_in_turn_empties_the_trail_gradually() {
    let mut t = three_edits();
    let mut expected = 3;
    while let Some(entry) = audit_entries(&t).first().cloned() {
        revert_one(&mut t, entry.row, entry.col);
        expected -= 1;
        assert_eq!(audit_entries(&t).len(), expected, "one at a time");
    }
    assert_eq!(expected, 0);
}
