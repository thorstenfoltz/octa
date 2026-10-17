//! Unit tests for [`duckdb`](super). Fixtures are DuckDB's real output.

use super::*;
use crate::sql::explain::StepKind::*;

fn join_steps() -> Vec<PlanStep> {
    parse(include_str!("fixtures/duckdb_join.json")).expect("fixture parses")
}

#[test]
fn a_join_plan_reads_as_five_plain_steps() {
    let steps = join_steps();
    let got: Vec<(usize, StepKind, Option<&str>)> = steps
        .iter()
        .map(|s| (s.depth, s.kind, s.table.as_deref()))
        .collect();
    assert_eq!(
        got,
        vec![
            (0, TopN, None),
            (1, Group, None),
            (2, JoinHash, None),
            (3, ReadTable, Some("orders")),
            (3, ReadTable, Some("customers")),
        ]
    );
}

#[test]
fn steps_carry_rows_and_details_from_extra_info() {
    let steps = join_steps();
    assert_eq!(steps[0].detail, "Top: 5; Order By: sum(o.amount) DESC");
    assert_eq!(steps[2].estimated_rows, Some(222.0));
    assert_eq!(steps[2].detail, "Conditions: customer_id = id");
    assert_eq!(steps[3].detail, "Filters: amount>10.0");
    assert_eq!(steps[4].detail, "");
}

#[test]
fn projection_nodes_are_folded_away() {
    assert!(join_steps().iter().all(|s| s.server_name != "PROJECTION"));
}

#[test]
fn quoted_workspace_names_read_bare() {
    assert_eq!(short_table("\"temp\".main.\"data\""), "data");
    assert_eq!(short_table("memory.main.orders"), "orders");
    assert_eq!(short_table("orders"), "orders");
}

#[test]
fn text_that_is_not_a_plan_is_an_error() {
    assert!(parse("not json").is_err());
    assert!(parse("{\"name\": \"x\"}").is_err());
}

#[test]
fn reads_and_top_n_carry_their_fact() {
    let steps = join_steps();
    let facts: Vec<Option<&str>> = steps.iter().map(|s| s.fact.as_deref()).collect();
    // DuckDB names a group's columns by slot (`#0`), which says nothing.
    assert_eq!(
        facts,
        vec![
            Some("sum(o.amount) DESC"),
            None,
            None,
            Some("amount>10.0"),
            None,
        ]
    );
}

#[test]
fn a_workspace_qualified_sort_key_reads_bare() {
    let json = r#"[{"name":"TOP_N","children":[],"extra_info":{"Top":"3","Order By":"\"temp\".main.\"data\".b ASC"}}]"#;
    assert_eq!(parse(json).unwrap()[0].fact.as_deref(), Some("b ASC"));
}
