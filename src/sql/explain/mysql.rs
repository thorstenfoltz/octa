//! MySQL and MariaDB. MySQL 8.0.16+ prints `EXPLAIN FORMAT=TREE`, a readable
//! tree that carries every step (Limit and Sort included); MariaDB and older
//! MySQL refuse it, so they fall back to `EXPLAIN FORMAT=JSON`, nested blocks
//! of `table`, `nested_loop`, `ordering_operation` / `filesort` and friends.

use anyhow::{Context, Result};
use serde_json::Value;

use super::{Plan, PlanStep, StepKind, cells_as_text};
use crate::db::DbConnector;

/// TREE first; a refusal or a TREE answer with no steps falls back to JSON.
/// If JSON fails too, its error carries the TREE error as context.
pub(crate) fn explain(conn: &mut dyn DbConnector, q: &str) -> Result<Plan> {
    let tree_err = match conn.query(&format!("EXPLAIN FORMAT=TREE {q}")) {
        Ok(t) => {
            let text = cells_as_text(&t);
            let steps = parse_tree(&text);
            if !steps.is_empty() {
                return Ok(Plan {
                    steps,
                    server_text: text,
                    bytes_scanned: None,
                });
            }
            None
        }
        Err(e) => Some(e),
    };
    let json = conn
        .query(&format!("EXPLAIN FORMAT=JSON {q}"))
        .map_err(|e| match tree_err {
            Some(t) => e.context(format!("EXPLAIN FORMAT=TREE failed first: {t:#}")),
            None => e,
        })?;
    let json = cells_as_text(&json);
    Ok(Plan {
        steps: parse_json(&json)?,
        server_text: crate::data::json_util::pretty_print(&json),
        bytes_scanned: None,
    })
}

/// The steps of a `FORMAT=TREE` plan: one `-> step` per line, four spaces per
/// level, the estimate in a trailing `(cost=... rows=N)`.
pub fn parse_tree(text: &str) -> Vec<PlanStep> {
    let mut out = Vec::new();
    for line in text.lines() {
        // A step that wraps continues on lines without an arrow.
        let Some(arrow) = line.find("-> ") else {
            continue;
        };
        let body = &line[arrow + 3..];
        let (step, cost) = match body.find("  (") {
            Some(i) => (&body[..i], &body[i..]),
            None => (body, ""),
        };
        let estimated_rows = cost.find("rows=").and_then(|i| {
            cost[i + 5..]
                .split(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | 'e' | '+')))
                .next()
                .and_then(|n| n.parse().ok())
        });
        let (kind, table, detail) = classify_tree_step(step);
        let fact = match kind {
            StepKind::Limit => super::leading_number(&detail),
            // `total DESC, limit input to 5 row(s) per chunk`
            StepKind::Sort => detail
                .split(", limit input")
                .next()
                .filter(|k| !k.is_empty())
                .map(str::to_string),
            StepKind::Filter => super::condition([detail.as_str()]),
            // `PRIMARY (id=o.customer_id)`: the lookup's condition.
            StepKind::ReadIndex => detail
                .find('(')
                .and_then(|i| super::condition([&detail[i..]])),
            _ => None,
        };
        out.push(PlanStep {
            depth: arrow / 4,
            kind,
            server_name: step.to_string(),
            table,
            estimated_rows,
            detail,
            fact,
        });
    }
    out
}

/// Kind, table and detail of one TREE step's text.
fn classify_tree_step(step: &str) -> (StepKind, Option<String>, String) {
    let after = |prefix: &str| step.strip_prefix(prefix).unwrap_or_default().to_string();
    let lower = step.to_ascii_lowercase();
    if step.starts_with("Limit") {
        return (StepKind::Limit, None, after("Limit: "));
    }
    if step.starts_with("Sort") {
        return (StepKind::Sort, None, after("Sort: "));
    }
    if step.starts_with("Filter") {
        return (StepKind::Filter, None, after("Filter: "));
    }
    if let Some(rest) = step.strip_prefix("Table scan on ") {
        // `<temporary>` is MySQL's own scratch table, not one of the user's.
        if rest.starts_with('<') {
            return (StepKind::Other, None, String::new());
        }
        return (StepKind::ReadTable, Some(rest.to_string()), String::new());
    }
    if lower.contains("index") && step.contains(" on ") && step.contains(" using ") {
        let on = step.find(" on ").map_or(0, |i| i + 4);
        let using = step.find(" using ").unwrap_or(step.len());
        let table = step.get(on..using).map(str::to_string);
        let detail = step.get(using + 7..).unwrap_or_default().to_string();
        return (StepKind::ReadIndex, table, detail);
    }
    if lower.contains("hash join") {
        return (StepKind::JoinHash, None, String::new());
    }
    if step.starts_with("Nested loop") {
        return (StepKind::JoinNestedLoop, None, String::new());
    }
    if step.starts_with("Aggregate") || step.starts_with("Group") {
        return (StepKind::Group, None, String::new());
    }
    if step.starts_with("Window") {
        return (StepKind::Window, None, String::new());
    }
    if step.starts_with("Remove duplicates") {
        return (StepKind::Distinct, None, String::new());
    }
    if lower.starts_with("union") {
        return (StepKind::Union, None, String::new());
    }
    (StepKind::Other, None, String::new())
}

/// The steps of a `FORMAT=JSON` plan (MySQL or MariaDB). Objects without a
/// key of their own (`query_block`, `temporary_table`, `cost_info`) are walked
/// through without adding a step.
pub fn parse_json(json: &str) -> Result<Vec<PlanStep>> {
    let v: Value =
        serde_json::from_str(json).context("MySQL returned a plan Octa could not read")?;
    let mut out = Vec::new();
    walk(&v, 0, &mut out);
    Ok(out)
}

fn walk(v: &Value, depth: usize, out: &mut Vec<PlanStep>) {
    match v {
        Value::Array(items) => {
            for item in items {
                walk(item, depth, out);
            }
        }
        Value::Object(map) => {
            for (key, val) in map {
                let kind = match key.as_str() {
                    "table" => {
                        push_table(val, depth, out);
                        continue;
                    }
                    "nested_loop" => StepKind::JoinNestedLoop,
                    "ordering_operation" | "filesort" => StepKind::Sort,
                    "grouping_operation" => StepKind::Group,
                    "duplicates_removal" => StepKind::Distinct,
                    "union_result" => StepKind::Union,
                    _ => {
                        walk(val, depth, out);
                        continue;
                    }
                };
                out.push(PlanStep {
                    depth,
                    kind,
                    server_name: key.clone(),
                    table: None,
                    estimated_rows: None,
                    detail: super::detail(val, &["sort_key"]),
                    fact: val["sort_key"].as_str().map(str::to_string),
                });
                walk(val, depth + 1, out);
            }
        }
        _ => {}
    }
}

fn push_table(t: &Value, depth: usize, out: &mut Vec<PlanStep>) {
    let access = t["access_type"].as_str().unwrap_or_default();
    out.push(PlanStep {
        depth,
        kind: if access == "ALL" {
            StepKind::ReadTable
        } else {
            StepKind::ReadIndex
        },
        server_name: format!("table ({access})"),
        table: t["table_name"].as_str().map(str::to_string),
        estimated_rows: t["rows_examined_per_scan"]
            .as_f64()
            .or_else(|| t["rows"].as_f64()),
        detail: super::detail(t, &["key", "attached_condition"]),
        fact: super::condition(t["attached_condition"].as_str()),
    });
}

#[cfg(test)]
#[path = "mysql_tests.rs"]
mod tests;
