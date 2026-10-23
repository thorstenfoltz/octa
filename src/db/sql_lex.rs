//! The code of a SQL text with its strings, quoted names and comments taken
//! out, read the way one engine reads them, so a read-only check can look for
//! a word in what will run without tripping over the same word in
//! `WHERE note = 'commit'` or `-- into staging later`.
//!
//! Every quoted section comes back as its delimiters with nothing between
//! (`''`, `""`, `` `` ``, `[]`) and every comment as one space. Where the
//! engine's reading depends on something the text does not say, there is no
//! answer (`None`) and the caller falls back to counting words in the whole
//! text: a backslash (an escape or not, depending on a setting or a raw
//! string), a dollar sign (dollar quoting), a nested comment where the engine
//! may or may not nest, an unclosed quote or comment.

/// The engines whose lexing a read-only check needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    /// Databricks: `'` and `"` strings, backtick names, backslash escapes.
    Spark,
    /// SQL Server: `'` strings, `"` and `[..]` names (`]]` inside), nested
    /// block comments, no escapes.
    TSql,
    /// Amazon Redshift: `'` strings, `"` names, backslash escapes, dollar
    /// quoting.
    Redshift,
}

/// `sql` with strings, quoted names and comments emptied, or `None` when
/// `dialect` could read it more than one way.
pub(crate) fn code_only(sql: &str, dialect: Dialect) -> Option<String> {
    if sql.contains('\\') && dialect != Dialect::TSql {
        return None;
    }
    if sql.contains('$') && dialect == Dialect::Redshift {
        return None;
    }
    let mut out = String::with_capacity(sql.len());
    let mut rest = sql;
    while let Some(c) = rest.chars().next() {
        let quote = match c {
            '\'' | '"' => Some(c),
            '`' if dialect == Dialect::Spark => Some(c),
            '[' if dialect == Dialect::TSql => Some(']'),
            _ => None,
        };
        if let Some(close) = quote {
            // A doubled quote inside is an escape in some dialects and two
            // adjacent literals in others; either way the text stays quoted,
            // so pairing quotes in order reads the same. Only `]]` needs care.
            let body = &rest[1..];
            let mut end = body.find(close)?;
            while close == ']' && body[end + 1..].starts_with(']') {
                end += 2 + body[end + 2..].find(']')?;
            }
            out.push(c);
            out.push(close);
            rest = &body[end + 1..];
        } else if rest.starts_with("--") {
            out.push(' ');
            rest = rest.find('\n').map_or("", |n| &rest[n..]);
        } else if rest.starts_with("/*") {
            out.push(' ');
            rest = skip_block_comment(rest, dialect)?;
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    Some(out)
}

/// `sql` after the block comment it starts with. SQL Server nests them, so
/// its comments are counted open and closed; for the others a `/*` inside a
/// comment has no single reading.
fn skip_block_comment(sql: &str, dialect: Dialect) -> Option<&str> {
    let mut depth = 0usize;
    let mut i = 0;
    while i < sql.len() {
        if sql[i..].starts_with("/*") {
            depth += 1;
            if depth > 1 && dialect != Dialect::TSql {
                return None;
            }
            i += 2;
        } else if sql[i..].starts_with("*/") {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return Some(&sql[i..]);
            }
        } else {
            i += sql[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    None
}

/// Whether one of `words` (upper case) runs in `sql`: in its code when
/// `dialect` reads it one way, anywhere in it when not. A word is a run of
/// letters, digits and `_` (plus `@`, `#` and `$` on SQL Server, so `@exec`
/// is a variable); leading digits are dropped, since `1EXEC` may lex as a
/// number and then the word.
pub(crate) fn has_code_word(sql: &str, dialect: Dialect, words: &[&str]) -> bool {
    let code = code_only(sql, dialect);
    let text = code.as_deref().unwrap_or(sql);
    let in_word =
        |c: char| c.is_alphanumeric() || c == '_' || dialect == Dialect::TSql && "@#$".contains(c);
    text.split(|c: char| !in_word(c)).any(|w| {
        let w = w.trim_start_matches(|c: char| c.is_ascii_digit());
        words.iter().any(|x| w.eq_ignore_ascii_case(x))
    })
}

#[cfg(test)]
#[path = "sql_lex_tests.rs"]
mod tests;
