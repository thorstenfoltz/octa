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
mod tests {
    use super::*;

    fn cols() -> Vec<ColumnInfo> {
        vec![
            ColumnInfo {
                name: "amount".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "country".into(),
                data_type: "Utf8".into(),
            },
        ]
    }

    /// Facts for a plain local tab, so a test only states what it varies.
    fn facts(table: &str, dialect: &str, related: &str) -> AskSqlFacts {
        AskSqlFacts {
            table_name: table.to_string(),
            dialect: dialect.to_string(),
            columns: cols(),
            row_count: Some(42),
            related: related.to_string(),
            workspace: String::new(),
        }
    }

    #[test]
    fn parses_a_well_formed_reply() {
        let reply = r#"{"sql":"SELECT country FROM data"}"#;
        assert_eq!(parse_reply(reply).unwrap(), "SELECT country FROM data");
    }

    /// Models wrap JSON in prose and fences whatever the prompt says.
    #[test]
    fn tolerates_fenced_and_chatty_replies() {
        let reply = "Sure!\n```json\n{\"sql\":\"SELECT 1 FROM data\"}\n```";
        assert_eq!(parse_reply(reply).unwrap(), "SELECT 1 FROM data");
    }

    #[test]
    fn accepts_a_with_clause() {
        let reply = r#"{"sql":"WITH t AS (SELECT 1) SELECT * FROM t"}"#;
        assert!(parse_reply(reply).is_ok());
    }

    #[test]
    fn allows_one_trailing_semicolon() {
        let reply = r#"{"sql":"SELECT 1 FROM data;"}"#;
        assert_eq!(parse_reply(reply).unwrap(), "SELECT 1 FROM data;");
    }

    /// A semicolon inside a quoted literal is data, not a statement break.
    #[test]
    fn allows_a_semicolon_inside_a_string_literal() {
        let reply = r#"{"sql":"SELECT * FROM data WHERE note = 'a;b'"}"#;
        assert!(parse_reply(reply).is_ok());
    }

