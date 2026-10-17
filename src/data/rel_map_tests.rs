use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

#[test]
fn apply_pair_without_a_match_counts_every_value_as_an_orphan() {
    let mut e = Relationship {
        left_table: 0,
        left_col: 0,
        right_table: 1,
        right_col: 0,
        score: 1.0,
        overlap: 1.0,
        left_distinct: 1.0,
        right_distinct: 1.0,
        left_orphans: 0,
        left_distinct_values: 0,
        right_orphans: 0,
        right_distinct_values: 0,
        constraint: None,
        scored: false,
    };
    apply_pair(&mut e, 3, 5, None);
    assert_eq!((e.score, e.left_orphans, e.right_orphans), (0.0, 3, 5));
    assert!(e.scored);
}

fn tbl(col: &str, vals: &[&str]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: col.into(),
        data_type: "Utf8".into(),
    }];
    t.rows = vals
        .iter()
        .map(|v| vec![CellValue::String((*v).into())])
        .collect();
    t
}

#[test]
fn finds_a_perfect_key_with_no_orphans() {
    let a = tbl("customer_id", &["1", "2", "3"]);
    let b = tbl("id", &["1", "2", "3"]);
    let map = build_map(
        &[("orders".into(), &a), ("customers".into(), &b)],
        &RelMapOptions::default(),
    );
    assert_eq!(map.nodes.len(), 2);
    let edge = map.edges.first().expect("no relationship found");
    assert_eq!(edge.left_orphans, 0);
    assert!(edge.overlap > 0.99);
}

#[test]
fn counts_orphans_on_the_left() {
    let a = tbl("customer_id", &["1", "2", "9"]);
    let b = tbl("id", &["1", "2", "3"]);
    let map = build_map(
        &[("orders".into(), &a), ("customers".into(), &b)],
        &RelMapOptions::default(),
    );
    let edge = map
        .edges
        .iter()
        .find(|e| e.left_table == 0 && e.right_table == 1)
        .expect("no orders -> customers relationship");
    assert_eq!(edge.left_orphans, 1, "value 9 has no partner");
    assert_eq!(edge.left_distinct_values, 3);
    assert_eq!(edge.matched(), 2);
}

#[test]
fn unrelated_columns_produce_no_edge() {
    let a = tbl("colour", &["red", "green"]);
    let b = tbl("id", &["1", "2"]);
    let map = build_map(
        &[("a".into(), &a), ("b".into(), &b)],
        &RelMapOptions::default(),
    );
    assert!(map.edges.is_empty(), "got {:?}", map.edges);
}

#[test]
fn nodes_carry_names_columns_and_row_counts() {
    let a = tbl("x", &["1", "2"]);
    let map = build_map(&[("only".into(), &a)], &RelMapOptions::default());
    assert_eq!(map.nodes[0].name, "only");
    assert_eq!(map.nodes[0].columns, vec!["x".to_string()]);
    assert_eq!(map.nodes[0].rows, 2);
    assert!(map.edges.is_empty(), "a single table has no pairs");
}
