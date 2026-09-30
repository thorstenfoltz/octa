//! **Format SQL**: lay a query out one clause per line, in the style the user
//! picked under Settings -> SQL. The tokenising and line breaking are
//! `sqlformat`'s; this module maps Octa's settings onto it and adds the one
//! style it lacks, leading commas.

use serde::{Deserialize, Serialize};

/// How keywords (`SELECT`, `from`, `Join`) come out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum KeywordCase {
    #[default]
    Upper,
    Lower,
    /// Leave every keyword as it was typed.
    AsWritten,
}

/// What one indentation step is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum IndentStyle {
    TwoSpaces,
    #[default]
    FourSpaces,
    Tab,
}

/// Where the comma between two list items goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CommaStyle {
    /// `a,` then `b` on the next line.
    #[default]
    Trailing,
    /// `a` then `, b` on the next line.
    Leading,
}

/// The dialect decides only what counts as one token: `[a b]` is a quoted
/// name on SQL Server and an array elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormatDialect {
    #[default]
    Generic,
    /// Postgres, Redshift and DuckDB (the local workspace).
    Postgres,
    SqlServer,
}

/// The user's formatting style, stored in `settings.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SqlFormatOptions {
    pub keyword_case: KeywordCase,
    pub indent: IndentStyle,
    pub commas: CommaStyle,
    /// `JOIN` starts a clause of its own, level with `FROM`, instead of being
    /// indented under it.
    pub joins_top_level: bool,
    /// A list shorter than this many characters stays on one line
    /// (`SELECT a, b, c`). `0` puts every item on a line of its own.
    pub inline_width: usize,
    /// Empty lines between two statements.
    pub blank_lines_between: u8,
    /// A clause whose body is shorter than this stays on its keyword's line
    /// (`GROUP BY c.name`, a short subquery in brackets). `None` follows
    /// `inline_width`, which set both before the two were separate. Applies
    /// only while `inline_width` is above 0.
    pub clause_width: Option<usize>,
    /// A bracket shorter than this stays on one line (`coalesce(a, 0)`,
    /// `IN (1, 2, 3)`); longer ones put each item on a line of its own.
    pub bracket_width: usize,
    /// The whole statement on one line: tidies spacing and keyword case and
    /// breaks nothing. The widths above do not apply.
    pub one_line: bool,
    /// End the text with `;` when the last statement has none.
    pub final_semicolon: bool,
}

impl Default for SqlFormatOptions {
    fn default() -> Self {
        Self {
            keyword_case: KeywordCase::Upper,
            indent: IndentStyle::FourSpaces,
            commas: CommaStyle::Trailing,
            joins_top_level: false,
            inline_width: 0,
            blank_lines_between: 1,
            clause_width: None,
            bracket_width: 50,
            one_line: false,
            final_semicolon: false,
        }
    }
}

impl SqlFormatOptions {
    /// The clause width in effect: its own, or the list width's.
    pub fn effective_clause_width(&self) -> usize {
        self.clause_width.unwrap_or(self.inline_width)
    }
}

impl KeywordCase {
    pub const ALL: &[Self] = &[Self::Upper, Self::Lower, Self::AsWritten];
    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::Upper => "enum.sqlfmt_upper",
            Self::Lower => "enum.sqlfmt_lower",
            Self::AsWritten => "enum.sqlfmt_as_written",
        })
    }
}

impl IndentStyle {
    pub const ALL: &[Self] = &[Self::TwoSpaces, Self::FourSpaces, Self::Tab];
    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::TwoSpaces => "enum.sqlfmt_two_spaces",
            Self::FourSpaces => "enum.sqlfmt_four_spaces",
            Self::Tab => "enum.sqlfmt_tab",
        })
    }
}

impl CommaStyle {
    pub const ALL: &[Self] = &[Self::Trailing, Self::Leading];
    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::Trailing => "enum.sqlfmt_trailing",
            Self::Leading => "enum.sqlfmt_leading",
        })
    }
}

/// Format `sql` in the given style. Comments are kept; only whitespace,
/// keyword case and comma placement change, so the statement means the same.
pub fn format_sql(sql: &str, opts: &SqlFormatOptions, dialect: FormatDialect) -> String {
    let options = sqlformat::FormatOptions {
        indent: match opts.indent {
            IndentStyle::TwoSpaces => sqlformat::Indent::Spaces(2),
            IndentStyle::FourSpaces => sqlformat::Indent::Spaces(4),
            IndentStyle::Tab => sqlformat::Indent::Tabs,
        },
        uppercase: match opts.keyword_case {
            KeywordCase::Upper => Some(true),
            KeywordCase::Lower => Some(false),
            KeywordCase::AsWritten => None,
        },
        // sqlformat counts line breaks, the setting counts empty lines.
        lines_between_queries: opts.blank_lines_between.saturating_add(1),
        max_inline_arguments: (opts.inline_width > 0).then_some(opts.inline_width),
        // Only with a list width: alone, sqlformat leaves a clause's items
        // unindented at column 0 (`SELECT a,\nb,\nc`).
        max_inline_top_level: {
            let w = opts.effective_clause_width();
            (w > 0 && opts.inline_width > 0).then_some(w)
        },
        max_inline_block: opts.bracket_width,
        inline: opts.one_line,
        joins_as_top_level: opts.joins_top_level,
        dialect: match dialect {
            FormatDialect::Generic => sqlformat::Dialect::Generic,
            FormatDialect::Postgres => sqlformat::Dialect::PostgreSql,
            FormatDialect::SqlServer => sqlformat::Dialect::SQLServer,
        },
        ..Default::default()
    };
    let out = sqlformat::format(sql, &sqlformat::QueryParams::None, &options);
    let out = match opts.commas {
        // One line has no line starts to move commas to.
        CommaStyle::Leading if !opts.one_line => move_commas_to_line_start(&out),
        _ => out,
    };
    if opts.final_semicolon {
        add_final_semicolon(&out)
    } else {
        out
    }
}

/// Append `;` unless the text already ends with one. After a trailing `--`
/// comment it goes on a line of its own, or the comment would swallow it.
fn add_final_semicolon(sql: &str) -> String {
    let body = sql.trim_end();
    if body.is_empty() || body.ends_with(';') {
        return sql.to_string();
    }
    let last_line = body.lines().last().unwrap_or("");
    let sep = if last_line.contains("--") { "\n" } else { "" };
    format!("{body}{sep};{}", &sql[body.len()..])
}

/// Turn `a,\n    b` into `a\n    , b`. Only a comma that ends a line moves,
/// and a line holding a `--` comment keeps its comma: there the comma may be
/// part of the comment text.
// ponytail: a line ending in a comma inside a multi-line string literal would
// be moved too. sqlformat never breaks inside a literal, so only a literal
// the user wrote across lines can hit it; a tokenizer pass fixes it if needed.
fn move_commas_to_line_start(sql: &str) -> String {
    let lines: Vec<&str> = sql.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut carry = false;
    for (i, line) in lines.iter().enumerate() {
        let mut line = (*line).to_string();
        if carry {
            let body = line.trim_start();
            let indent = &line[..line.len() - body.len()];
            line = format!("{indent}, {body}");
            carry = false;
        }
        let has_next = lines.get(i + 1).is_some_and(|n| !n.trim().is_empty());
        if has_next && !line.contains("--") && line.trim_end().ends_with(',') {
            let trimmed = line.trim_end();
            line = trimmed[..trimmed.len() - 1].to_string();
            carry = true;
        }
        out.push(line);
    }
    let mut joined = out.join("\n");
    if sql.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
