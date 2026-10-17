use crate::data::*;

fn one_col(values: &[&str]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns.push(ColumnInfo {
        name: "a".into(),
        data_type: "Utf8".into(),
    });
    t.rows = values
        .iter()
        .map(|v| vec![CellValue::String(v.to_string())])
        .collect();
    t
}

#[test]
fn batch_undo_is_one_step() {
    let mut t = one_col(&["x", "y"]);
    let start = t.undo_stack.len();
    t.set(0, 0, CellValue::String("X".into()));
    t.set(1, 0, CellValue::String("Y".into()));
    t.coalesce_undo_since(start);
    assert_eq!(t.undo_stack.len(), start + 1);
    assert!(t.undo());
    assert_eq!(t.get(0, 0).unwrap().to_string(), "x");
    assert_eq!(t.get(1, 0).unwrap().to_string(), "y");
    assert!(t.redo());
    assert_eq!(t.get(0, 0).unwrap().to_string(), "X");
    assert_eq!(t.get(1, 0).unwrap().to_string(), "Y");
}

#[test]
fn coalesce_noop_for_zero_or_one() {
    let mut t = one_col(&["x"]);
    let start = t.undo_stack.len();
    t.coalesce_undo_since(start);
    assert_eq!(t.undo_stack.len(), start);
    t.set(0, 0, CellValue::String("Z".into()));
    let one = t.undo_stack.len();
    t.coalesce_undo_since(one - 1);
    assert_eq!(t.undo_stack.len(), one);
}

#[test]
fn rename_column_undo_redo_round_trip() {
    let mut t = one_col(&["x"]);
    t.rename_column(0, "renamed".into());
    assert_eq!(t.columns[0].name, "renamed");
    assert!(t.structural_changes);
    assert!(t.undo());
    assert_eq!(t.columns[0].name, "a");
    assert!(t.redo());
    assert_eq!(t.columns[0].name, "renamed");
}

#[test]
fn rename_column_noop_when_unchanged() {
    let mut t = one_col(&["x"]);
    let start = t.undo_stack.len();
    t.rename_column(0, "a".into());
    assert_eq!(t.undo_stack.len(), start, "no-op rename must not push undo");
}

#[test]
fn bulk_rename_undoes_as_one_step() {
    let mut t = DataTable::empty();
    for n in ["a", "b"] {
        t.columns.push(ColumnInfo {
            name: n.into(),
            data_type: "Utf8".into(),
        });
    }
    let start = t.undo_stack.len();
    t.rename_column(0, "x".into());
    t.rename_column(1, "y".into());
    t.coalesce_undo_since(start);
    assert_eq!(t.undo_stack.len(), start + 1);
    assert!(t.undo());
    assert_eq!(t.columns[0].name, "a");
    assert_eq!(t.columns[1].name, "b");
    assert!(t.redo());
    assert_eq!(t.columns[0].name, "x");
    assert_eq!(t.columns[1].name, "y");
}
