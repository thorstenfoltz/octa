//! The relationship map's Measure on the server: exact distinct and
//! orphan counts, both ways round, for every drawn edge.
//!
//! Replaces reading a 10,000-row sample of each table. On Redshift,
//! Snowflake, Databricks and BigQuery, which accept a foreign key without
//! enforcing it, this is the only way to know a declared key holds.

use std::sync::atomic::AtomicBool;

use super::ServerSource;
use super::join_keys::{ColPair, pair_counts};
use crate::data::join_keys::score_counts;
use crate::data::rel_map::{RelMap, apply_pair};
use crate::db::{DbConnection, DbConnector, DbEngine};

/// Each node's `FROM` target. Node names are `schema.table`.
pub fn node_froms(conn: &DbConnection, catalog: Option<&str>, map: &RelMap) -> Vec<String> {
    map.nodes
        .iter()
        .map(|n| {
            let (schema, table) = n.name.split_once('.').unwrap_or(("", n.name.as_str()));
            ServerSource {
                conn: conn.clone(),
                catalog: catalog.map(str::to_string),
                schema: schema.into(),
                table: table.into(),
                filter: None,
                derived: Vec::new(),
            }
            .from_sql()
        })
        .collect()
}

/// Put exact numbers on every edge of `map`, as `score_edges` would over
/// every row.
pub fn run(
    c: &mut dyn DbConnector,
    engine: DbEngine,
    froms: &[String],
    map: &mut RelMap,
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    let names: Vec<Vec<String>> = map.nodes.iter().map(|n| n.columns.clone()).collect();
    let pairs: Vec<ColPair> = map
        .edges
        .iter()
        .map(|e| ((e.left_table, e.left_col), (e.right_table, e.right_col)))
        .collect();
    let counts = pair_counts(c, engine, froms, &names, &pairs, cancel)?;
    for (e, n) in map.edges.iter_mut().zip(counts) {
        apply_pair(
            e,
            n.lv,
            n.rv,
            score_counts(n.lv, n.ls, n.rv, n.rs, n.shared),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
}
