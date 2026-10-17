//! One-shot "turn this sentence into a query" request for the SQL panel.
//!
//! Deliberately not the agent loop: no tools, no follow-up, one request and
//! one reply, so a text box in a panel can never become an autonomous
//! session. The answer lands in the editor and is never run, and the parser
//! refuses anything that is not a single SELECT, so a plain-language box
//! cannot hand back a DELETE to fire off by reflex.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::AtomicBool;

use octa::data::ColumnInfo;
use octa::db::relationships::{ColumnRow, ForeignKey};

use super::providers::{ChatProvider, ProviderConfig};
use super::types::{ChatEvent, Message};

/// Token budget for the joinable-tables block, not a correctness limit: the
/// model writes better SQL for knowing its neighbours, but a wide warehouse
/// schema repeated on every question would undo the diet the chat panel is on.
const MAX_RELATED_TABLES: usize = 8;
const MAX_RELATED_COLUMNS: usize = 40;

/// What the server says `table` can be joined to, one hop out, as prompt text.
///
/// Declared foreign keys only, straight from [`octa::db::relationships::scan`],
/// so this costs the two catalog queries that module already runs and reads no
/// table data. Empty when nothing declares a key, which is the normal case on
/// Redshift, Snowflake, Databricks and BigQuery. That is the whole safety
/// story: no keys, no block, and the prompt is the one that shipped before.
///
/// `table` and the labels are `schema.table`, which is how `scan` spells them
/// and how the query must spell them.
pub fn related_tables_block(table: &str, columns: &[ColumnRow], fks: &[ForeignKey]) -> String {
    // Neighbour -> the conditions that reach it. Sorted, so the same schema
    // always produces the same prompt: a stable prefix is what lets a provider
    // cache it, and what makes the tests readable.
    let mut joins: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for fk in fks {
        let (child, parent) = (fk.child(), fk.parent());
        let cond = format!(
            "{}.{} = {}.{}",
            child, fk.child_column, parent, fk.parent_column
        );
        if child == table {
            joins.entry(parent).or_default().push(cond);
        } else if parent == table {
            joins.entry(child).or_default().push(cond);
        }
    }
    if joins.is_empty() {
        return String::new();
    }

    let kept: Vec<String> = joins.keys().take(MAX_RELATED_TABLES).cloned().collect();
    let omitted = joins.len() - kept.len();

    // Columns for the kept neighbours only. `columns` covers the whole schema,
    // so this walks it once rather than once per neighbour.
    let mut cols: HashMap<&str, Vec<&str>> =
        kept.iter().map(|l| (l.as_str(), Vec::new())).collect();
    for (schema, tbl, col) in columns {
        if let Some(v) = cols.get_mut(format!("{schema}.{tbl}").as_str()) {
            v.push(col.as_str());
        }
    }

    let mut out = format!(
        "Tables the database declares as joinable to {table}, with the key each \
         join uses:\n"
    );
    for label in &kept {
        if label == table {
            // A table whose foreign key points at itself. Its columns are
            // already listed above, so only the condition is news.
            out.push_str(&format!("{label} (this table, joined to itself)\n"));
        } else {
            let all = cols.get(label.as_str()).map(Vec::as_slice).unwrap_or(&[]);
            if all.is_empty() {
                // A parent in a schema the scan did not cover: the key query
                // filters on the child's schema, the column query on its own,
                // so a cross-schema parent arrives without its columns. Say so
                // rather than print an empty list, which would read as "this
                // table has no columns" and invite invented ones.
                out.push_str(&format!("{label} (columns not read)\n"));
            } else {
                let shown = all
                    .iter()
                    .take(MAX_RELATED_COLUMNS)
                    .copied()
                    .collect::<Vec<_>>()
                    .join(", ");
                let extra = all.len().saturating_sub(MAX_RELATED_COLUMNS);
                out.push_str(&format!("{label} columns: {shown}"));
                if extra > 0 {
                    out.push_str(&format!(" (+{extra} more)"));
                }
                out.push('\n');
            }
        }
        for cond in &joins[label] {
            out.push_str(&format!("  join: {cond}\n"));
        }
    }
    if omitted > 0 {
        out.push_str(&format!(
            "({omitted} further related tables are not listed.)\n"
        ));
    }
    out
}

