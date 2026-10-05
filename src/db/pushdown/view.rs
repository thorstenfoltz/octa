//! A database tab's view: its sort and its row filters, spelled as the
//! `ORDER BY` and `WHERE` of the query that pages the tab. A partial
//! database tab then sorts and filters the whole table, not the rows it
//! happens to hold.
//!
//! Each filter mirrors the in-memory test in `find_replace::recompute_filter`,
//! which reads a cell as its text (`CellValue::to_string`, `""` for NULL).
//! A filter an engine cannot spell exactly is never built: the constructors
//! return `None` and the app keeps that part on the loaded rows, with a note.
//!
//! Known differences: SQL Server's `=`, `IN` and `<> ''` ignore trailing
//! spaces, so a value filter `b2` also keeps `b2 ` and Empty also keeps cells
//! of only spaces there. SQL `%` and `_` match line breaks, while Octa's
//! wildcard `.` does not.

use crate::data::conditional_format::CondOp;
use crate::data::is_numeric_data_type;
use crate::db::DbEngine;

use super::dialect::{exact, float_lit, lower, present, str_lit, subquery_alias, text};
use super::hash::{ServerHash, select_with};

/// Oracle refuses an `IN` list over 1000 entries (ORA-01795).
const MAX_IN_LIST: usize = 1000;

/// One sort key, by column name. `text` marks a text column, which Octa
/// sorts ignoring case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortKey {
    pub column: String,
    pub ascending: bool,
    pub text: bool,
}

/// One row filter, by column name.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewFilter {
    /// The cell's text is one of `values`; `""` stands for an empty or
    /// missing cell. Sorted and deduplicated, so equal sets compare equal.
    Values { column: String, values: Vec<String> },
    /// A comparison chip. Built only through [`ViewFilter::compare`].
    Compare {
        column: String,
        op: CondOp,
        value: String,
        case_sensitive: bool,
    },
    /// The search box in Plain mode: any of `columns` contains `needle`.
    Contains {
        columns: Vec<String>,
        needle: String,
        case_sensitive: bool,
    },
    /// The search box in Wildcard mode: any of `columns` matches `pattern`
    /// (`*` any run, `?` one character, `\*` and `\?` literal) anywhere.
    Wildcard {
        columns: Vec<String>,
        pattern: String,
        case_sensitive: bool,
    },
}

/// What a database tab's query adds to `SELECT * FROM table`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServerView {
    pub order: Vec<SortKey>,
    pub filters: Vec<ViewFilter>,
    /// Hash columns the database adds after the table's own; the filters
    /// and the sort can name them.
    pub derived: Vec<ServerHash>,
}

/// A text column, which Octa sorts ignoring case and the sort keys fold.
pub fn is_text_type(data_type: &str) -> bool {
    matches!(data_type, "Utf8" | "LargeUtf8")
}

impl ServerView {
    pub fn is_empty(&self) -> bool {
        self.order.is_empty() && self.filters.is_empty() && self.derived.is_empty()
    }

    /// The `FROM` target for the table `table_sql`: the table itself, or with
    /// hash columns the table plus them under an alias.
    pub fn from_item(&self, engine: DbEngine, table_sql: &str) -> String {
        if self.derived.is_empty() {
            return table_sql.to_string();
        }
        format!(
            "({}){}",
            select_with(engine, table_sql, &self.derived),
            subquery_alias(engine, "octa_d")
        )
    }

    /// The `WHERE` condition without the keyword; `None` with no filters.
    pub fn where_sql(&self, engine: DbEngine) -> Option<String> {
        if self.filters.is_empty() {
            return None;
        }
        Some(
            self.filters
                .iter()
                .map(|f| format!("({})", f.sql(engine)))
                .collect::<Vec<_>>()
                .join(" AND "),
        )
    }

