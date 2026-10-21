//! Explain a query: how a database would run it, without running it.
//!
//! Every engine answers in its own shape (DuckDB and PostgreSQL JSON, MySQL and
//! Databricks a text tree, BigQuery only a byte count), so each has a parser file that turns
//! its answer into one flat [`Plan`]: the steps in pre-order, each with its
//! depth. The three views the SQL panel offers (plain words, table, the
//! server's own text) and the CLI and MCP output are all built from that one
//! list, so they cannot name different steps.
//!
//! Nothing here sends `EXPLAIN ANALYZE`: that runs the query.

pub mod bigquery;
pub mod databricks;
pub mod duckdb;
pub mod mysql;
pub mod postgres;

use anyhow::{Result, bail};
use serde_json::Value;

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::db::{DbConnector, DbEngine};
use crate::i18n::t;

/// What one step of a plan does, in words a reader knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    ReadTable,
    ReadIndex,
    JoinHash,
    JoinNestedLoop,
    JoinMerge,
    Join,
    Group,
    Sort,
    TopN,
    Filter,
    Limit,
    Union,
    Window,
    Distinct,
    /// A step Octa has no plain name for; it shows the server's own.
    Other,
}

impl StepKind {
    /// Stable id for the CLI and MCP `step` field.
    pub fn id(self) -> &'static str {
        match self {
            StepKind::ReadTable => "read_table",
            StepKind::ReadIndex => "read_index",
            StepKind::JoinHash => "join_hash",
            StepKind::JoinNestedLoop => "join_nested_loop",
            StepKind::JoinMerge => "join_merge",
            StepKind::Join => "join",
            StepKind::Group => "group",
            StepKind::Sort => "sort",
            StepKind::TopN => "top_n",
            StepKind::Filter => "filter",
            StepKind::Limit => "limit",
            StepKind::Union => "union",
            StepKind::Window => "window",
            StepKind::Distinct => "distinct",
            StepKind::Other => "other",
        }
    }

    fn i18n_key(self) -> Option<&'static str> {
        Some(match self {
            StepKind::ReadTable => "explain.step_read_table",
            StepKind::ReadIndex => "explain.step_read_index",
            StepKind::JoinHash => "explain.step_join_hash",
            StepKind::JoinNestedLoop => "explain.step_join_nested_loop",
            StepKind::JoinMerge => "explain.step_join_merge",
            StepKind::Join => "explain.step_join",
            StepKind::Group => "explain.step_group",
            StepKind::Sort => "explain.step_sort",
            StepKind::TopN => "explain.step_top_n",
            StepKind::Filter => "explain.step_filter",
            StepKind::Limit => "explain.step_limit",
            StepKind::Union => "explain.step_union",
            StepKind::Window => "explain.step_window",
            StepKind::Distinct => "explain.step_distinct",
            StepKind::Other => return None,
        })
    }
}

/// One step of a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanStep {
    /// Nesting level: 0 for the step that produces the result.
    pub depth: usize,
    pub kind: StepKind,
    /// The operator as the server names it (`HASH_JOIN`, `Seq Scan`).
    pub server_name: String,
    /// The table this step reads, if it reads one.
    pub table: Option<String>,
    /// The server's estimate of the rows this step produces.
    pub estimated_rows: Option<f64>,
    /// Join condition, filter, sort key or index, in the server's words.
    pub detail: String,
    /// What sets this step apart, in the query's own terms: the condition a
    /// read or filter keeps rows by, how many rows a limit keeps, what a
    /// group or sort goes by. Plain words print it; without it two queries
    /// over the same table read the same.
    pub fact: Option<String>,
}

impl PlanStep {
    /// The name a person reads: the plain label, or the server's own for a
    /// step Octa does not know.
    pub fn label(&self) -> String {
        match self.kind.i18n_key() {
            Some(key) => t(key),
            None => self.server_name.clone(),
        }
    }
}

