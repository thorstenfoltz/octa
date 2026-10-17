//! Databricks, i.e. Spark: `EXPLAIN q` answers one cell holding the physical
//! plan as an indented tree, three characters per level (`+- ` for the last
//! child, `:- ` for the others, `:  ` / `   ` to carry a level on):
//!
//! ```text
//! == Physical Plan ==
//! AdaptiveSparkPlan isFinalPlan=false
//! +- HashAggregate(keys=[name#27], functions=[sum(amount#25)])
//!    +- BroadcastHashJoin [customer_id#24], [id#26], Inner, BuildRight, false
//!       :- FileScan parquet spark_catalog.default.orders[customer_id#24] ...
//! ```
//!
//! Spark has no EXPLAIN that runs the query, and its physical plan carries no
//! row estimates. A query it cannot plan does not fail: the error comes back
//! as the plan text, so [`parse`] reports it as one. SQL warehouses run Photon,
//! whose operators carry a `Photon` prefix (`PhotonGroupingAgg`); they are
//! named like Spark's otherwise.

use std::sync::LazyLock;

use anyhow::{Result, bail};
use regex::Regex;

use super::{
    Plan, PlanStep, StepKind, cells_as_text, close_of, condition, leading_number, split_top,
};
use crate::db::DbConnector;

pub(crate) fn explain(conn: &mut dyn DbConnector, q: &str) -> Result<Plan> {
    let text = cells_as_text(&conn.query(&format!("EXPLAIN {q}"))?);
    Ok(Plan {
        steps: parse(&text)?,
        server_text: text,
        bytes_scanned: None,
    })
}

/// Expression ids (`amount#25`, `id#3L`): Spark's bookkeeping, noise to a reader.
static EXPR_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#\d+L?\b").expect("static regex"));

/// The steps of the `== Physical Plan ==` section, or the planning error the
/// server put in its place.
pub fn parse(text: &str) -> Result<Vec<PlanStep>> {
    let has_sections = text.lines().any(|l| l.starts_with("== "));
    let mut in_physical = !has_sections;
    // Adaptive execution can print the plan twice under in-tree headers
    // (`+- == Initial Plan ==`); the first one is the plan.
    let mut tree_headers = 0;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with("== ") {
            in_physical = line.trim() == "== Physical Plan ==";
            continue;
        }
        if !in_physical || line.trim().is_empty() {
            continue;
        }
        let body = line.trim_start_matches([' ', ':', '+', '-']);
        if out.is_empty() && (body.contains("Exception:") || body.starts_with("Error")) {
            // Up to the unresolved plan Spark prints after the message.
            let message = text
                .lines()
                .skip_while(|l| !l.contains(body))
                .take_while(|l| !l.starts_with('\'') && !l.starts_with("+-"))
                .collect::<Vec<_>>()
                .join("\n");
            bail!("{}", message.trim());
        }
        if body.starts_with("== ") {
            tree_headers += 1;
            if tree_headers > 1 {
                break;
            }
            continue;
        }
        let depth = (line.len() - body.len()) / 3;
        // Whole-stage code generation marks a step `*(1) `.
        let body = match body.strip_prefix("*(") {
            Some(rest) => rest.split_once(") ").map_or(rest, |(_, b)| b),
            None => body,
        };
        let name_end = body.find(['(', ' ', '[']).unwrap_or(body.len());
        let (name, rest) = body.split_at(name_end);
        let kind = kind_of(name);
        let (table, rest) = if kind == StepKind::ReadTable {
            read_table(rest)
        } else {
            (None, rest)
        };
        // `Name(args)`: the parentheses are the argument list. Only then, as
        // `Filter (a) AND (b)` begins and ends with one too.
        let rest = rest
            .strip_prefix('(')
            .and_then(|r| r.strip_suffix(')'))
            .unwrap_or(rest);
        let detail = EXPR_ID.replace_all(rest.trim(), "").into_owned();
        // A read under Spark's own Filter step only skips files by the
        // condition; the Filter is what keeps the rows, and says so.
        let under_filter = out
            .iter()
            .rev()
            .find(|s: &&PlanStep| s.depth < depth)
            .is_some_and(|p| p.kind == StepKind::Filter);
        out.push(PlanStep {
            depth,
            kind,
            server_name: name.to_string(),
            table,
            estimated_rows: None,
            fact: fact(kind, &detail, under_filter),
            detail,
        });
    }
    if out.is_empty() {
        bail!("Databricks returned a plan Octa could not read");
    }
    Ok(out)
}

