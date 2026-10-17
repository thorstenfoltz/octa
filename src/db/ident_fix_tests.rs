use super::*;

const DBX: &str = "[INVALID_IDENTIFIER] The unquoted identifier my-col is invalid and \
        must be back quoted as: `my-col`. Unquoted identifiers can only contain ASCII letters";

#[test]
fn quotes_bare_occurrences_only() {
    let sql = "SELECT my-col, 'my-col', `my-col`, my-col2 FROM t.my-col -- my-col\nWHERE x";
    assert_eq!(
        quote_rejected_identifier(DbEngine::Databricks, sql, DBX).as_deref(),
        Some("SELECT `my-col`, 'my-col', `my-col`, my-col2 FROM t.`my-col` -- my-col\nWHERE x")
    );
}

#[test]
fn other_errors_and_nothing_left_give_none() {
    assert_eq!(
        quote_rejected_identifier(DbEngine::Databricks, "SELECT 1", "syntax error"),
        None
    );
    assert_eq!(
        quote_rejected_identifier(DbEngine::Databricks, "SELECT `my-col`", DBX),
        None
    );
}

#[test]
fn retries_until_the_server_accepts() {
    let mut calls = 0;
    let (res, sent) = with_identifier_fix(DbEngine::Databricks, "SELECT my-col FROM t", |s| {
        calls += 1;
        if s.contains("`my-col`") {
            Ok(())
        } else {
            anyhow::bail!("{DBX}")
        }
    });
    assert!(res.is_ok());
    assert_eq!(sent, "SELECT `my-col` FROM t");
    assert_eq!(calls, 2);
}
