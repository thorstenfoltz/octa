//! Quote an identifier the server refused to read bare, and run again.
//!
//! Databricks (Spark SQL) rejects a name like `my-col` written without
//! quotes: "The unquoted identifier my-col is invalid and must be back quoted
//! as: `my-col`". The error names the fix, so the query is adjusted and re-run
//! instead of sending the user back to the editor.

use super::DbEngine;

/// `sql` with every bare occurrence of the identifier `err` complains about
/// quoted in `engine`'s dialect. `None` when `err` is another kind of error,
/// or the identifier has no bare occurrence left (nothing to fix).
pub fn quote_rejected_identifier(engine: DbEngine, sql: &str, err: &str) -> Option<String> {
    const MARKER: &str = "unquoted identifier ";
    // ASCII lowercasing keeps byte offsets, so the index is valid in `err`.
    let at = err.to_ascii_lowercase().find(MARKER)? + MARKER.len();
    let ident = err[at..]
        .split_whitespace()
        .next()?
        .trim_matches(|c| c == '\'' || c == '"');
    if ident.is_empty() {
        return None;
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let quoted = engine.quote_ident(ident);
    let mut out = String::with_capacity(sql.len() + 8);
    let mut changed = false;
    let mut i = 0;
    while i < sql.len() {
        let rest = &sql[i..];
        // String literals, already-quoted names and comments pass verbatim.
        let skip = match rest.as_bytes()[0] {
            q @ (b'\'' | b'"' | b'`') => rest[1..].find(q as char).map_or(rest.len(), |e| e + 2),
            b'-' if rest.starts_with("--") => rest.find('\n').unwrap_or(rest.len()),
            b'/' if rest.starts_with("/*") => rest.find("*/").map_or(rest.len(), |e| e + 2),
            _ => 0,
        };
        if skip > 0 {
            out.push_str(&rest[..skip]);
            i += skip;
            continue;
        }
        if rest.starts_with(ident)
            && out.chars().last().is_none_or(|c| !is_word(c))
            && !rest[ident.len()..].starts_with(is_word)
        {
            out.push_str(&quoted);
            i += ident.len();
            changed = true;
            continue;
        }
        let c = rest.chars().next().expect("i < len");
        out.push(c);
        i += c.len_utf8();
    }
    changed.then_some(out)
}

/// Run `run(sql)`; while the server rejects an unquoted identifier, quote it
/// and run again. Returns the outcome and the SQL that produced it, so a
/// caller can show the user what was actually sent.
///
/// Terminates: each round only adds quotes, and an identifier with no bare
/// occurrence left yields `None`, which ends the loop with the error.
pub fn with_identifier_fix<T>(
    engine: DbEngine,
    sql: &str,
    mut run: impl FnMut(&str) -> anyhow::Result<T>,
) -> (anyhow::Result<T>, String) {
    let mut sql = sql.to_string();
    loop {
        match run(&sql) {
            Err(e) => match quote_rejected_identifier(engine, &sql, &format!("{e:#}")) {
                Some(fixed) => sql = fixed,
                None => return (Err(e), sql),
            },
            ok => return (ok, sql),
        }
    }
}

#[cfg(test)]
mod tests {
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
}
