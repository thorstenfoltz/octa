//! One-shot "turn this sentence into filters" request.
//!
//! Deliberately not the agent loop: no tools, no follow-up, one request and
//! one reply, so typing in the search box can never turn into an autonomous
//! session. The reply is parsed into ordinary filters the user can then edit
//! or delete by hand, which is what makes a wrong guess repairable rather
//! than mysterious.

use std::sync::atomic::AtomicBool;

use octa::data::ColumnInfo;
use octa::data::conditional_format::CondOp;
use octa::data::predicate_filter::PredicateFilter;

use super::providers::{ChatProvider, ProviderConfig};
use super::types::{ChatEvent, Message};

/// What the model asked for, resolved against the real columns.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AskResult {
    /// Comparisons (`amount > 1000`).
    pub predicates: Vec<PredicateFilter>,
    /// Categorical constraints, as (column index, allowed values).
    pub values: Vec<(usize, Vec<String>)>,
    /// Optional sort: (column index, ascending).
    pub sort: Option<(usize, bool)>,
}

/// The instruction sent as the system prompt. Short on purpose: one job, one
/// output shape.
pub fn build_prompt(columns: &[ColumnInfo], row_count: usize, question: &str) -> String {
    let cols = columns
        .iter()
        .map(|c| format!("- {} ({})", c.name, c.data_type))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "You turn a question about a table into filters. Reply with JSON only, no prose.\n\
         \n\
         The table has {row_count} rows and these columns:\n{cols}\n\
         \n\
         Reply shape:\n\
         {{\"predicates\":[{{\"column\":\"NAME\",\"op\":\"OP\",\"value\":\"V\"}}],\n\
         \x20 \"values\":[{{\"column\":\"NAME\",\"allowed\":[\"A\",\"B\"]}}],\n\
         \x20 \"sort\":{{\"column\":\"NAME\",\"ascending\":true}}}}\n\
         \n\
         OP is one of: = != contains !contains > < >= <= empty !empty\n\
         Use \"predicates\" for comparisons, \"values\" for membership in a set.\n\
         Omit \"sort\" unless the question asks for an order.\n\
         Use only the column names listed above, spelled exactly as given.\n\
         \n\
         Question: {question}"
    )
}

fn op_from_str(s: &str) -> Option<CondOp> {
    Some(match s.trim() {
        "=" | "==" | "eq" => CondOp::Eq,
        "!=" | "<>" | "ne" => CondOp::Ne,
        "contains" => CondOp::Contains,
        "!contains" | "not contains" => CondOp::NotContains,
        ">" | "gt" => CondOp::Gt,
        "<" | "lt" => CondOp::Lt,
        ">=" | "ge" => CondOp::Ge,
        "<=" | "le" => CondOp::Le,
        "empty" => CondOp::Empty,
        "!empty" | "not empty" => CondOp::NotEmpty,
        _ => return None,
    })
}

/// Pull the JSON object out of a reply that may be fenced or prefixed with
/// chat, then resolve every column name against the real schema.
///
/// Every failure is an error rather than a partial result: applying half of a
/// misunderstood sentence is worse than applying none of it.
pub fn parse_reply(reply: &str, columns: &[ColumnInfo]) -> Result<AskResult, String> {
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

    let index_of = |name: &str| -> Result<usize, String> {
        columns
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("the assistant named a column that does not exist: {name}"))
    };

    let mut out = AskResult::default();
    let empty = Vec::new();

    for p in v
        .get("predicates")
        .and_then(|x| x.as_array())
        .unwrap_or(&empty)
    {
        let name = p.get("column").and_then(|x| x.as_str()).unwrap_or_default();
        let op_str = p.get("op").and_then(|x| x.as_str()).unwrap_or_default();
        let op = op_from_str(op_str)
            .ok_or_else(|| format!("the assistant used an unknown operator: {op_str}"))?;
        out.predicates.push(PredicateFilter {
            col: index_of(name)?,
            op,
            // Numbers arrive either quoted or bare depending on the model.
            value: match p.get("value") {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(other) if !other.is_null() => other.to_string(),
                _ => String::new(),
            },
            case_sensitive: false,
        });
    }

    for entry in v.get("values").and_then(|x| x.as_array()).unwrap_or(&empty) {
        let name = entry
            .get("column")
            .and_then(|x| x.as_str())
            .unwrap_or_default();
        let allowed: Vec<String> = entry
            .get("allowed")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        if allowed.is_empty() {
            continue;
        }
        out.values.push((index_of(name)?, allowed));
    }

    if let Some(sort) = v.get("sort").filter(|s| s.is_object()) {
        let name = sort
            .get("column")
            .and_then(|x| x.as_str())
            .unwrap_or_default();
        let ascending = sort
            .get("ascending")
            .and_then(|x| x.as_bool())
            .unwrap_or(true);
        out.sort = Some((index_of(name)?, ascending));
    }

    if out.predicates.is_empty() && out.values.is_empty() && out.sort.is_none() {
        return Err("the assistant did not produce any filter".to_string());
    }
    Ok(out)
}

