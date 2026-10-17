//! Unit tests for [`databricks`](super). The `spark_*` fixtures are Apache
//! Spark 3.5.3's real `EXPLAIN` output (`apache/spark:3.5.3`, spark-sql).

use super::*;
use crate::sql::explain::StepKind::*;

#[test]
fn a_join_plan_reads_its_tree_by_the_three_character_levels() {
    let steps = parse(include_str!("fixtures/spark_join.txt")).unwrap();
    let got: Vec<(usize, StepKind, &str, Option<&str>)> = steps
        .iter()
        .map(|s| (s.depth, s.kind, s.server_name.as_str(), s.table.as_deref()))
        .collect();
    assert_eq!(
        got,
        vec![
            (0, Other, "AdaptiveSparkPlan", None),
            (1, TopN, "TakeOrderedAndProject", None),
            (2, Group, "HashAggregate", None),
            (3, Other, "Exchange", None),
            (4, Group, "HashAggregate", None),
            (5, Other, "Project", None),
            (6, JoinHash, "BroadcastHashJoin", None),
            (7, Filter, "Filter", None),
            (
                8,
                ReadTable,
                "FileScan",
                Some("spark_catalog.default.orders")
            ),
            (7, Other, "BroadcastExchange", None),
            (8, Filter, "Filter", None),
            (
                9,
                ReadTable,
                "FileScan",
                Some("spark_catalog.default.customers")
            ),
        ]
    );
    // Expression ids are dropped, the outer parentheses too.
    assert_eq!(steps[2].detail, "keys=[name], functions=[sum(amount)]");
    assert!(steps[8].detail.starts_with("Batched: true, DataFilters:"));
    assert!(steps.iter().all(|s| s.estimated_rows.is_none()));
}

#[test]
fn a_union_keeps_both_branches_on_one_level() {
    let steps = parse(include_str!("fixtures/spark_union.txt")).unwrap();
    let got: Vec<(usize, StepKind)> = steps.iter().map(|s| (s.depth, s.kind)).collect();
    assert_eq!(
        got,
        vec![
            (0, Other),
            (1, Group),
            (2, Other),
            (3, Group),
            (4, Union),
            (5, ReadTable),
            (5, ReadTable),
        ]
    );
}

#[test]
fn a_planning_error_in_the_plan_text_is_an_error_not_a_step() {
    let err = parse(include_str!("fixtures/spark_missing.txt"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("TABLE_OR_VIEW_NOT_FOUND"), "{err}");
    assert!(!err.contains("UnresolvedRelation"), "{err}");
}

#[test]
fn photon_operators_and_codegen_marks_are_named_like_sparks() {
    let text = "== Physical Plan ==\n\
                *(1) PhotonGroupingAgg(keys=[k#1], functions=[])\n\
                +- PhotonScan parquet main.sales.orders[k#1] DataFilters: []\n";
    let steps = parse(text).unwrap();
    assert_eq!(steps[0].kind, Group);
    assert_eq!(steps[0].server_name, "PhotonGroupingAgg");
    assert_eq!(steps[1].kind, ReadTable);
    assert_eq!(steps[1].table.as_deref(), Some("main.sales.orders"));
}

#[test]
fn an_empty_answer_is_an_error() {
    assert!(parse("").is_err());
}

fn plain(text: &str) -> String {
    crate::sql::explain::plain_text(&Plan {
        steps: parse(text).unwrap(),
        ..Plan::default()
    })
}

#[test]
fn a_filter_photon_folds_into_the_read_is_in_plain_words() {
    // Real Databricks SQL warehouse output (Photon), storage path redacted.
    // Two queries of the same shape used to read the same, so a new Explain
    // looked like the old plan shown again.
    assert_eq!(
        plain(include_str!("fixtures/photon_filter_limit.txt")),
        "1. Read whole table: samples.nyctaxi.trips, keeps rows where trip_distance > 5.0\n\
         2. Limit: first 10 rows\n"
    );
    assert_eq!(
        plain(include_str!("fixtures/photon_project_limit.txt")),
        "1. Read whole table: samples.nyctaxi.trips, keeps rows where pickup_zip = 10001\n\
         2. Limit: first 3 rows\n"
    );
}

#[test]
fn groups_and_sorts_say_what_they_go_by() {
    let steps = parse(include_str!("fixtures/photon_group_topk.txt")).unwrap();
    let facts: Vec<(StepKind, Option<&str>)> = steps
        .iter()
        .filter(|s| s.kind != Other)
        .map(|s| (s.kind, s.fact.as_deref()))
        .collect();
    assert_eq!(
        facts,
        vec![
            (TopN, Some("avg(fare_amount) DESC NULLS LAST")),
            (TopN, Some("avg(fare_amount) DESC NULLS LAST")),
            (Group, Some("pickup_zip")),
            (Group, Some("pickup_zip")),
            (ReadTable, Some("fare_amount < 3.0")),
        ]
    );
}

#[test]
fn sparks_own_filter_step_carries_the_condition_not_the_read_below_it() {
    let steps = parse(include_str!("fixtures/spark_join.txt")).unwrap();
    let fact = |i: usize| steps[i].fact.as_deref();
    assert_eq!(fact(1), Some("total DESC NULLS LAST"));
    assert_eq!(fact(2), Some("name"));
    // The not-null test on the compared column is Spark's own addition.
    assert_eq!(fact(7), Some("amount > 1.0 AND customer_id IS NOT NULL"));
    // The read under a Filter step only skips files; the Filter keeps rows.
    assert_eq!(fact(8), None);
    assert_eq!(fact(10), Some("id IS NOT NULL"));
    // A union of plain reads has nothing to add.
    let union = parse(include_str!("fixtures/spark_union.txt")).unwrap();
    assert!(union.iter().all(|s| s.kind == Group || s.fact.is_none()));
}

#[test]
fn a_value_spark_cut_short_ends_at_its_dots() {
    let text = "== Physical Plan ==\n\
                PhotonScan parquet c.s.t[a#1,b#2] DataFilters: [(a#1 > 5), (b#2 = 'a long tex..., \
                DictionaryFilters: [(a#1 > 5)], Format: parquet\n";
    let steps = parse(text).unwrap();
    // The cut-off part keeps its opening parenthesis: its partner is gone.
    assert_eq!(
        steps[0].fact.as_deref(),
        Some("a > 5 AND (b = 'a long tex...")
    );
}
