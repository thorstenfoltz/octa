//! Unit tests for [`explain`](super).

use super::*;

fn step(depth: usize, kind: StepKind, table: Option<&str>) -> PlanStep {
    PlanStep {
        depth,
        kind,
        server_name: format!("{kind:?}"),
        table: table.map(str::to_string),
        estimated_rows: Some(10.0),
        detail: String::new(),
        fact: None,
    }
}

fn sample() -> Plan {
    Plan {
        steps: vec![
            step(0, StepKind::JoinHash, None),
            step(1, StepKind::ReadTable, Some("orders")),
            step(1, StepKind::Other, Some("customers")),
        ],
        server_text: String::new(),
        bytes_scanned: None,
    }
}

#[test]
fn one_statement_passes_with_or_without_its_semicolon() {
    assert_eq!(single_statement("  SELECT 1;  ").unwrap(), "SELECT 1");
    assert_eq!(single_statement("SELECT 1").unwrap(), "SELECT 1");
}

#[test]
fn a_second_statement_is_refused() {
    assert!(single_statement("SELECT 1; SELECT 2").is_err());
}

#[test]
fn a_semicolon_in_a_string_or_comment_still_counts() {
    // MySQL lexes these differently from a standard scanner, and its
    // connector runs every statement, so none may pass.
    for q in [
        "SELECT 1 --1; DROP TABLE t",
        "SELECT price--discount AS net FROM t;\nDELETE FROM t",
        "SELECT * FROM orders # today's orders\n;\nDELETE FROM orders WHERE id = 5",
        "SELECT 'a\\'' ; DROP TABLE t; -- '",
        "SELECT ';' FROM t",
        "SELECT 1 /* x; y */",
    ] {
        assert!(single_statement(q).is_err(), "{q}");
    }
}

#[test]
fn a_hash_does_not_hide_a_second_statement() {
    assert!(single_statement("SELECT * FROM data ORDER BY #1; DELETE FROM data").is_err());
    assert!(single_statement("SELECT * FROM data ORDER BY #1").is_ok());
}

#[test]
fn an_empty_query_is_refused() {
    assert!(single_statement(" ; ").is_err());
}

#[test]
fn plain_words_and_table_name_the_same_steps() {
    // The plain view leaves out the unnamed step; the table keeps it.
    let plan = sample();
    let plain = plain_text(&plan);
    let table = steps_table(&plan);
    assert_eq!(table.row_count(), 3);
    assert_eq!(plain.lines().count(), 2, "{plain}");
    for (line, label) in plain.lines().zip(["Read whole table", "Join (hash)"]) {
        assert!(line[3..].starts_with(label), "{line} vs {label}");
    }
    let named: Vec<String> = table
        .rows
        .iter()
        .map(|r| r[1].to_string())
        .filter(|l| l != "Other")
        .collect();
    assert_eq!(named, ["Join (hash)", "Read whole table"]);
}

#[test]
fn a_dry_run_plan_says_how_much_it_would_scan_and_nothing_about_money() {
    let plan = Plan {
        bytes_scanned: Some(3 * 1024 * 1024 * 1024),
        ..Plan::default()
    };
    let plain = plain_text(&plan);
    assert_eq!(plain, "Would scan 3.0 GB\n");
    for symbol in ['$', '\u{20ac}', '\u{a3}'] {
        assert!(!plain.contains(symbol));
    }
    let table = steps_table(&plan);
    assert_eq!(table.columns[0].name, "bytes_scanned");
}

#[test]
fn box_drawing_becomes_ascii() {
    let boxed = "\u{250c}\u{2500}\u{2510}\n\u{2502}x\u{2502}\n\u{2514}\u{2500}\u{2518}";
    assert_eq!(ascii_server_text(boxed), "+-+\n|x|\n+-+");
}

#[test]
fn detail_joins_present_keys_in_order() {
    let v = serde_json::json!({"Filters": "a>1", "Order By": ["x", "y"], "Other": "z"});
    assert_eq!(
        detail(&v, &["Order By", "Filters", "Missing"]),
        "Order By: x, y; Filters: a>1"
    );
}

