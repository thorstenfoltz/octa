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
#[path = "rel_measure_tests.rs"]
mod tests;
