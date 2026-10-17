//! DuckDB's `EXPLAIN (FORMAT JSON)`: an array of nodes
//! `{name, children, extra_info}`, where `extra_info` holds the table, the
//! filters and an "Estimated Cardinality" as text.

use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;
use serde_json::Value;

use super::{PlanStep, StepKind};

/// The steps of a DuckDB JSON plan. `PROJECTION` nodes, the column shuffles
/// DuckDB inserts between nearly every pair of real steps, are folded away:
/// their children move up a level.
pub fn parse(json: &str) -> Result<Vec<PlanStep>> {
    let v: Value =
        serde_json::from_str(json).context("DuckDB returned a plan Octa could not read")?;
    let roots = v
        .as_array()
        .context("DuckDB returned a plan Octa could not read")?;
    let mut out = Vec::new();
    for n in roots {
        walk(n, 0, &mut out);
    }
    Ok(out)
}

fn walk(n: &Value, depth: usize, out: &mut Vec<PlanStep>) {
    let name = n["name"].as_str().unwrap_or_default().trim();
    let children = n["children"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if name == "PROJECTION" {
        for c in children {
            walk(c, depth, out);
        }
        return;
    }
    let info = &n["extra_info"];
    let kind = kind_of(name);
    // `Filters` and `Order By` come as one string or a list of them.
    let texts = |field: &str| -> Vec<&str> {
        match &info[field] {
            Value::String(s) => vec![s.as_str()],
            Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        }
    };
    // A LIMIT prints no count, and a group names its columns by slot (`#0`),
    // which says nothing; neither gets a fact.
    let fact = match kind {
        StepKind::ReadTable | StepKind::ReadIndex => super::condition(texts("Filters")),
        StepKind::Filter => super::condition(texts("Expression")),
        StepKind::Sort | StepKind::TopN => {
            let keys = texts("Order By").join(", ");
            (!keys.is_empty()).then(|| QUALIFIER.replace_all(&keys, "").into_owned())
        }
        _ => None,
    };
    out.push(PlanStep {
        depth,
        kind,
        server_name: name.to_string(),
        table: info["Table"].as_str().map(short_table),
        estimated_rows: info["Estimated Cardinality"]
            .as_str()
            .and_then(|s| s.trim().trim_start_matches('~').parse().ok()),
        detail: super::detail(
            info,
            &[
                "Top",
                "Order By",
                "Conditions",
                "Filters",
                "Expression",
                "Groups",
                "Aggregates",
            ],
        ),
        fact,
    });
    for c in children {
        walk(c, depth + 1, out);
    }
}

/// The `"temp".main."data".` before a column in a sort key: three name parts
/// and a dot.
static QUALIFIER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?:(?:"[^"]*"|\w+)\.){3}"#).expect("static regex"));

fn kind_of(name: &str) -> StepKind {
    match name {
        "SEQ_SCAN" | "TABLE_SCAN" | "COLUMN_DATA_SCAN" => StepKind::ReadTable,
        n if n.starts_with("READ_") => StepKind::ReadTable,
        "INDEX_SCAN" => StepKind::ReadIndex,
        "HASH_JOIN" => StepKind::JoinHash,
        "NESTED_LOOP_JOIN" | "BLOCKWISE_NL_JOIN" | "CROSS_PRODUCT" => StepKind::JoinNestedLoop,
        "PIECEWISE_MERGE_JOIN" | "IE_JOIN" | "MERGE_JOIN" => StepKind::JoinMerge,
        n if n.ends_with("_JOIN") => StepKind::Join,
        "HASH_GROUP_BY" | "PERFECT_HASH_GROUP_BY" | "UNGROUPED_AGGREGATE" => StepKind::Group,
        "ORDER_BY" => StepKind::Sort,
        "TOP_N" => StepKind::TopN,
        "FILTER" => StepKind::Filter,
        "LIMIT" | "STREAMING_LIMIT" | "LIMIT_PERCENT" => StepKind::Limit,
        "UNION" => StepKind::Union,
        "WINDOW" | "STREAMING_WINDOW" => StepKind::Window,
        "DISTINCT" => StepKind::Distinct,
        _ => StepKind::Other,
    }
}

/// `memory.main.orders` and `"temp".main."data"` both read as the bare name.
/// ponytail: a quoted name containing a dot reads as its last part; real
/// tables rarely have one, and a parser for quoted paths is not worth it here.
fn short_table(qualified: &str) -> String {
    qualified
        .rsplit('.')
        .next()
        .unwrap_or(qualified)
        .trim_matches('"')
        .to_string()
}

#[cfg(test)]
#[path = "duckdb_tests.rs"]
mod tests;