#[test]
fn only_the_engines_with_a_parser_can_explain() {
    use crate::db::DbEngine;
    for &engine in DbEngine::ALL {
        let expected = matches!(
            engine,
            DbEngine::Postgres | DbEngine::MySql | DbEngine::BigQuery | DbEngine::Databricks
        );
        assert_eq!(can_explain(engine), expected, "{engine:?}");
    }
}

#[test]
fn plain_words_number_the_steps_in_the_order_they_happen() {
    let plan = Plan {
        steps: postgres::parse(include_str!("fixtures/postgres_join.json")).unwrap(),
        ..Plan::default()
    };
    let text = plain_text(&plan);
    // Postgres prints the result first; a reader starts at the tables. Each
    // step names what sets it apart, in the query's own terms.
    assert_eq!(
        text,
        "1. Read whole table: orders, keeps rows where amount > '10'::numeric, ~993 rows\n\
         2. Read whole table: customers, ~50 rows\n\
         3. Join (hash), uses steps 1 and 2, ~993 rows\n\
         4. Group: by c.name, ~50 rows\n\
         5. Sort: by (sum(o.amount)) DESC, ~50 rows\n\
         6. Limit, ~5 rows\n"
    );
    // No server wording: that is what Server's own is for.
    assert!(!text.contains("Hash Cond") && !text.contains('['), "{text}");
}

#[test]
fn plain_words_leave_out_steps_without_a_plain_name() {
    // Spark wraps the plan in exchanges and projections nobody asked about.
    let plan = Plan {
        steps: databricks::parse(include_str!("fixtures/spark_union.txt")).unwrap(),
        ..Plan::default()
    };
    let text = plain_text(&plan);
    assert!(
        !text.contains("Exchange") && !text.contains("AdaptiveSparkPlan"),
        "{text}"
    );
    assert!(text.contains("3. Union, uses steps 1 and 2"), "{text}");
}

#[test]
fn plain_words_keep_a_plan_made_only_of_unnamed_steps() {
    let plan = Plan {
        steps: vec![
            step(0, StepKind::Other, None),
            step(1, StepKind::Other, None),
        ],
        ..Plan::default()
    };
    assert_eq!(plain_text(&plan).lines().count(), 2);
}

#[test]
fn a_condition_reads_as_its_parts_joined_by_and() {
    assert_eq!(
        condition(["((isnotnull(amount) AND (amount > 1.0)) AND isnotnull(customer_id))"]),
        Some("amount > 1.0 AND customer_id IS NOT NULL".to_string())
    );
    // MySQL spells it in lower case.
    assert_eq!(
        condition(["((o.amount > 10.00) and (o.customer_id is not null))"]),
        Some("o.amount > 10.00 AND o.customer_id IS NOT NULL".to_string())
    );
}

#[test]
fn a_not_null_test_goes_only_when_another_part_names_that_column() {
    // `id` is not named by `customer_id = 5`.
    assert_eq!(
        condition(["isnotnull(id)", "(customer_id = 5)"]),
        Some("id IS NOT NULL AND customer_id = 5".to_string())
    );
    assert_eq!(
        condition(["isnotnull(id)", "(id = 5)"]),
        Some("id = 5".to_string())
    );
}

#[test]
fn an_and_inside_text_or_brackets_does_not_split() {
    assert_eq!(
        condition(["(name = 'Tom AND Jerry')"]),
        Some("name = 'Tom AND Jerry'".to_string())
    );
    assert_eq!(
        condition(["(a > 1) OR (b AND c)"]),
        Some("(a > 1) OR (b AND c)".to_string())
    );
}

#[test]
fn duckdbs_runtime_filters_and_empty_lists_add_nothing() {
    assert_eq!(
        condition(["a>1", "b!='z' AND optional: Dynamic Filter (b)"]),
        Some("a>1 AND b!='z'".to_string())
    );
    assert_eq!(condition([""]), None);
    assert_eq!(condition([]), None);
}

#[test]
fn a_limit_count_is_the_leading_number() {
    assert_eq!(leading_number("10, 0").as_deref(), Some("10"));
    assert_eq!(leading_number("5 row(s)").as_deref(), Some("5"));
    assert_eq!(leading_number("all"), None);
}
