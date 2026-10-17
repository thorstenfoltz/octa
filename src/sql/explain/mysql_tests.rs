//! Unit tests for [`mysql`](super). Fixtures are MySQL 8.4's and MariaDB 11's
//! real output.

use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::db::{DbEngine, DbWriteMode, DbWriteReport};
use crate::sql::explain::StepKind::*;

/// Answers `EXPLAIN FORMAT=TREE` and `EXPLAIN FORMAT=JSON` from a script and
/// records every statement it was sent.
struct Fake {
    tree: Result<&'static str, &'static str>,
    json: Result<&'static str, &'static str>,
    sent: Vec<String>,
}

impl DbConnector for Fake {
    fn engine(&self) -> DbEngine {
        DbEngine::MySql
    }
    fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
        anyhow::bail!("unused")
    }
    fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
        anyhow::bail!("unused")
    }
    fn query(&mut self, sql: &str) -> anyhow::Result<DataTable> {
        self.sent.push(sql.to_string());
        let answer = if sql.starts_with("EXPLAIN FORMAT=TREE") {
            self.tree
        } else if sql.starts_with("EXPLAIN FORMAT=JSON") {
            self.json
        } else {
            anyhow::bail!("unexpected SQL: {sql}")
        };
        let text = answer.map_err(|e| anyhow::anyhow!(e))?;
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "EXPLAIN".into(),
            data_type: "Utf8".into(),
        }];
        t.rows = vec![vec![CellValue::String(text.to_string())]];
        Ok(t)
    }
    fn execute(&mut self, _: &str) -> anyhow::Result<u64> {
        anyhow::bail!("unused")
    }
    fn write_table(
        &mut self,
        _: Option<&str>,
        _: &str,
        _: &str,
        _: DbWriteMode,
        _: &DataTable,
    ) -> anyhow::Result<DbWriteReport> {
        anyhow::bail!("unused")
    }
}

const TREE: &str = include_str!("fixtures/mysql_join_tree.txt");
const MARIADB: &str = include_str!("fixtures/mariadb_join.json");
const NO_TREE: &str = "Unknown EXPLAIN/ANALYZE format name: 'TREE'";

fn fake(
    tree: Result<&'static str, &'static str>,
    json: Result<&'static str, &'static str>,
) -> Fake {
    Fake {
        tree,
        json,
        sent: vec![],
    }
}

#[test]
fn the_tree_format_keeps_every_step_and_its_depth() {
    let steps = parse_tree(include_str!("fixtures/mysql_join_tree.txt"));
    let got: Vec<(usize, StepKind)> = steps.iter().map(|s| (s.depth, s.kind)).collect();
    assert_eq!(
        got,
        vec![
            (0, Limit),
            (1, Sort),
            (2, Other), // Table scan on <temporary>: MySQL's own scratch table
            (3, Group),
            (4, JoinNestedLoop),
            (5, Filter),
            (6, ReadTable),
            (5, ReadIndex),
        ]
    );
    assert_eq!(steps[6].table.as_deref(), Some("o"));
    assert_eq!(steps[6].estimated_rows, Some(1000.0));
    assert_eq!(steps[7].table.as_deref(), Some("c"));
    assert_eq!(steps[7].detail, "PRIMARY (id=o.customer_id)");
    assert_eq!(
        steps[1].detail,
        "total DESC, limit input to 5 row(s) per chunk"
    );
}

#[test]
fn the_json_format_reads_tables_joins_and_sorts() {
    let steps = parse_json(include_str!("fixtures/mysql_join.json")).unwrap();
    let got: Vec<(usize, StepKind, Option<&str>)> = steps
        .iter()
        .map(|s| (s.depth, s.kind, s.table.as_deref()))
        .collect();
    assert_eq!(
        got,
        vec![
            (0, Sort, None),
            (1, Group, None),
            (2, JoinNestedLoop, None),
            (3, ReadTable, Some("o")),
            (3, ReadIndex, Some("c")),
        ]
    );
    assert_eq!(steps[3].estimated_rows, Some(1000.0));
}

#[test]
fn a_const_lookup_reads_by_index() {
    let steps = parse_json(include_str!("fixtures/mysql_index.json")).unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].kind, ReadIndex);
    assert_eq!(steps[0].detail, "key: PRIMARY");
}

#[test]
fn mariadb_json_reads_too() {
    let steps = parse_json(include_str!("fixtures/mariadb_join.json")).unwrap();
    assert_eq!(steps[0].kind, Sort);
    assert_eq!(steps[0].detail, "sort_key: sum(o.amount) desc");
    assert!(
        steps
            .iter()
            .any(|s| s.kind == ReadIndex && s.table.as_deref() == Some("o"))
    );
    assert!(
        steps
            .iter()
            .any(|s| s.kind == ReadTable && s.table.as_deref() == Some("c"))
    );
}

#[test]
fn a_tree_answer_is_used_and_json_is_never_asked() {
    let mut c = fake(Ok(TREE), Err("json must not be asked"));
    let plan = explain(&mut c, "select 1").unwrap();
    assert_eq!(plan.steps[0].kind, Limit);
    assert_eq!(c.sent.len(), 1);
}

#[test]
fn a_refused_tree_falls_back_to_json() {
    let mut c = fake(Err(NO_TREE), Ok(MARIADB));
    let plan = explain(&mut c, "select 1").unwrap();
    assert_eq!(plan.steps[0].kind, Sort);
}

#[test]
fn a_tree_answer_without_steps_falls_back_to_json() {
    let mut c = fake(Ok("nothing to see"), Ok(MARIADB));
    let plan = explain(&mut c, "select 1").unwrap();
    assert_eq!(plan.steps[0].kind, Sort);
}

#[test]
fn when_both_formats_fail_the_error_names_both() {
    let mut c = fake(Err("tree broke"), Err("json broke"));
    let e = explain(&mut c, "select 1").unwrap_err();
    let text = format!("{e:#}");
    assert!(
        text.contains("tree broke") && text.contains("json broke"),
        "{text}"
    );
}

#[test]
fn tree_steps_carry_their_fact() {
    let steps = parse_tree(TREE);
    let facts: Vec<Option<&str>> = steps.iter().map(|s| s.fact.as_deref()).collect();
    assert_eq!(
        facts,
        vec![
            Some("5"),
            Some("total DESC"),
            None,
            None,
            None,
            Some("o.amount > 10.00 AND o.customer_id IS NOT NULL"),
            None,
            Some("id=o.customer_id"),
        ]
    );
}

#[test]
fn json_tables_carry_their_condition() {
    let steps = parse_json(include_str!("fixtures/mysql_join.json")).unwrap();
    assert_eq!(
        steps[3].fact.as_deref(),
        Some("`app`.`o`.`amount` > 10.00 AND `app`.`o`.`customer_id` IS NOT NULL")
    );
    let maria = parse_json(MARIADB).unwrap();
    assert_eq!(maria[0].fact.as_deref(), Some("sum(o.amount) desc"));
}