    /// The `ORDER BY` list without the keywords; `""` with no sort keys.
    /// `tie_break` (the table's key columns) makes the order total, so a row
    /// cannot move from one page to the next.
    pub fn order_sql(&self, engine: DbEngine, tie_break: &[String]) -> String {
        if self.order.is_empty() {
            return String::new();
        }
        let mut parts: Vec<String> = self.order.iter().map(|k| order_term(engine, k)).collect();
        for col in tie_break {
            if !self.order.iter().any(|k| &k.column == col) {
                parts.push(engine.quote_ident(col));
            }
        }
        parts.join(", ")
    }
}

/// One page of `view` over the `FROM` target `from`: `limit` rows after the
/// first `offset`. The `ORDER BY` sits at the top level, never in a derived
/// table: SQL Server refuses that, and no engine promises to keep a derived
/// table's order.
pub fn page_sql(
    engine: DbEngine,
    view: &ServerView,
    from: &str,
    tie_break: &[String],
    limit: usize,
    offset: usize,
) -> String {
    let mut sql = format!("SELECT * FROM {from}");
    if let Some(w) = view.where_sql(engine) {
        sql.push_str(&format!(" WHERE {w}"));
    }
    let order = view.order_sql(engine, tie_break);
    let order_by = if order.is_empty() {
        String::new()
    } else {
        format!(" ORDER BY {order}")
    };
    match engine {
        // OFFSET ... FETCH needs an ORDER BY; a no-op one when unsorted.
        DbEngine::Mssql => {
            let order_by = if order.is_empty() {
                " ORDER BY (SELECT NULL)".to_string()
            } else {
                order_by
            };
            format!("{sql}{order_by} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY")
        }
        DbEngine::Oracle => {
            format!("{sql}{order_by} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY")
        }
        DbEngine::Trino | DbEngine::Athena => {
            format!("{sql}{order_by} OFFSET {offset} LIMIT {limit}")
        }
        _ => format!("{sql}{order_by} LIMIT {limit} OFFSET {offset}"),
    }
}

/// Octa's order for one key: NULL first ascending and last descending, text
/// ignoring case and compared byte for byte. MySQL and SQL Server already
/// put NULL lowest and have no `NULLS` keyword.
fn order_term(e: DbEngine, k: &SortKey) -> String {
    let q = e.quote_ident(&k.column);
    let expr = if k.text {
        byte_order(e, &lower(e, &text(e, &q)))
    } else {
        q
    };
    let dir = if k.ascending { "ASC" } else { "DESC" };
    match e {
        DbEngine::MySql | DbEngine::Mssql => format!("{expr} {dir}"),
        _ => format!(
            "{expr} {dir} NULLS {}",
            if k.ascending { "FIRST" } else { "LAST" }
        ),
    }
}

/// Byte order for a sort: Postgres sorts by the database's locale otherwise.
fn byte_order(e: DbEngine, expr: &str) -> String {
    match e {
        DbEngine::Postgres => format!("{expr} COLLATE \"C\""),
        // The session's NLS_SORT can be linguistic.
        DbEngine::Oracle => format!("NLSSORT({expr}, 'NLS_SORT=BINARY')"),
        _ => exact(e, expr),
    }
}

impl ViewFilter {
    /// A value filter. An empty set allows nothing and hides every row, as
    /// in memory (the Column Filter window's Select none, then Apply).
    pub fn values(column: &str, allowed: impl IntoIterator<Item = String>) -> Self {
        let mut values: Vec<String> = allowed.into_iter().collect();
        values.sort();
        values.dedup();
        ViewFilter::Values {
            column: column.to_string(),
            values,
        }
    }

    /// A comparison chip on a column of type `data_type`. An ordering
    /// compare goes to the server only for a numeric column and a numeric
    /// value: in memory each cell falls back to comparing text when it does
    /// not parse, and text order follows the server's collation. Anything
    /// else is `None` and stays on the loaded rows.
    pub fn compare(
        column: &str,
        data_type: &str,
        op: CondOp,
        value: &str,
        case_sensitive: bool,
    ) -> Option<Self> {
        let ordering = matches!(op, CondOp::Gt | CondOp::Lt | CondOp::Ge | CondOp::Le);
        let numeric = is_numeric_data_type(data_type)
            && value.trim().parse::<f64>().is_ok_and(f64::is_finite);
        if ordering && !numeric {
            return None;
        }
        Some(ViewFilter::Compare {
            column: column.to_string(),
            op,
            value: value.to_string(),
            case_sensitive,
        })
    }

