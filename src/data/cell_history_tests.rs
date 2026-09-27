//! Unit tests for [`cell_history`](super). Included via `#[path]`.

use super::*;
use crate::data::ColumnInfo;

fn t(rows: &[(&str, &str)]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = ["id", "price"]
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).into(),
            data_type: "Utf8".into(),
        })
        .collect();
    t.rows = rows
        .iter()
        .map(|(a, b)| {
            vec![
                CellValue::String((*a).into()),
                CellValue::String((*b).into()),
            ]
        })
        .collect();
    t
}

fn v(sha: &str, table: DataTable) -> Version {
    Version {
        commit: CommitInfo {
            sha: sha.into(),
            subject: sha.into(),
            ..Default::default()
        },
        table,
    }
}

fn key(value: &str, position: usize) -> RowRef {
    RowRef::Key {
        columns: vec!["id".into()],
        values: vec![value.into()],
        position,
    }
}

#[test]
fn follows_the_row_by_key_through_a_resort() {
    let versions = vec![
        v("c3", t(&[("2", "25"), ("1", "10")])), // re-sorted
        v("c2", t(&[("1", "10"), ("2", "25")])), // price raised
        v("c1", t(&[("1", "10"), ("2", "20")])),
    ];
    let h = cell_history(&versions, &key("2", 0), "price");
    let shas: Vec<&str> = h.entries.iter().map(|e| e.commit.sha.as_str()).collect();
    assert_eq!(shas, ["c2", "c1"], "the re-sort is not a change");
    assert_eq!(h.entries[0].state, CellState::Value("25".into()));
    assert_eq!(h.entries[0].previous, Some(CellState::Value("20".into())));
    assert_eq!(h.entries[1].previous, None);
    assert!(h.positional.is_empty());
}

#[test]
fn reports_row_added_and_column_absent() {
    let mut no_price = t(&[("1", "10")]);
    no_price.columns.truncate(1);
    no_price.rows = vec![vec![CellValue::String("1".into())]];
    let versions = vec![
        v("c3", t(&[("1", "10"), ("2", "20")])),
        v("c2", t(&[("1", "10")])),
        v("c1", no_price),
    ];
    let h = cell_history(&versions, &key("2", 1), "price");
    assert_eq!(h.entries[0].previous, Some(CellState::RowAbsent));
    assert_eq!(h.entries.last().unwrap().state, CellState::ColumnAbsent);
}

#[test]
fn a_non_unique_key_falls_back_to_the_position_and_says_so() {
    let versions = vec![
        v("c2", t(&[("1", "11"), ("1", "20")])),
        v("c1", t(&[("1", "10"), ("1", "20")])),
    ];
    let h = cell_history(&versions, &key("1", 0), "price");
    assert_eq!(h.positional.len(), 2);
    assert_eq!(h.entries[0].state, CellState::Value("11".into()));
}

#[test]
fn history_table_has_one_row_per_entry() {
    let versions = vec![v("c2", t(&[("1", "11")])), v("c1", t(&[("1", "10")]))];
    let h = cell_history(&versions, &RowRef::Position(0), "price");
    let out = history_table(&h);
    let names: Vec<&str> = out.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        ["commit", "date", "author", "subject", "value", "change"]
    );
    assert_eq!(out.rows.len(), 2);
}