    #[test]
    fn rejects_a_second_statement() {
        let reply = r#"{"sql":"SELECT 1 FROM data; DROP TABLE data"}"#;
        let err = parse_reply(reply).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn rejects_a_non_select_verb() {
        for sql in [
            "DELETE FROM data",
            "UPDATE data SET a = 1",
            "INSERT INTO data VALUES (1)",
            "DROP TABLE data",
        ] {
            let reply = format!(r#"{{"sql":"{sql}"}}"#);
            assert!(parse_reply(&reply).is_err(), "accepted: {sql}");
        }
    }

    #[test]
    fn rejects_missing_or_empty_sql() {
        assert!(parse_reply(r#"{"query":"SELECT 1"}"#).is_err());
        assert!(parse_reply(r#"{"sql":""}"#).is_err());
        assert!(parse_reply(r#"{"sql":"   "}"#).is_err());
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(parse_reply("not json at all").is_err());
        assert!(parse_reply("").is_err());
        assert!(parse_reply("{not json}").is_err());
    }

    #[test]
    fn splice_inserts_at_the_index() {
        assert_eq!(splice_at("AB", 1, "X"), "A\nX\nB");
    }

    #[test]
    fn splice_past_the_end_appends() {
        assert_eq!(splice_at("AB", 99, "X"), "AB\nX");
    }

    #[test]
    fn splice_into_empty_text_is_the_insert_alone() {
        assert_eq!(splice_at("", 0, "SELECT 1"), "SELECT 1");
    }

    /// Existing whitespace is not doubled.
    #[test]
    fn splice_does_not_add_a_second_newline() {
        assert_eq!(splice_at("A\n", 2, "X"), "A\nX");
    }

    #[test]
    fn prompt_names_the_columns_types_table_and_dialect() {
        let p = build_prompt(&facts("data", "DuckDB", ""), "revenue per country");
        assert!(p.contains("amount (Int64)"), "{p}");
        assert!(p.contains("country (Utf8)"), "{p}");
        assert!(p.contains("data"), "{p}");
        assert!(p.contains("DuckDB"), "{p}");
        assert!(p.contains("42 rows"), "{p}");
        assert!(p.contains("revenue per country"), "{p}");
    }

    fn col_rows(pairs: &[(&str, &str, &str)]) -> Vec<ColumnRow> {
        pairs
            .iter()
            .map(|(s, t, c)| (s.to_string(), t.to_string(), c.to_string()))
            .collect()
    }

    fn fk(child: &str, ccol: &str, parent: &str, pcol: &str) -> ForeignKey {
        let (cs, ct) = child.split_once('.').unwrap();
        let (ps, pt) = parent.split_once('.').unwrap();
        ForeignKey {
            child_schema: cs.to_string(),
            child_table: ct.to_string(),
            child_column: ccol.to_string(),
            parent_schema: ps.to_string(),
            parent_table: pt.to_string(),
            parent_column: pcol.to_string(),
            constraint: "c".to_string(),
        }
    }

    /// A warehouse that declares nothing must leave the prompt exactly as it
    /// was: this is what keeps the feature additive.
    #[test]
    fn no_foreign_keys_means_no_block_and_no_new_rules() {
        assert_eq!(related_tables_block("s.orders", &[], &[]), "");
        let p = build_prompt(&facts("s.orders", "PostgreSQL", ""), "how many");
        assert!(p.contains("Refer to the table only as s.orders"), "{p}");
        assert!(!p.contains("Join only"), "{p}");
    }

    /// Keys are followed in both directions: the parent a table points at and
    /// the children that point back at it are both one hop away.
    #[test]
    fn both_directions_are_neighbours_with_their_columns() {
        let columns = col_rows(&[
            ("s", "customers", "id"),
            ("s", "customers", "name"),
            ("s", "items", "order_id"),
            ("s", "items", "sku"),
            ("s", "elsewhere", "x"),
        ]);
        let fks = [
            fk("s.orders", "customer_id", "s.customers", "id"),
            fk("s.items", "order_id", "s.orders", "id"),
            fk("s.other", "z", "s.elsewhere", "x"),
        ];
        let b = related_tables_block("s.orders", &columns, &fks);
        assert!(b.contains("s.customers columns: id, name"), "{b}");
        assert!(
            b.contains("join: s.orders.customer_id = s.customers.id"),
            "{b}"
        );
        assert!(b.contains("s.items columns: order_id, sku"), "{b}");
        assert!(b.contains("join: s.items.order_id = s.orders.id"), "{b}");
        // A key between two other tables is not this table's business.
        assert!(!b.contains("elsewhere"), "{b}");
    }

    /// A key pointing at its own table names the join without repeating the
    /// column list the prompt already carries.
    #[test]
    fn a_self_reference_lists_the_join_only() {
        let columns = col_rows(&[("s", "staff", "id"), ("s", "staff", "manager_id")]);
        let fks = [fk("s.staff", "manager_id", "s.staff", "id")];
        let b = related_tables_block("s.staff", &columns, &fks);
        assert!(b.contains("s.staff (this table, joined to itself)"), "{b}");
        assert!(b.contains("join: s.staff.manager_id = s.staff.id"), "{b}");
        assert!(!b.contains("columns:"), "{b}");
    }

    /// The key query filters on the child's schema, the column query on its
    /// own, so a parent living elsewhere comes back nameless. It still names a
    /// real join, so it is kept, but never with an empty column list.
    #[test]
    fn a_neighbour_whose_columns_were_not_read_says_so() {
        let b = related_tables_block(
            "s.orders",
            &col_rows(&[("s", "orders", "country_id")]),
            &[fk("s.orders", "country_id", "ref.countries", "id")],
        );
        assert!(b.contains("ref.countries (columns not read)"), "{b}");
        assert!(!b.contains("columns: \n"), "{b}");
        assert!(
            b.contains("join: s.orders.country_id = ref.countries.id"),
            "{b}"
        );
    }

    #[test]
    fn the_table_and_column_caps_hold_and_say_so() {
        let mut columns = Vec::new();
        let mut fks = Vec::new();
        for t in 0..MAX_RELATED_TABLES + 3 {
            let name = format!("s.t{t:02}");
            fks.push(fk("s.orders", "k", &name, "id"));
            for c in 0..MAX_RELATED_COLUMNS + 5 {
                columns.push(("s".to_string(), format!("t{t:02}"), format!("c{c:02}")));
            }
        }
        let b = related_tables_block("s.orders", &columns, &fks);
        assert_eq!(b.matches("columns:").count(), MAX_RELATED_TABLES, "{b}");
        assert!(
            b.contains("(3 further related tables are not listed.)"),
            "{b}"
        );
        assert!(b.contains("(+5 more)"), "{b}");
    }

    /// With neighbours the prompt gains them plus the rules that keep the
    /// model from inventing a join or double counting through one.
    #[test]
    fn prompt_carries_the_related_block_and_its_rules() {
        let related = related_tables_block(
            "s.orders",
            &col_rows(&[("s", "customers", "id")]),
            &[fk("s.orders", "customer_id", "s.customers", "id")],
        );
        let p = build_prompt(&facts("s.orders", "PostgreSQL", &related), "who spent most");
        assert!(p.contains("s.customers columns: id"), "{p}");
        assert!(p.contains("Join only on a listed key pair"), "{p}");
        assert!(p.contains("A join can multiply rows"), "{p}");
        assert!(!p.contains("Refer to the table only as"), "{p}");
    }

    #[test]
    fn workspace_tables_are_described_and_replace_the_single_table_rule() {
        let ws = workspace_block(&[(
            "wh.public.orders".to_string(),
            vec![
                ("id".to_string(), "BIGINT".to_string()),
                ("total".to_string(), "DOUBLE".to_string()),
            ],
        )]);
        let mut f = facts("data", "DuckDB", "");
        f.workspace = ws;
        let p = build_prompt(&f, "biggest order");
        assert!(p.contains("wh.public.orders"), "{p}");
        assert!(p.contains("total (DOUBLE)"), "{p}");
        assert!(p.contains("Do not invent tables or columns"), "{p}");
        assert!(!p.contains("Refer to the table only as"), "{p}");
    }

    /// The tab that exists only to attach a server has no table of its own.
    /// The prompt must still be answerable, and must not name a `data` that
    /// was never registered.
    #[test]
    fn a_tab_with_no_table_of_its_own_still_gets_a_usable_prompt() {
        let f = AskSqlFacts {
            table_name: "data".to_string(),
            dialect: "DuckDB".to_string(),
            columns: Vec::new(),
            row_count: None,
            related: String::new(),
            workspace: workspace_block(&[(
                "pg.public.users".to_string(),
                vec![("email".to_string(), "VARCHAR".to_string())],
            )]),
        };
        let p = build_prompt(&f, "how many users");
        assert!(p.contains("pg.public.users"), "{p}");
        assert!(!p.contains("The table is named"), "{p}");
        assert!(p.contains("DuckDB"), "{p}");
    }

    /// An unknown row count is omitted rather than guessed: a live database
    /// tab holds one page, and stating a page size as the table size is a lie
    /// the model has no way to detect.
    #[test]
    fn an_unknown_row_count_is_left_out_of_the_prompt() {
        let mut f = facts("s.orders", "PostgreSQL", "");
        f.row_count = None;
        let p = build_prompt(&f, "how many");
        assert!(!p.contains(" rows"), "{p}");
        assert!(
            p.contains("The table is named s.orders and has these columns"),
            "{p}"
        );
    }

    #[test]
    fn an_empty_workspace_leaves_the_prompt_exactly_as_it_was() {
        assert_eq!(workspace_block(&[]), "");
    }
}
