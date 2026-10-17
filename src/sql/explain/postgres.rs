//! PostgreSQL's `EXPLAIN (FORMAT JSON)`: `[{"Plan": node}]`, each node with a
//! "Node Type", optional "Relation Name", "Plan Rows" and child "Plans".

use anyhow::{Context, Result};
use serde_json::Value;

use super::{Plan, PlanStep, StepKind, cells_as_text};
use crate::db::DbConnector;

/// Plan `q` on a PostgreSQL connection: the JSON plan for the steps, the
/// text plan for the server's own view. Plain EXPLAIN plans only.
pub(crate) fn explain(conn: &mut dyn DbConnector, q: &str) -> Result<Plan> {
    let json = cells_as_text(&conn.query(&format!("EXPLAIN (FORMAT JSON) {q}"))?);
    let text = cells_as_text(&conn.query(&format!("EXPLAIN (FORMAT TEXT) {q}"))?);
    Ok(Plan {
        steps: parse(&json)?,
        server_text: text,
        bytes_scanned: None,
    })
}

/// The steps of a PostgreSQL JSON plan. The `Hash` node under a hash join
/// (the table being turned into a hash table) is folded into the join.
pub fn parse(json: &str) -> Result<Vec<PlanStep>> {
    let v: Value =
        serde_json::from_str(json).context("PostgreSQL returned a plan Octa could not read")?;
    let root = v
        .get(0)
        .and_then(|p| p.get("Plan"))
        .context("PostgreSQL returned a plan Octa could not read")?;
    let mut out = Vec::new();
    walk(root, 0, &mut out);
    Ok(out)
}

fn walk(n: &Value, depth: usize, out: &mut Vec<PlanStep>) {
    let name = n["Node Type"].as_str().unwrap_or_default();
    let children = n["Plans"].as_array().map(Vec::as_slice).unwrap_or_default();
    if name == "Hash" {
        for c in children {
            walk(c, depth, out);
        }
        return;
    }
    let kind = kind_of(name);
    let keys = |field: &str| {
        let list: Vec<&str> = n[field]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        (!list.is_empty()).then(|| list.join(", "))
    };
    // Postgres prints no count for a Limit, so a Limit has no fact.
    let fact = match kind {
        StepKind::ReadTable | StepKind::ReadIndex => super::condition(
            ["Index Cond", "Filter"]
                .iter()
                .filter_map(|k| n[*k].as_str()),
        ),
        StepKind::Group => keys("Group Key"),
        StepKind::Sort => keys("Sort Key"),
        _ => None,
    };
    out.push(PlanStep {
        depth,
        kind,
        server_name: name.to_string(),
        table: n["Relation Name"].as_str().map(str::to_string),
        estimated_rows: n["Plan Rows"].as_f64(),
        detail: super::detail(
            n,
            &[
                "Index Name",
                "Index Cond",
                "Hash Cond",
                "Merge Cond",
                "Join Filter",
                "Filter",
                "Sort Key",
                "Group Key",
            ],
        ),
        fact,
    });
    for c in children {
        walk(c, depth + 1, out);
    }
}

fn kind_of(name: &str) -> StepKind {
    match name {
        "Seq Scan" => StepKind::ReadTable,
        "Index Scan" | "Index Only Scan" | "Bitmap Heap Scan" | "Bitmap Index Scan" => {
            StepKind::ReadIndex
        }
        "Hash Join" => StepKind::JoinHash,
        "Nested Loop" => StepKind::JoinNestedLoop,
        "Merge Join" => StepKind::JoinMerge,
        "Aggregate" | "Group" => StepKind::Group,
        "Sort" | "Incremental Sort" => StepKind::Sort,
        "Limit" => StepKind::Limit,
        "Append" | "Merge Append" => StepKind::Union,
        "WindowAgg" => StepKind::Window,
        "Unique" => StepKind::Distinct,
        _ => StepKind::Other,
    }
}

#[cfg(test)]
#[path = "postgres_tests.rs"]
mod tests;