/// A query plan, whichever engine produced it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub steps: Vec<PlanStep>,
    /// The server's own plan text, box drawing already turned into ASCII.
    pub server_text: String,
    /// BigQuery's dry run: the bytes the query would read. Set instead of
    /// steps; never turned into a price.
    pub bytes_scanned: Option<u64>,
}

/// The plan in plain words, the SQL panel's first view and the CLI and MCP
/// text: numbered in the order
/// the database works (what a step uses comes before it), plain step names
/// plus each step's [`fact`](PlanStep::fact), none of the server's other wording. Steps without a plain name are left
/// out, Table and Server's own keep them, unless the plan holds nothing else.
pub fn plain_text(plan: &Plan) -> String {
    if let Some(bytes) = plan.bytes_scanned {
        return format!(
            "{}\n",
            t("explain.would_scan").replace("{size}", &crate::ui::status_bar::human_size(bytes))
        );
    }
    let steps = &plan.steps;
    let any_named = steps.iter().any(|s| s.kind != StepKind::Other);
    let shown = |i: usize| !any_named || steps[i].kind != StepKind::Other;
    // Steps arrive in pre-order: a step's parent is the nearest earlier one
    // a level up.
    let parent: Vec<Option<usize>> = (0..steps.len())
        .map(|i| (0..i).rev().find(|&j| steps[j].depth < steps[i].depth))
        .collect();
    // A left-out step hands what it uses to the nearest shown step above it.
    let mut inputs: Vec<Vec<usize>> = vec![Vec::new(); steps.len()];
    let mut roots = Vec::new();
    for i in (0..steps.len()).filter(|&i| shown(i)) {
        let mut p = parent[i];
        while let Some(j) = p.filter(|&j| !shown(j)) {
            p = parent[j];
        }
        match p {
            Some(p) => inputs[p].push(i),
            None => roots.push(i),
        }
    }
    fn visit(i: usize, inputs: &[Vec<usize>], order: &mut Vec<usize>) {
        for &c in &inputs[i] {
            visit(c, inputs, order);
        }
        order.push(i);
    }
    let mut order = Vec::new();
    for &r in &roots {
        visit(r, &inputs, &mut order);
    }
    let mut number = vec![0; steps.len()];
    let mut out = String::new();
    for (n, &i) in order.iter().enumerate() {
        number[i] = n + 1;
        let s = &steps[i];
        let fact = s.fact.as_deref().map(|f| match s.kind {
            StepKind::Limit => t("explain.first_rows").replace("{n}", f),
            StepKind::Group | StepKind::Sort | StepKind::TopN => {
                t("explain.by").replace("{keys}", f)
            }
            _ => t("explain.keeps_where").replace("{cond}", f),
        });
        let mut parts = vec![match (&s.table, &fact) {
            (Some(table), _) => format!("{}: {table}", s.label()),
            (None, Some(fact)) => format!("{}: {fact}", s.label()),
            (None, None) => s.label(),
        }];
        if s.table.is_some() {
            parts.extend(fact);
        }
        // One input is always the step just before; two or more are named.
        if let [rest @ .., last] = inputs[i].as_slice()
            && !rest.is_empty()
        {
            let rest: Vec<String> = rest.iter().map(|&c| number[c].to_string()).collect();
            parts.push(
                t("explain.uses_steps")
                    .replace("{list}", &rest.join(", "))
                    .replace("{last}", &number[*last].to_string()),
            );
        }
        if let Some(rows) = s.estimated_rows {
            let rows = crate::ui::status_bar::format_number(rows.max(0.0).round() as usize);
            parts.push(t("explain.rows").replace("{n}", &rows));
        }
        out.push_str(&format!("{}. {}\n", n + 1, parts.join(", ")));
    }
    out
}