/// The step's [`fact`](PlanStep::fact), read from its detail.
fn fact(kind: StepKind, detail: &str, under_filter: bool) -> Option<String> {
    let list = |field: &str| {
        bracketed(detail, field)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    match kind {
        // Photon reads and filters in one step: the WHERE lives here.
        StepKind::ReadTable if !under_filter => condition(
            ["PartitionFilters: ", "DataFilters: "]
                .into_iter()
                .filter_map(|f| bracketed(detail, f))
                .flat_map(|l| split_top(l, ", ")),
        ),
        StepKind::Filter => condition([detail]),
        StepKind::Limit => leading_number(detail),
        StepKind::Group => list("keys="),
        // `Sort [a ASC NULLS FIRST], true, 0`
        StepKind::Sort => list(""),
        StepKind::TopN => list("orderBy=").or_else(|| list("sortOrder=")),
        _ => None,
    }
}

/// What the `[...]` right after `field` in `detail` holds (`keys=[a, b]` ->
/// `a, b`). `field` must start a word, so `DataFilters: ` does not find
/// `OptionalDataFilters: `. Spark cuts a long value off with `...`; such a
/// value ends at its dots.
fn bracketed<'a>(detail: &'a str, field: &str) -> Option<&'a str> {
    let at = detail.match_indices(field).find(|(i, _)| {
        !detail[..*i]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
    })?;
    let from = &detail[at.0 + field.len()..];
    if !from.starts_with('[') {
        return None;
    }
    let inner = &from[1..close_of(from).unwrap_or(from.len())];
    Some(inner.find("...").map_or(inner, |i| &inner[..i + 3]).trim())
}

/// `parquet spark_catalog.default.orders[a#1,b#2] Batched: true, ...` ->
/// (`spark_catalog.default.orders`, what follows the column list).
fn read_table(rest: &str) -> (Option<String>, &str) {
    let Some(open) = rest.find('[') else {
        return (None, rest);
    };
    let name = rest[..open].rsplit(' ').next().unwrap_or_default();
    let after = rest[open..].find(']').map_or("", |c| &rest[open + c + 1..]);
    if name.is_empty() {
        (None, rest)
    } else {
        (Some(name.to_string()), after)
    }
}

fn kind_of(name: &str) -> StepKind {
    let n = name.strip_prefix("Photon").unwrap_or(name);
    if n.contains("Scan") {
        StepKind::ReadTable
    } else if n.contains("NestedLoopJoin") || n == "CartesianProduct" {
        StepKind::JoinNestedLoop
    } else if n.contains("HashJoin") {
        StepKind::JoinHash
    } else if n.contains("SortMergeJoin") {
        StepKind::JoinMerge
    } else if n.contains("Join") {
        StepKind::Join
    } else if n.contains("Agg") {
        StepKind::Group
    } else if n == "TakeOrderedAndProject" || n == "TopK" {
        StepKind::TopN
    } else if n == "Sort" {
        StepKind::Sort
    } else if n == "Filter" {
        StepKind::Filter
    } else if n.contains("Limit") {
        StepKind::Limit
    } else if n == "Union" {
        StepKind::Union
    } else if n.contains("Window") {
        StepKind::Window
    } else {
        StepKind::Other
    }
}

#[cfg(test)]
#[path = "databricks_tests.rs"]
mod tests;
