use super::super::test_support::DuckConn;
use super::*;
use crate::data::rel_map::{RelMapOptions, build_map, score_edges};
use crate::data::{CellValue, ColumnInfo, DataTable};

fn ints(name: &str, vals: impl Iterator<Item = i64>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: name.into(),
        data_type: "Int64".into(),
    }];
    t.rows = vals.map(|v| vec![CellValue::Int(v)]).collect();
    t
}

#[test]
fn the_server_measures_like_score_edges_over_every_row() {
    let orders = ints("customer_id", (1..=40).map(|i| 1 + i % 7));
    let customers = ints("id", 1..=5);
    let tables = vec![
        ("orders".to_string(), orders.clone()),
        ("customers".to_string(), customers.clone()),
    ];
    let mut want = build_map(
        &[("orders".into(), &orders), ("customers".into(), &customers)],
        &RelMapOptions {
            min_score: 0.0,
            ..Default::default()
        },
    );
    assert!(!want.edges.is_empty());
    let mut got = want.clone();
    score_edges(&tables, &mut want, usize::MAX);
    let mut c = DuckConn::with_tables(orders, vec![("customers", customers)]);
    let froms = vec!["\"data\"".to_string(), "\"customers\"".to_string()];
    run(
        &mut c,
        crate::db::DbEngine::Postgres,
        &froms,
        &mut got,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(got, want);
    // orders 1..7 vs customers 1..5: two child values have no parent.
    assert_eq!(got.edges[0].left_orphans, 2);
}

#[test]
fn node_froms_quote_schema_and_table() {
    let map = RelMap {
        nodes: vec![crate::data::rel_map::TableNode {
            name: "sales.orders".into(),
            columns: vec![],
            rows: 0,
        }],
        edges: vec![],
    };
    let conn = super::super::test_support::source().conn;
    assert_eq!(
        node_froms(&conn, None, &map),
        vec!["\"sales\".\"orders\"".to_string()]
    );
}