/// Run one request against `provider` and parse the reply. Blocking: callers
/// run it on a worker thread, like every other network call in the app.
pub fn ask(
    provider: &dyn ChatProvider,
    cfg: &ProviderConfig,
    columns: &[ColumnInfo],
    row_count: usize,
    question: &str,
    cancel: &AtomicBool,
) -> Result<AskResult, String> {
    let system = build_prompt(columns, row_count, question);
    let messages = vec![Message::user_text(question)];
    let mut reply = String::new();
    provider.stream_turn(cfg, &system, &messages, &[], cancel, &mut |ev| {
        if let ChatEvent::TextDelta(chunk) = ev {
            reply.push_str(&chunk);
        }
    })?;
    if reply.trim().is_empty() {
        return Err("the assistant returned nothing".to_string());
    }
    parse_reply(&reply, columns)
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
        let reply = r#"{"predicates":[{"column":"amount","op":">","value":"1000"}],
                        "values":[{"column":"country","allowed":["DE"]}],
                        "sort":{"column":"amount","ascending":false}}"#;
        let got = parse_reply(reply, &cols()).unwrap();
        assert_eq!(got.predicates.len(), 1);
        assert_eq!(got.predicates[0].col, 0);
        assert_eq!(got.predicates[0].op, CondOp::Gt);
        assert_eq!(got.values, vec![(1, vec!["DE".to_string()])]);
        assert_eq!(got.sort, Some((0, false)));
    }

    /// Models wrap JSON in prose and fences whatever the prompt says.
    #[test]
    fn tolerates_fenced_and_chatty_replies() {
        let reply = "Sure!\n```json\n{\"predicates\":[{\"column\":\"amount\",\"op\":\">=\",\"value\":\"5\"}]}\n```";
        let got = parse_reply(reply, &cols()).unwrap();
        assert_eq!(got.predicates.len(), 1);
        assert_eq!(got.predicates[0].op, CondOp::Ge);
    }

    /// A bare JSON number is as likely as a quoted one.
    #[test]
    fn accepts_unquoted_numbers() {
        let reply = r#"{"predicates":[{"column":"amount","op":">","value":1000}]}"#;
        let got = parse_reply(reply, &cols()).unwrap();
        assert_eq!(got.predicates[0].value, "1000");
    }

    #[test]
    fn unknown_column_is_an_error_not_a_guess() {
        let reply = r#"{"predicates":[{"column":"revenue","op":">","value":"1"}]}"#;
        let err = parse_reply(reply, &cols()).unwrap_err();
        assert!(err.contains("revenue"), "{err}");
    }

    #[test]
    fn unknown_operator_is_an_error() {
        let reply = r#"{"predicates":[{"column":"amount","op":"~=","value":"1"}]}"#;
        assert!(parse_reply(reply, &cols()).is_err());
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_reply("not json at all", &cols()).is_err());
        assert!(parse_reply("", &cols()).is_err());
        assert!(parse_reply("{not json}", &cols()).is_err());
    }

    /// An empty result would silently do nothing, which reads as a bug.
    #[test]
    fn empty_result_is_an_error() {
        let err = parse_reply(r#"{"predicates":[],"values":[]}"#, &cols()).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn prompt_lists_the_columns_and_their_types() {
        let p = build_prompt(&cols(), 42, "big German orders");
        assert!(p.contains("amount (Int64)"), "{p}");
        assert!(p.contains("country (Utf8)"), "{p}");
        assert!(p.contains("42 rows"), "{p}");
        assert!(p.contains("big German orders"), "{p}");
    }
}