/// The plan as rows, for the result grid, `-f json|csv` and Export.
pub fn steps_table(plan: &Plan) -> DataTable {
    let col = |name: &str, ty: &str| ColumnInfo {
        name: name.to_string(),
        data_type: ty.to_string(),
    };
    let mut out = DataTable::empty();
    if let Some(bytes) = plan.bytes_scanned {
        out.columns = vec![col("bytes_scanned", "Int64")];
        out.rows = vec![vec![CellValue::Int(
            i64::try_from(bytes).unwrap_or(i64::MAX),
        )]];
        return out;
    }
    out.columns = vec![
        col("depth", "Int64"),
        col("step", "Utf8"),
        col("table", "Utf8"),
        col("estimated_rows", "Int64"),
        col("detail", "Utf8"),
        col("server_name", "Utf8"),
    ];
    out.rows = plan
        .steps
        .iter()
        .map(|s| {
            vec![
                CellValue::Int(s.depth as i64),
                CellValue::String(s.label()),
                s.table.clone().map_or(CellValue::Null, CellValue::String),
                s.estimated_rows
                    .map_or(CellValue::Null, |r| CellValue::Int(r.round() as i64)),
                CellValue::String(s.detail.clone()),
                CellValue::String(s.server_name.clone()),
            ]
        })
        .collect();
    out
}

/// Box-drawing characters as ASCII: egui's font draws them as empty squares.
pub fn ascii_server_text(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{2500}' | '\u{2501}' | '\u{2504}' | '\u{2505}' | '\u{2508}' | '\u{2509}'
            | '\u{254C}' | '\u{254D}' | '\u{2550}' => '-',
            '\u{2502}' | '\u{2503}' | '\u{2506}' | '\u{2507}' | '\u{250A}' | '\u{250B}'
            | '\u{254E}' | '\u{254F}' | '\u{2551}' => '|',
            '\u{2500}'..='\u{257F}' => '+',
            c => c,
        })
        .collect()
}

/// The query without its trailing semicolon, or an error when any other
/// semicolon is left. A plan answers for one statement, and Explain skips the
/// write gates, so nothing after a second statement may reach the server.
/// Every semicolon counts, even one in a string or a comment: engines lex
/// those differently (MySQL reads `--1` as minus minus one, `#` as a comment
/// and `\'` as a quote), and its connector runs every statement it is sent.
pub fn single_statement(query: &str) -> Result<&str> {
    let q = query
        .trim()
        .trim_end_matches(|c: char| c == ';' || c.is_whitespace());
    if q.is_empty() {
        bail!(t("explain.empty"));
    }
    if crate::sql::has_inner_semicolon(q) {
        bail!(t("explain.one_statement"));
    }
    Ok(q)
}

/// Engines Explain can answer for. The SQL panel greys the button out for
/// the rest, naming the engine.
pub fn can_explain(engine: DbEngine) -> bool {
    matches!(
        engine,
        DbEngine::Postgres
            | DbEngine::MySql
            | DbEngine::BigQuery
            | DbEngine::Databricks
            | DbEngine::DuckDb
    )
}

/// The plan `query` would run on `conn`, without running it.
pub fn explain_db(conn: &mut dyn DbConnector, query: &str) -> Result<Plan> {
    let q = single_statement(query)?;
    match conn.engine() {
        DbEngine::Postgres => postgres::explain(conn, q),
        DbEngine::MySql => mysql::explain(conn, q),
        DbEngine::Databricks => databricks::explain(conn, q),
        DbEngine::DuckDb => duckdb::explain(conn, q),
        DbEngine::BigQuery => Ok(Plan {
            bytes_scanned: Some(conn.dry_run_bytes(q)?),
            ..Plan::default()
        }),
        other => bail!(t("explain.not_available").replace("{engine}", other.label())),
    }
}

