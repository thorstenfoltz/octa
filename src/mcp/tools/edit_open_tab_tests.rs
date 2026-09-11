use super::*;
use crate::mcp::tools::{TableSnapshot, ToolContext};
use octa::data::{CellValue, ColumnInfo, DataTable};
use std::sync::{Arc, Mutex};

fn ctx_with_tab(unlocked: bool) -> ToolContext {
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
    ];
    let mut ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false, 0);
    ctx.open_tabs = vec![TableSnapshot {
        handle: "#1".into(),
        display_name: "t".into(),
        source_path: None,
        table: t,
        window: None,
    }];
    ctx.active_tab = Some(0);
    ctx.allow_existing_writes = unlocked;
    ctx.pending_tab_edits = Some(Arc::new(Mutex::new(Vec::new())));
    ctx
}

#[test]
fn refuses_when_existing_writes_disallowed() {
    let ctx = ctx_with_tab(false);
    let p = Params {
        open_tab: "#1".into(),
        ops: vec![OpSpec::DeleteRows { rows: vec![0] }],
    };
    assert!(run(&ctx, &p).is_err());
    assert!(
        ctx.pending_tab_edits
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn queues_resolved_add_column() {
    let ctx = ctx_with_tab(true);
    let p = Params {
        open_tab: "@active".into(),
        ops: vec![OpSpec::AddColumn {
            name: "double".into(),
            expression: "v * 2".into(),
        }],
    };
    run(&ctx, &p).unwrap();
    let q = ctx.pending_tab_edits.as_ref().unwrap().lock().unwrap();
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].tab_handle, "#1");
    assert_eq!(q[0].snapshot_rows, 2);
    match &q[0].ops[0] {
        ResolvedOp::AddColumn { name, values, .. } => {
            assert_eq!(name, "double");
            assert_eq!(values.len(), 2);
        }
        _ => panic!("expected AddColumn"),
    }
}

#[test]
fn queues_drop_columns_resolved_by_name() {
    let ctx = ctx_with_tab(true);
    let p = Params {
        open_tab: "#1".into(),
        ops: vec![OpSpec::DropColumns {
            cols: vec![ColRef::Name("v".into())],
        }],
    };
    run(&ctx, &p).unwrap();
    let q = ctx.pending_tab_edits.as_ref().unwrap().lock().unwrap();
    match &q[0].ops[0] {
        ResolvedOp::DropColumns(idxs) => assert_eq!(idxs, &vec![1]),
        _ => panic!("expected DropColumns"),
    }
}

#[test]
fn refuses_dropping_every_column() {
    let ctx = ctx_with_tab(true);
    let p = Params {
        open_tab: "#1".into(),
        ops: vec![OpSpec::DropColumns {
            cols: vec![ColRef::Index(0), ColRef::Index(1)],
        }],
    };
    assert!(run(&ctx, &p).is_err());
}

#[test]
fn queues_sort_keys_resolved_by_name_and_direction() {
    let ctx = ctx_with_tab(true);
    let p = Params {
        open_tab: "#1".into(),
        ops: vec![OpSpec::SortRows {
            by: vec![
                SortSpec {
                    col: ColRef::Name("v".into()),
                    descending: true,
                },
                SortSpec {
                    col: ColRef::Index(0),
                    descending: false,
                },
            ],
        }],
    };
    run(&ctx, &p).unwrap();
    let q = ctx.pending_tab_edits.as_ref().unwrap().lock().unwrap();
    match &q[0].ops[0] {
        // `ascending` is the stored flag, so `descending: true` must invert.
        ResolvedOp::SortRows(keys) => assert_eq!(keys, &vec![(1, false), (0, true)]),
        other => panic!("expected SortRows, got {other:?}"),
    }
}

#[test]
fn sort_is_queued_after_row_ops_that_address_rows_by_index() {
    let ctx = ctx_with_tab(true);
    let p = Params {
        open_tab: "#1".into(),
        ops: vec![
            OpSpec::SortRows {
                by: vec![SortSpec {
                    col: ColRef::Index(1),
                    descending: false,
                }],
            },
            OpSpec::DeleteRows { rows: vec![0] },
        ],
    };
    run(&ctx, &p).unwrap();
    let q = ctx.pending_tab_edits.as_ref().unwrap().lock().unwrap();
    assert!(matches!(q[0].ops[0], ResolvedOp::DeleteRows(_)));
    assert!(matches!(q[0].ops[1], ResolvedOp::SortRows(_)));
}

#[test]
fn sort_with_drop_columns_is_refused_rather_than_misapplied() {
    let ctx = ctx_with_tab(true);
    let p = Params {
        open_tab: "#1".into(),
        ops: vec![
            OpSpec::DropColumns {
                cols: vec![ColRef::Index(0)],
            },
            OpSpec::SortRows {
                by: vec![SortSpec {
                    col: ColRef::Index(1),
                    descending: false,
                }],
            },
        ],
    };
    assert!(run(&ctx, &p).is_err());
    assert!(
        ctx.pending_tab_edits
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_large_file_tab_is_refused_instead_of_edited_through_its_page() {
    let mut ctx = ctx_with_tab(true);
    ctx.open_tabs[0].window = Some(crate::mcp::tools::SnapshotWindow {
        offset: 0,
        len: 2,
        total: 9_000_000,
    });
    let p = Params {
        open_tab: "#1".into(),
        ops: vec![OpSpec::DeleteRows { rows: vec![0] }],
    };
    assert!(run(&ctx, &p).is_err());
}
