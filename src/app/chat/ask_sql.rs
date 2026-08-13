//! One-shot "turn this sentence into a query" request for the SQL panel.
//!
//! Deliberately not the agent loop: no tools, no follow-up, one request and
//! one reply, so a text box in a panel can never become an autonomous
//! session. The answer lands in the editor and is never run, and the parser
//! refuses anything that is not a single SELECT, so a plain-language box
//! cannot hand back a DELETE to fire off by reflex.

use std::sync::atomic::AtomicBool;

use octa::data::ColumnInfo;

use super::providers::{ChatProvider, ProviderConfig};
use super::types::{ChatEvent, Message};

/// The instruction sent as the system prompt. Short on purpose: one job, one
/// output shape.
pub fn build_prompt(
    table_name: &str,
    dialect: &str,
    columns: &[ColumnInfo],
    row_count: usize,
    question: &str,
) -> String {
    let cols = columns
        .iter()
        .map(|c| format!("- {} ({})", c.name, c.data_type))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "You turn a question about a table into one SQL query. Reply with JSON \
         only, no prose.\n\
         \n\
         The SQL dialect is {dialect}. The table is named {table_name}, has \
         {row_count} rows and these columns:\n{cols}\n\
         \n\
         Reply shape:\n\
         {{\"sql\":\"SELECT ...\"}}\n\
         \n\
         Rules:\n\
         - Exactly one statement. Do not write two statements.\n\
         - It must be a SELECT (a leading WITH clause is fine).\n\
         - Never write INSERT, UPDATE, DELETE, DROP, CREATE or ALTER.\n\
         - Use only the column names listed above, spelled exactly as given.\n\
         - Refer to the table only as {table_name}.\n\
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
pub fn ask(
    provider: &dyn ChatProvider,
    cfg: &ProviderConfig,
    system: &str,
    question: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let messages = vec![Message::user_text(question)];
    let mut reply = String::new();
    provider.stream_turn(cfg, system, &messages, &[], cancel, &mut |ev| {
        if let ChatEvent::TextDelta(chunk) = ev {
            reply.push_str(&chunk);
        }
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
        let p = build_prompt("data", "DuckDB", &cols(), 42, "revenue per country");
        assert!(p.contains("amount (Int64)"), "{p}");
        assert!(p.contains("country (Utf8)"), "{p}");
        assert!(p.contains("data"), "{p}");
        assert!(p.contains("DuckDB"), "{p}");
        assert!(p.contains("42 rows"), "{p}");
        assert!(p.contains("revenue per country"), "{p}");
    }
}