/// Cap on tables described from the SQL workspace. Same budget reasoning as
/// [`MAX_RELATED_TABLES`]: enough to answer a real question, not a whole
/// warehouse catalogue resent on every keystroke.
pub const MAX_WORKSPACE_TABLES: usize = 12;

/// What the local DuckDB workspace can query besides the active tab, as prompt
/// text: registered tables and the tables inside every ATTACHed database, each
/// with its columns.
///
/// Without this the prompt hardcoded "refer to the table only as data", so a
/// question spanning an attached server could not be answered at all - and the
/// tab that is *only* a server target has no `data` to refer to. Empty when the
/// workspace holds nothing but the active tab, which restores the old prompt.
pub fn workspace_block(tables: &[(String, Vec<(String, String)>)]) -> String {
    if tables.is_empty() {
        return String::new();
    }
    let mut out = String::from("The SQL workspace can also query these tables:\n");
    for (name, cols) in tables.iter().take(MAX_WORKSPACE_TABLES) {
        out.push_str(&format!("- {name}\n"));
        for (col, ty) in cols.iter().take(MAX_RELATED_COLUMNS) {
            out.push_str(&format!("  - {col} ({ty})\n"));
        }
        if cols.len() > MAX_RELATED_COLUMNS {
            out.push_str(&format!(
                "  - ... and {} more columns\n",
                cols.len() - MAX_RELATED_COLUMNS
            ));
        }
    }
    if tables.len() > MAX_WORKSPACE_TABLES {
        out.push_str(&format!(
            "- ... and {} more tables (not described)\n",
            tables.len() - MAX_WORKSPACE_TABLES
        ));
    }
    out
}

/// Cap on table names listed without columns. Names are cheap, but a
/// warehouse schema can hold thousands.
const MAX_SERVER_TABLE_NAMES: usize = 200;

/// The tables of a live server the SQL editor runs against, as prompt text,
/// for a tab that has no table of its own there.
///
/// Tables the question names come first and get their columns, the rest fill
/// the column budget in catalog order and are then listed by name only.
/// Declared keys between listed tables become `join:` lines. Meant for
/// [`AskSqlFacts::workspace`], so [`build_prompt`] applies its "only the tables
/// listed above" rules. Names are `catalog.schema.table` on the three-level
/// engines, which is how the query has to spell them.
pub fn server_tables_block(
    catalog: Option<&str>,
    columns: &[ColumnRow],
    fks: &[ForeignKey],
    question: &str,
) -> String {
    let prefix = catalog
        .filter(|c| !c.is_empty())
        .map(|c| format!("{c}."))
        .unwrap_or_default();
    let mut order: Vec<String> = Vec::new();
    let mut cols: HashMap<String, Vec<&str>> = HashMap::new();
    for (s, t, c) in columns {
        let label = format!("{s}.{t}");
        cols.entry(label.clone())
            .or_insert_with(|| {
                order.push(label.clone());
                Vec::new()
            })
            .push(c.as_str());
    }
    if order.is_empty() {
        return String::new();
    }
    // Whole words, so a table called `s` is not "named" by every question
    // containing the letter.
    let words: std::collections::HashSet<String> = question
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    // Stable sort: named tables keep their catalog order among themselves.
    order.sort_by_key(|l| {
        let table = l.rsplit('.').next().unwrap_or(l).to_lowercase();
        !words.contains(&table)
    });

    let mut out = String::from("The database has these tables:\n");
    for label in order.iter().take(MAX_WORKSPACE_TABLES) {
        out.push_str(&format!("- {prefix}{label}\n"));
        let all = &cols[label];
        for c in all.iter().take(MAX_RELATED_COLUMNS) {
            out.push_str(&format!("  - {c}\n"));
        }
        if all.len() > MAX_RELATED_COLUMNS {
            out.push_str(&format!(
                "  - ... and {} more columns\n",
                all.len() - MAX_RELATED_COLUMNS
            ));
        }
    }
    let shown = MAX_WORKSPACE_TABLES + MAX_SERVER_TABLE_NAMES;
    let rest = &order[order.len().min(MAX_WORKSPACE_TABLES)..order.len().min(shown)];
    if !rest.is_empty() {
        out.push_str("Other tables (columns not listed):\n");
        for l in rest {
            out.push_str(&format!("- {prefix}{l}\n"));
        }
    }
    if order.len() > shown {
        out.push_str(&format!(
            "- ... and {} more tables (not listed)\n",
            order.len() - shown
        ));
    }

    let listed: std::collections::HashSet<&str> =
        order.iter().take(shown).map(String::as_str).collect();
    let joins: Vec<String> = fks
        .iter()
        .filter(|f| listed.contains(f.child().as_str()) && listed.contains(f.parent().as_str()))
        .map(|f| {
            format!(
                "  join: {prefix}{}.{} = {prefix}{}.{}\n",
                f.child(),
                f.child_column,
                f.parent(),
                f.parent_column
            )
        })
        .collect();
    if !joins.is_empty() {
        out.push_str("Declared foreign keys:\n");
        out.extend(joins);
    }
    out
}

