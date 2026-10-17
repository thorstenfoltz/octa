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