    fn sql(&self, e: DbEngine) -> String {
        match self {
            ViewFilter::Values { column, values } => values_sql(e, &e.quote_ident(column), values),
            ViewFilter::Compare {
                column,
                op,
                value,
                case_sensitive,
            } => compare_sql(e, &e.quote_ident(column), *op, value, *case_sensitive),
            ViewFilter::Contains {
                columns,
                needle,
                case_sensitive,
            } => any_column(e, columns, |q| contains_sql(e, q, needle, *case_sensitive)),
            ViewFilter::Wildcard {
                columns,
                pattern,
                case_sensitive,
            } => any_column(e, columns, |q| like_sql(e, q, pattern, *case_sensitive)),
        }
    }
}

/// Any of `columns` passes; no column at all hides every row, as the
/// in-memory search over an empty scope does.
fn any_column(e: DbEngine, columns: &[String], one: impl Fn(&str) -> String) -> String {
    if columns.is_empty() {
        return "1 = 0".to_string();
    }
    columns
        .iter()
        .map(|c| format!("({})", one(&e.quote_ident(c))))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// Empty or missing: the cell Octa reads as `""`.
fn blank(e: DbEngine, q: &str) -> String {
    format!("NOT ({})", present(e, q))
}

/// The cell's text, lower-cased unless `case_sensitive`, compared byte for
/// byte.
fn folded(e: DbEngine, q: &str, case_sensitive: bool) -> String {
    exact(e, &fold(e, q, case_sensitive))
}

/// The cell's text, lower-cased unless `case_sensitive`; no byte wrapper.
fn fold(e: DbEngine, q: &str, case_sensitive: bool) -> String {
    let t = text(e, q);
    if case_sensitive { t } else { lower(e, &t) }
}

/// `v` folded like [`folded`], as a literal.
fn folded_lit(e: DbEngine, v: &str, case_sensitive: bool) -> String {
    exact(e, &fold_lit(e, v, case_sensitive))
}

fn fold_lit(e: DbEngine, v: &str, case_sensitive: bool) -> String {
    let v = if case_sensitive {
        v.to_string()
    } else {
        v.to_lowercase()
    };
    str_lit(e, &v)
}

/// What `LIKE` compares on. MySQL's `exact` is a binary string, where `_`
/// matches one byte, so there the text is compared per character under a
/// case and accent sensitive collation instead.
fn like_exact(e: DbEngine, t: &str) -> String {
    match e {
        DbEngine::MySql => format!("CONVERT({t} USING utf8mb4) COLLATE utf8mb4_0900_bin"),
        _ => exact(e, t),
    }
}

fn values_sql(e: DbEngine, q: &str, values: &[String]) -> String {
    let t = exact(e, &text(e, q));
    let listed: Vec<&String> = values.iter().filter(|v| !v.is_empty()).collect();
    let mut parts: Vec<String> = listed
        .chunks(MAX_IN_LIST)
        .map(|chunk| {
            let items: Vec<String> = chunk.iter().map(|v| exact(e, &str_lit(e, v))).collect();
            format!("{t} IN ({})", items.join(", "))
        })
        .collect();
    if values.iter().any(String::is_empty) {
        parts.push(blank(e, q));
    }
    if parts.is_empty() {
        return "1 = 0".into();
    }
    parts.join(" OR ")
}

/// `rule_matches` on the cell's text. A missing cell reads as `""`, which is
/// not a number, so in memory it compares as text: `"" < "1000"` holds and
/// `"" > "1000"` does not. Hence `IS NULL OR` on the `<` side only.
fn compare_sql(e: DbEngine, q: &str, op: CondOp, value: &str, cs: bool) -> String {
    let number = || float_lit(value.trim().parse::<f64>().unwrap_or(0.0));
    match op {
        CondOp::Empty => blank(e, q),
        CondOp::NotEmpty => present(e, q),
        CondOp::Gt => format!("{q} > {}", number()),
        CondOp::Ge => format!("{q} >= {}", number()),
        CondOp::Lt => format!("{q} IS NULL OR {q} < {}", number()),
        CondOp::Le => format!("{q} IS NULL OR {q} <= {}", number()),
        CondOp::Eq if value.is_empty() => blank(e, q),
        CondOp::Ne if value.is_empty() => present(e, q),
        CondOp::Eq => format!("{} = {}", folded(e, q, cs), folded_lit(e, value, cs)),
        CondOp::Ne => format!(
            "{q} IS NULL OR {} <> {}",
            folded(e, q, cs),
            folded_lit(e, value, cs)
        ),
        CondOp::Contains if value.is_empty() => "1 = 1".to_string(),
        CondOp::NotContains if value.is_empty() => "1 = 0".to_string(),
        CondOp::Contains => contains_sql(e, q, value, cs),
        CondOp::NotContains => format!("{q} IS NULL OR NOT ({})", contains_sql(e, q, value, cs)),
    }
}

/// The cell's text contains `needle` (never empty here). A position test,
/// not `LIKE`, so no character in the needle needs escaping.
fn contains_sql(e: DbEngine, q: &str, needle: &str, cs: bool) -> String {
    if needle.is_empty() {
        return "1 = 1".to_string();
    }
    let h = folded(e, q, cs);
    let n = folded_lit(e, needle, cs);
    match e {
        DbEngine::Mssql => format!("CHARINDEX({n}, {h}) > 0"),
        DbEngine::Oracle => format!("INSTR({h}, {n}) > 0"),
        DbEngine::BigQuery => format!("STRPOS({h}, {n}) > 0"),
        DbEngine::ClickHouse => format!("position({h}, {n}) > 0"),
        _ => format!("POSITION({n} IN {h}) > 0"),
    }
}

/// The Wildcard search as `LIKE`, matching anywhere like the unanchored
/// regex `wildcard_to_regex` builds.
fn like_sql(e: DbEngine, q: &str, pattern: &str, cs: bool) -> String {
    // A missing cell reads as "" in memory, which `.*` matches; `NULL LIKE`
    // is never true.
    if !pattern.is_empty() && pattern.chars().all(|c| c == '*') {
        return "1 = 1".to_string();
    }
    let h = like_exact(e, &fold(e, q, cs));
    let backslash = matches!(e, DbEngine::BigQuery | DbEngine::ClickHouse);
    let esc = if backslash { '\\' } else { '!' };
    let pattern = if cs {
        pattern.to_string()
    } else {
        pattern.to_lowercase()
    };
    let p = like_exact(
        e,
        &str_lit(e, &like_pattern(&pattern, esc, e == DbEngine::Mssql)),
    );
    if backslash {
        format!("{h} LIKE {p}")
    } else {
        format!("{h} LIKE {p} ESCAPE '!'")
    }
}

/// `*` to `%`, `?` to `_`, `\*` and `\?` literal, and the characters `LIKE`
/// reads specially escaped with `esc`. `[` only on SQL Server: Oracle
/// refuses an escape before anything but `%`, `_` and itself.
fn like_pattern(pattern: &str, esc: char, bracket: bool) -> String {
    let mut out = String::from("%");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if matches!(chars.peek(), Some('*') | Some('?')) => {
                if let Some(lit) = chars.next() {
                    out.push(lit);
                }
            }
            '*' => out.push('%'),
            '?' => out.push('_'),
            '%' | '_' => {
                out.push(esc);
                out.push(c);
            }
            '[' if bracket => {
                out.push(esc);
                out.push(c);
            }
            c if c == esc => {
                out.push(esc);
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push('%');
    out
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