/// Everything the prompt says about the data, gathered on the UI thread and
/// moved to the worker.
///
/// A struct rather than six more parameters: `build_prompt` was already at the
/// argument count where `clippy::too_many_arguments` fires, and the no-`#[allow]`
/// rule means the escape hatch is not available.
pub struct AskSqlFacts {
    /// How the active table must be spelled: `data` locally, `schema.table`
    /// against a server.
    pub table_name: String,
    pub dialect: String,
    pub columns: Vec<ColumnInfo>,
    /// Rows in the whole table. `None` when only a page is loaded, which is
    /// the normal case for a live database tab - a page size stated as a table
    /// size is worse than saying nothing.
    pub row_count: Option<usize>,
    /// [`related_tables_block`] output (server mode, declared foreign keys).
    pub related: String,
    /// [`workspace_block`] output (local mode, registered + attached tables).
    pub workspace: String,
}

/// The instruction sent as the system prompt. Short on purpose: one job, one
/// output shape.
pub fn build_prompt(facts: &AskSqlFacts, question: &str) -> String {
    let AskSqlFacts {
        table_name,
        dialect,
        columns,
        row_count,
        related,
        workspace,
    } = facts;
    let cols = columns
        .iter()
        .map(|c| format!("- {} ({})", c.name, c.data_type))
        .collect::<Vec<_>>()
        .join("\n");
    // Two different jobs. Alone, the model must not invent a second table.
    // With neighbours it may join, but only on keys the server declared, and
    // it has to be told that a join changes what an aggregate counts: joining
    // one row to its many children and then summing a column of the one is
    // the classic way to report a number that is silently several times too
    // big.
    let (related_block, table_rules) = if related.trim().is_empty() {
        (
            String::new(),
            format!("- Refer to the table only as {table_name}.\n"),
        )
    } else {
        (
            format!("\n{related}"),
            format!(
                "- Refer to {table_name} and the tables above only by those exact names.\n\
                 - Join only on a listed key pair. Never invent a join condition.\n\
                 - Query {table_name} alone unless the question needs a joined table.\n\
                 - A join can multiply rows, so do not aggregate a column of {table_name} \
                 after joining a table that has several rows per row of it. Aggregate \
                 first, or count distinct.\n"
            ),
        )
    };
    // The workspace block replaces the "only as {table_name}" rule outright: a
    // tab opened solely to attach a server has no table of its own, and telling
    // the model to name one that is not registered produces SQL that cannot run.
    let (workspace_block, table_rules) = if workspace.trim().is_empty() {
        (String::new(), table_rules)
    } else {
        (
            format!("\n{workspace}"),
            "- Use only the table names listed above, spelled exactly as given, \
             qualification included.\n\
             - Do not invent tables or columns. If the question cannot be answered from \
             them, answer with the closest query that can.\n"
                .to_string(),
        )
    };
    // Describing the active table at all only makes sense when there is one.
    let table_block = if columns.is_empty() {
        format!("The SQL dialect is {dialect}.\n")
    } else {
        let size = match row_count {
            Some(n) => format!(", has {n} rows,"),
            None => String::new(),
        };
        format!(
            "The SQL dialect is {dialect}. The table is named {table_name}{size} and has these \
             columns:\n{cols}\n"
        )
    };
    format!(
        "You turn a question about a table into one SQL query. Reply with JSON \
         only, no prose.\n\
         \n\
         {table_block}{related_block}{workspace_block}\n\
         Reply shape:\n\
         {{\"sql\":\"SELECT ...\"}}\n\
         \n\
         Rules:\n\
         - Exactly one statement. Do not write two statements.\n\
         - It must be a SELECT (a leading WITH clause is fine).\n\
         - Never write INSERT, UPDATE, DELETE, DROP, CREATE or ALTER.\n\
         - Use only the column names listed above, spelled exactly as given.\n\
         {table_rules}\
         \n\
         Question: {question}"
    )
}

