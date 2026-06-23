use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn sample() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "v".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::Int(1), CellValue::Int(10)],
        vec![CellValue::Int(2), CellValue::Int(20)],
        vec![CellValue::Int(3), CellValue::Int(30)],
    ];
    t
}

#[test]
fn add_column_running_total_is_row_aligned() {
    let mut t = sample();
    let ops = vec![EditOp::AddColumn {
        name: "running".into(),
        expression: "SUM(v) OVER (ORDER BY id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)"
            .into(),
    }];
    let s = apply_edit_ops(&mut t, &ops).unwrap();
    assert_eq!(s.columns_added, 1);
    let col = t.columns.iter().position(|c| c.name == "running").unwrap();
    let got: Vec<i64> = (0..t.row_count())
        .map(|r| match t.get(r, col).unwrap() {
            CellValue::Int(i) => *i,
            CellValue::Float(f) => *f as i64,
            _ => panic!(),
        })
        .collect();
    assert_eq!(got, vec![10, 30, 60]);
}

#[test]
fn mixed_batch_applies_in_canonical_order() {
    let mut t = sample();
    let ops = vec![
        EditOp::AddColumn {
            name: "double".into(),
            expression: "v * 2".into(),
        },
        EditOp::InsertRows {
            at: None,
            rows: vec![vec![CellValue::Int(4), CellValue::Int(40), CellValue::Null]],
        },
        EditOp::SetCells(vec![(0, EditColRef::Name("v".into()), CellValue::Int(99))]),
        EditOp::DeleteRows(vec![1]),
    ];
    let s = apply_edit_ops(&mut t, &ops).unwrap();
    assert_eq!(
        (
            s.columns_added,
            s.rows_inserted,
            s.cells_set,
            s.rows_deleted
        ),
        (1, 1, 1, 1)
    );
    assert!(t.columns.iter().any(|c| c.name == "double"));
    let v = t.columns.iter().position(|c| c.name == "v").unwrap();
    assert_eq!(t.get(0, v).unwrap(), &CellValue::Int(99));
    assert_eq!(t.row_count(), 3);
}

#[test]
fn drop_columns_applies_last_and_keeps_other_refs_valid() {
    // Add a column AND drop the original "v" by name in one batch: drop runs
    // last, so the add's expression still sees "v".
    let mut t = sample();
    let ops = vec![
        EditOp::AddColumn {
            name: "double".into(),
            expression: "v * 2".into(),
        },
        EditOp::DropColumns(vec![EditColRef::Name("v".into())]),
    ];
    let s = apply_edit_ops(&mut t, &ops).unwrap();
    assert_eq!(s.columns_added, 1);
    assert_eq!(s.columns_dropped, 1);
    assert!(t.columns.iter().all(|c| c.name != "v"));
    let d = t.columns.iter().position(|c| c.name == "double").unwrap();
    assert_eq!(t.get(0, d).unwrap(), &CellValue::Int(20));
}

#[test]
fn drop_multiple_columns_by_index_is_shift_safe() {
    let mut t = sample(); // columns: id, v
    let ops = vec![EditOp::DropColumns(vec![
        EditColRef::Index(0),
        EditColRef::Index(0), // duplicate of resolved 0, deduped
    ])];
    let s = apply_edit_ops(&mut t, &ops).unwrap();
    assert_eq!(s.columns_dropped, 1);
    assert_eq!(t.col_count(), 1);
    assert_eq!(t.columns[0].name, "v");
}

#[test]
fn cannot_drop_every_column() {
    let mut t = sample();
    let ops = vec![EditOp::DropColumns(vec![
        EditColRef::Index(0),
        EditColRef::Index(1),
    ])];
    assert!(apply_edit_ops(&mut t, &ops).is_err());
}