/// Every first cell of a one-column answer, one per line: PostgreSQL's text
/// plan arrives as one row per line, the JSON plans as a single cell.
fn cells_as_text(table: &DataTable) -> String {
    table
        .rows
        .iter()
        .filter_map(|r| r.first())
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `Key: value; Key: value` for the keys `obj` has, in `keys` order. Arrays
/// are joined with commas, so a sort-key list reads as one value.
pub(crate) fn detail(obj: &Value, keys: &[&str]) -> String {
    keys.iter()
        .filter_map(|k| {
            let text = match obj.get(*k)? {
                Value::String(s) => s.clone(),
                Value::Array(a) => a
                    .iter()
                    .map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string))
                    .collect::<Vec<_>>()
                    .join(", "),
                Value::Null => return None,
                other => other.to_string(),
            };
            (!text.is_empty()).then(|| format!("{k}: {text}"))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The condition `exprs` make together, for a step's `fact`: split at every
/// top-level AND, outer parentheses dropped, joined with ` AND `. A not-null
/// test reads `x IS NOT NULL` and is left out when another part names `x`
/// (Spark and MySQL add one for every compared column); DuckDB's runtime
/// `optional:` filters are left out. `None` when nothing is left.
pub(crate) fn condition<'a>(exprs: impl IntoIterator<Item = &'a str>) -> Option<String> {
    fn conjuncts<'a>(expr: &'a str, out: &mut Vec<&'a str>) {
        let e = unwrap_parens(expr);
        match split_top(e, " AND ").as_slice() {
            [one] => out.push(one),
            many => many.iter().for_each(|p| conjuncts(p, out)),
        }
    }
    let mut parts = Vec::new();
    for e in exprs {
        conjuncts(e, &mut parts);
    }
    parts.retain(|p| !p.is_empty() && !p.starts_with("optional:"));
    let kept: Vec<String> = parts
        .iter()
        .enumerate()
        .filter_map(|(i, p)| match not_null_column(p) {
            Some(c)
                if parts
                    .iter()
                    .enumerate()
                    .any(|(j, q)| j != i && not_null_column(q).is_none() && mentions(q, c)) =>
            {
                None
            }
            Some(c) => Some(format!("{c} IS NOT NULL")),
            None => Some(p.to_string()),
        })
        .collect();
    (!kept.is_empty()).then(|| kept.join(" AND "))
}

/// The column of `isnotnull(x)` (Spark) or `x is not null` (MySQL).
fn not_null_column(part: &str) -> Option<&str> {
    if let Some(c) = part
        .strip_prefix("isnotnull(")
        .and_then(|r| r.strip_suffix(')'))
    {
        return Some(c);
    }
    let cut = part.len().checked_sub(" is not null".len())?;
    part.get(cut..)
        .filter(|tail| tail.eq_ignore_ascii_case(" is not null"))
        .and_then(|_| part.get(..cut))
}

/// `text` names `column` as a whole word (`id` is not in `customer_id`).
fn mentions(text: &str, column: &str) -> bool {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    text.match_indices(column).any(|(i, _)| {
        !word(text[..i].chars().next_back()) && !word(text[i + column.len()..].chars().next())
    })
}

/// `expr` without parentheses that wrap all of it: `((a))` -> `a`.
fn unwrap_parens(expr: &str) -> &str {
    let mut e = expr.trim();
    while e.starts_with('(') && close_of(e) == Some(e.len() - 1) {
        e = e[1..e.len() - 1].trim();
    }
    e
}

/// Byte index of the bracket closing the one `s` starts with. Brackets inside
/// quoted text do not count.
pub(crate) fn close_of(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut quoted = false;
    for (i, c) in s.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            '(' | '[' if !quoted => depth += 1,
            ')' | ']' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// `s` split at every `sep` (any case) outside brackets and quoted text.
pub(crate) fn split_top<'a>(s: &'a str, sep: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let (mut depth, mut quoted, mut start) = (0i32, false, 0);
    for (i, c) in s.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            '(' | '[' if !quoted => depth += 1,
            ')' | ']' if !quoted => depth -= 1,
            _ if depth == 0
                && !quoted
                && i >= start
                && s.get(i..i + sep.len())
                    .is_some_and(|w| w.eq_ignore_ascii_case(sep)) =>
            {
                out.push(s[start..i].trim());
                start = i + sep.len();
            }
            _ => {}
        }
    }
    out.push(s[start..].trim());
    out
}

/// The number `s` starts with: a limit's row count (`10, 0`, `5 row(s)`).
pub(crate) fn leading_number(s: &str) -> Option<String> {
    let n: String = s.trim().chars().take_while(char::is_ascii_digit).collect();
    (!n.is_empty()).then_some(n)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