/// Byte offsets of the semicolons that actually separate statements. Walks the
/// string once, treating `''` as an escaped quote, which is what lets a
/// semicolon inside `'a;b'` stay data rather than being read as a break.
fn split_points_outside_literals(sql: &str) -> Vec<usize> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut in_literal = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                if in_literal && bytes.get(i + 1) == Some(&b'\'') {
                    i += 1; // escaped quote, stay inside
                } else {
                    in_literal = !in_literal;
                }
            }
            b';' if !in_literal => out.push(i),
            _ => {}
        }
        i += 1;
    }
    out
}

/// Pull the JSON object out of a reply that may be fenced or prefixed with
/// chat, then check it is a single SELECT.
///
/// Every failure is an error rather than a partial result: inserting half of
/// a misunderstood sentence into the user's editor is worse than inserting
/// nothing.
pub fn parse_reply(reply: &str) -> Result<String, String> {
    let start = reply
        .find('{')
        .ok_or_else(|| "no JSON in the reply".to_string())?;
    let end = reply
        .rfind('}')
        .ok_or_else(|| "no JSON in the reply".to_string())?;
    if end <= start {
        return Err("no JSON in the reply".to_string());
    }
    let v: serde_json::Value = serde_json::from_str(&reply[start..=end])
        .map_err(|e| format!("could not read the reply as JSON: {e}"))?;

    let sql = v
        .get("sql")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if sql.is_empty() {
        return Err("the assistant did not produce a query".to_string());
    }

    // One statement only. A single trailing semicolon is fine; anything after
    // it is a second statement.
    let semis = split_points_outside_literals(&sql);
    let trailing_only =
        semis.is_empty() || (semis.len() == 1 && sql[semis[0] + 1..].trim().is_empty());
    if !trailing_only {
        return Err("the assistant returned more than one statement".to_string());
    }

    let verb = sql
        .split(|c: char| c.is_whitespace() || c == '(')
        .find(|w| !w.is_empty())
        .unwrap_or_default()
        .to_ascii_uppercase();
    if verb != "SELECT" && verb != "WITH" {
        return Err(format!(
            "the assistant returned a {verb} statement, not a SELECT"
        ));
    }

    Ok(sql)
}

/// Insert `insert` into `text` at `byte_idx`, clamped to the end and to the
/// nearest character boundary, separating it from its neighbours with a
/// newline where one is not already there.
pub fn splice_at(text: &str, byte_idx: usize, insert: &str) -> String {
    let mut idx = byte_idx.min(text.len());
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    let (before, after) = text.split_at(idx);
    let mut out = String::with_capacity(text.len() + insert.len() + 2);
    out.push_str(before);
    if !before.is_empty() && !before.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(insert);
    if !after.is_empty() && !after.starts_with('\n') {
        out.push('\n');
    }
    out.push_str(after);
    out
}

/// Run one request against `provider` and parse the reply. Blocking: callers
/// run it on a worker thread, like every other network call in the app.
///
/// Takes the already-built prompt rather than the table facts so the argument
/// list stays under clippy's limit without an `#[allow]`; callers pair it with
/// [`build_prompt`].
/// `usage` accumulates what the provider says this request cost. It is an
/// out-parameter rather than part of the return value because the tokens are
/// spent whether or not the reply parses, and the session meter must not
/// under-report a question that failed.
pub fn ask(
    provider: &dyn ChatProvider,
    cfg: &ProviderConfig,
    system: &str,
    question: &str,
    cancel: &AtomicBool,
    usage: &mut (u32, u32),
) -> Result<String, String> {
    let messages = vec![Message::user_text(question)];
    let mut reply = String::new();
    provider.stream_turn(cfg, system, &messages, &[], cancel, &mut |ev| match ev {
        ChatEvent::TextDelta(chunk) => reply.push_str(&chunk),
        ChatEvent::Usage {
            input_tokens,
            output_tokens,
        } => {
            usage.0 = usage.0.saturating_add(input_tokens);
            usage.1 = usage.1.saturating_add(output_tokens);
        }
        _ => {}
    })?;
    if reply.trim().is_empty() {
        return Err("the assistant returned nothing".to_string());
    }
    parse_reply(&reply)
}

#[cfg(test)]
#[path = "ask_sql_tests.rs"]
mod tests;
