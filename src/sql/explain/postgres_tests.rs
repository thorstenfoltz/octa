//! Unit tests for [`postgres`](super). Fixtures are PostgreSQL 17's real output.

use super::*;
use crate::sql::explain::StepKind::*;

#[test]
fn a_join_plan_reads_limit_sort_group_join_and_two_reads() {
    let steps = parse(include_str!("fixtures/postgres_join.json")).unwrap();
    let got: Vec<(usize, StepKind, Option<&str>)> = steps
        .iter()
        .map(|s| (s.depth, s.kind, s.table.as_deref()))
        .collect();
    // The inner `Hash` node (the build side of the hash join) is folded away.
    assert_eq!(
        got,
        vec![
            (0, Limit, None),
            (1, Sort, None),
            (2, Group, None),
            (3, JoinHash, None),
            (4, ReadTable, Some("orders")),
            (4, ReadTable, Some("customers")),
        ]
    );
    assert_eq!(steps[1].detail, "Sort Key: (sum(o.amount)) DESC");
    assert_eq!(steps[3].detail, "Hash Cond: (o.customer_id = c.id)");
    assert_eq!(steps[4].estimated_rows, Some(993.0));
}

#[test]
fn an_index_lookup_names_the_index() {
    let steps = parse(include_str!("fixtures/postgres_index.json")).unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].kind, ReadIndex);
    assert_eq!(steps[0].table.as_deref(), Some("orders"));
    assert_eq!(
        steps[0].detail,
        "Index Name: orders_pkey; Index Cond: (id = 7)"
    );
}

#[test]
fn json_without_a_plan_is_an_error() {
    assert!(parse("[]").is_err());
}

#[test]
fn reads_groups_and_sorts_carry_their_fact() {
    let steps = parse(include_str!("fixtures/postgres_join.json")).unwrap();
    let facts: Vec<Option<&str>> = steps.iter().map(|s| s.fact.as_deref()).collect();
    assert_eq!(
        facts,
        vec![
            // Postgres prints no count for a Limit.
            None,
            Some("(sum(o.amount)) DESC"),
            Some("c.name"),
            None,
            Some("amount > '10'::numeric"),
            None,
        ]
    );
    let index = parse(include_str!("fixtures/postgres_index.json")).unwrap();
    assert_eq!(index[0].fact.as_deref(), Some("id = 7"));
}
