//! Per-engine SQL spellings the pushdown analyses share.
//!
//! Twelve engines, few of which agree on a percentile, a correlation, a
//! string length or "the first n rows". Each helper matches on the engine;
//! `Option` means the engine has no such function and the caller computes
//! that part on the loaded rows instead, rather than sending SQL the server
//! rejects.

use crate::data::DataTable;
use crate::data::timeseries::Interval;
use crate::db::DbEngine;

use super::cell_f64;

/// `expr` as text, the engine's own cast.
pub fn text(engine: DbEngine, expr: &str) -> String {
    engine.as_text(expr)
}

fn float_type(engine: DbEngine) -> &'static str {
    match engine {
        DbEngine::Postgres | DbEngine::Redshift => "DOUBLE PRECISION",
        DbEngine::Mssql => "FLOAT",
        DbEngine::Oracle => "BINARY_DOUBLE",
        // ClickHouse rejects casting NULL to non-Nullable types
        DbEngine::ClickHouse => "Nullable(Float64)",
        DbEngine::BigQuery => "FLOAT64",
        DbEngine::MySql
        | DbEngine::Exasol
        | DbEngine::Trino
        | DbEngine::Athena
        | DbEngine::Snowflake
        | DbEngine::Databricks => "DOUBLE",
    }
}

/// `expr` as a double, so a sum of squares cannot overflow an integer type.
pub fn as_float(engine: DbEngine, expr: &str) -> String {
    format!("CAST({expr} AS {})", float_type(engine))
}

/// `col` as a double for a correlation, with `data_type` the tab's type for
/// it. A Boolean is 1.0/0.0, as the in-memory engine reads it: Postgres,
/// Redshift, BigQuery and Snowflake refuse to cast a boolean to a double, so
/// it goes through `CASE`. SQL Server's `bit` is not a predicate (`CASE WHEN
/// x` fails) but casts, and MySQL's BOOLEAN is a TINYINT, so those two cast.
pub fn as_number(engine: DbEngine, col: &str, data_type: &str) -> String {
    if data_type != "Boolean" || matches!(engine, DbEngine::Mssql | DbEngine::MySql) {
        return as_float(engine, col);
    }
    format!("CASE WHEN {col} THEN 1.0 WHEN NOT {col} THEN 0.0 END")
}

/// Character length of a text expression.
pub fn length_fn(engine: DbEngine) -> &'static str {
    match engine {
        DbEngine::Mssql => "LEN",
        DbEngine::MySql => "CHAR_LENGTH",
        DbEngine::ClickHouse => "lengthUTF8",
        _ => "LENGTH",
    }
}

/// Sample standard deviation, the one DuckDB's SUMMARIZE reports.
pub fn stddev_fn(engine: DbEngine) -> &'static str {
    match engine {
        DbEngine::Mssql => "STDEV",
        DbEngine::ClickHouse => "stddevSamp",
        _ => "STDDEV_SAMP",
    }
}

/// The cell holds a value: not NULL and not the empty string, the same
/// "missing" the in-memory engines count. Oracle and Exasol store `''` as
/// NULL, and `x <> ''` is never true there, so they get the NULL test alone.
pub fn present(engine: DbEngine, col: &str) -> String {
    match engine {
        DbEngine::Oracle | DbEngine::Exasol => format!("{col} IS NOT NULL"),
        _ => format!("{col} IS NOT NULL AND {} <> ''", text(engine, col)),
    }
}

/// Appended after an `ORDER BY` to keep the first `n` rows.
pub fn limit_clause(engine: DbEngine, n: usize) -> String {
    match engine {
        DbEngine::Mssql | DbEngine::Oracle => format!(" OFFSET 0 ROWS FETCH NEXT {n} ROWS ONLY"),
        _ => format!(" LIMIT {n}"),
    }
}

/// ` AS name` after a derived table; Oracle rejects the `AS`.
pub fn subquery_alias(engine: DbEngine, name: &str) -> String {
    match engine {
        DbEngine::Oracle => format!(" {name}"),
        _ => format!(" AS {name}"),
    }
}

/// A float literal every engine reads as a double: exponent form (no
/// 300-digit literal Exasol's DECIMAL cannot hold) inside parentheses (so
/// `x - (-3.5e0)` never becomes the comment `x --3.5`).
/// Callers must pass finite values; NaN and infinity are not valid SQL.
/// ponytail: Oracle reads `1e130` as NUMBER, which overflows past 1e126;
/// a `d` suffix (`1e130d`, BINARY_DOUBLE) fixes it if such a fence shows up.
pub fn float_lit(x: f64) -> String {
    debug_assert!(x.is_finite(), "float_lit needs a finite value");
    format!("({x:e})")
}

/// One row, three columns (25th, 50th, 75th percentile) for numeric `col`.
///
/// `PERCENTILE_CONT` interpolates, like the in-memory quartiles. SQL Server
/// and BigQuery only have it as a window function, hence the one-row
/// window query; Trino and Athena only have an approximate one; MySQL has
/// none, so `None` and the caller uses the loaded rows.
pub fn quartiles_sql(engine: DbEngine, from: &str, col: &str) -> Option<String> {
    let x = as_float(engine, col);
    let each = |f: &dyn Fn(f64) -> String| {
        [0.25, 0.5, 0.75]
            .iter()
            .map(|p| f(*p))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Some(match engine {
        DbEngine::Postgres
        | DbEngine::Redshift
        | DbEngine::Oracle
        | DbEngine::Snowflake
        | DbEngine::Exasol
        | DbEngine::Databricks => format!(
            "SELECT {} FROM {from}",
            each(&|p| format!("PERCENTILE_CONT({p}) WITHIN GROUP (ORDER BY {x})"))
        ),
        DbEngine::Mssql => format!(
            "SELECT TOP 1 {} FROM {from}",
            each(&|p| format!("PERCENTILE_CONT({p}) WITHIN GROUP (ORDER BY {x}) OVER ()"))
        ),
        DbEngine::BigQuery => format!(
            "SELECT {} FROM {from} LIMIT 1",
            each(&|p| format!("PERCENTILE_CONT({x}, {p}) OVER ()"))
        ),
        DbEngine::ClickHouse => format!(
            "SELECT {} FROM {from}",
            each(&|p| format!("quantileExactInclusive({p})({x})"))
        ),
        DbEngine::Trino | DbEngine::Athena => format!(
            "SELECT {} FROM {from}",
            each(&|p| format!("approx_percentile({x}, {p})"))
        ),
        DbEngine::MySql => return None,
    })
}

/// Select expressions for the Pearson correlation of `x` and `y` over the
/// rows where both are present, and how many result columns they take.
///
/// `CORR` where the engine has one. MySQL, SQL Server and Redshift do not,
/// so they send the six sums and [`pearson_read`] finishes the formula.
/// ponytail: the sums form loses precision on huge values with a tiny
/// spread (catastrophic cancellation); CORR does not. Centre the values in
/// a subquery if that ever shows.
pub fn pearson_select(engine: DbEngine, x: &str, y: &str) -> (String, usize) {
    match engine {
        DbEngine::MySql | DbEngine::Mssql | DbEngine::Redshift => {
            let (fx, fy) = (as_float(engine, x), as_float(engine, y));
            let both = format!("{x} IS NOT NULL AND {y} IS NOT NULL");
            let s = |e: &str| format!("SUM(CASE WHEN {both} THEN {e} END)");
            let parts = [
                format!("SUM(CASE WHEN {both} THEN 1 ELSE 0 END)"),
                s(&fx),
                s(&fy),
                s(&format!("{fx} * {fx}")),
                s(&format!("{fy} * {fy}")),
                s(&format!("{fx} * {fy}")),
            ];
            (parts.join(", "), 6)
        }
        DbEngine::ClickHouse => (format!("corr({x}, {y})"), 1),
        _ => (format!("CORR({x}, {y})"), 1),
    }
}

/// The coefficient from a [`pearson_select`] result starting at column
/// `start` of row 0. `None` when undefined (fewer than two rows, or no
/// spread), the same cases the in-memory engine leaves blank.
pub fn pearson_read(t: &DataTable, start: usize, width: usize) -> Option<f64> {
    let at = |i: usize| t.get(0, start + i).and_then(cell_f64);
    let r = if width == 1 {
        at(0)?
    } else {
        let (n, sx, sy, sxx, syy, sxy) = (at(0)?, at(1)?, at(2)?, at(3)?, at(4)?, at(5)?);
        if n < 2.0 {
            return None;
        }
        let vx = n * sxx - sx * sx;
        let vy = n * syy - sy * sy;
        if vx <= 0.0 || vy <= 0.0 {
            return None;
        }
        (n * sxy - sx * sy) / (vx.sqrt() * vy.sqrt())
    };
    r.is_finite().then(|| r.clamp(-1.0, 1.0))
}

/// Average rank of `col` (ties share the mean of their positions, 1-based)
/// among the rows where it is not NULL; NULL where it is. Spearman is the
/// Pearson correlation of these.
///
/// BigQuery rejects PARTITION BY on a FLOAT64 expression, so the tie count
/// partitions by a text cast of the column; other engines partition by the
/// column directly.
pub fn avg_rank(engine: DbEngine, col: &str) -> String {
    let partition_expr = if engine == DbEngine::BigQuery {
        text(engine, col)
    } else {
        col.to_string()
    };
    format!(
        "CASE WHEN {col} IS NULL THEN NULL ELSE \
         RANK() OVER (PARTITION BY CASE WHEN {col} IS NULL THEN 1 ELSE 0 END ORDER BY {col}) \
         + (COUNT(*) OVER (PARTITION BY {partition_expr}) - 1) / 2.0 END"
    )
}

/// Strip leading and trailing spaces. SQL Server has `TRIM` only from 2017.
pub fn trim(engine: DbEngine, t: &str) -> String {
    match engine {
        DbEngine::Mssql => format!("LTRIM(RTRIM({t}))"),
        _ => format!("TRIM({t})"),
    }
}

/// A key as the key analyses compare it: trimmed text, blank as NULL, so
/// `COUNT` and `COUNT(DISTINCT)` skip it as the in-memory engines skip a
/// blank cell. `col` is already quoted.
pub fn key_text(engine: DbEngine, col: &str) -> String {
    format!("NULLIF({}, '')", trim(engine, &text(engine, col)))
}

/// A cell as Find lookup tables groups it: untrimmed text, with NULL and
/// `''` one value (the in-memory engine reads both as "").
pub fn group_text(engine: DbEngine, col: &str) -> String {
    format!("NULLIF({}, '')", text(engine, col))
}

/// Text `t` unchanged, or NULL when it is blank or only spaces: Join
/// diagnostics compares the raw value but skips empty keys. The test uses
/// `NULLIF(TRIM(..), '')` because Oracle's `TRIM('  ')` is NULL, which `= ''`
/// never matches.
pub fn blank_null(engine: DbEngine, t: &str) -> String {
    format!(
        "CASE WHEN NULLIF({}, '') IS NULL THEN NULL ELSE {t} END",
        trim(engine, t)
    )
}

/// Lower case. ClickHouse's `lower` is ASCII only.
pub fn lower(engine: DbEngine, t: &str) -> String {
    match engine {
        DbEngine::ClickHouse => format!("lowerUTF8({t})"),
        _ => format!("LOWER({t})"),
    }
}

/// `t` without leading `0`s. SQL Server's two-argument `LTRIM` arrived in
/// 2022 and nothing older spells it, so `None` there.
pub fn strip_leading_zeros(engine: DbEngine, t: &str) -> Option<String> {
    match engine {
        DbEngine::Mssql => None,
        DbEngine::MySql
        | DbEngine::Databricks
        | DbEngine::Trino
        | DbEngine::Athena
        | DbEngine::ClickHouse => Some(format!("TRIM(LEADING '0' FROM {t})")),
        DbEngine::Postgres
        | DbEngine::Redshift
        | DbEngine::Oracle
        | DbEngine::Snowflake
        | DbEngine::BigQuery
        | DbEngine::Exasol => Some(format!("LTRIM({t}, '0')")),
    }
}

/// `t` compared byte for byte, as the in-memory engines compare text. MySQL's
/// default `utf8mb4_0900_ai_ci` and SQL Server's `CI_AS` collations ignore
/// case and accents in `=`, `DISTINCT` and `COUNT(DISTINCT)`. Apply it last:
/// MySQL's `LOWER` does nothing to a binary string.
///
/// Trailing spaces are the exception on SQL Server only: its `=` ignores them
/// under every collation, `_BIN2` included, so there `'b2 '` still equals
/// `'b2'`. MySQL's `CAST(.. AS BINARY)` is a VARBINARY, which compares every
/// byte, trailing spaces too.
pub fn exact(engine: DbEngine, t: &str) -> String {
    match engine {
        DbEngine::MySql => format!("CAST({t} AS BINARY)"),
        DbEngine::Mssql => format!("{t} COLLATE Latin1_General_100_BIN2"),
        _ => t.to_string(),
    }
}

/// What `COUNT(DISTINCT ..)` counts for column `q` of type `data_type`: a
/// text column's exact text (see [`exact`]), anything else as it is (SQL
/// Server refuses `COLLATE` on a number).
pub fn distinct_key(engine: DbEngine, q: &str, data_type: &str) -> String {
    if super::view::is_text_type(data_type) {
        exact(engine, q)
    } else {
        q.to_string()
    }
}

/// Replace every match of a character class with one space. POSIX bracket
/// classes where the engine's regex has them; Java's `\s` / `\p{Punct}` on
/// Databricks, Trino and Athena, whose Java-style regex has no POSIX
/// brackets. Postgres replaces only the first match without `'g'`.
/// SQL Server has no regex function (before 2025): `None`.
fn replace_class(engine: DbEngine, t: &str, posix: &str, java: &str) -> Option<String> {
    match engine {
        DbEngine::Mssql => None,
        DbEngine::Postgres => Some(format!("REGEXP_REPLACE({t}, '{posix}', ' ', 'g')")),
        DbEngine::Redshift
        | DbEngine::Oracle
        | DbEngine::MySql
        | DbEngine::Snowflake
        | DbEngine::BigQuery
        | DbEngine::Exasol => Some(format!("REGEXP_REPLACE({t}, '{posix}', ' ')")),
        DbEngine::ClickHouse => Some(format!("replaceRegexpAll({t}, '{posix}', ' ')")),
        // Spark unescapes backslashes in string literals: double them. This
        // assumes `spark.sql.parser.escapedStringLiterals=false`, the default.
        DbEngine::Databricks => Some(format!(
            "regexp_replace({t}, '{}', ' ')",
            java.replace('\\', "\\\\")
        )),
        DbEngine::Trino | DbEngine::Athena => Some(format!("regexp_replace({t}, '{java}', ' ')")),
    }
}

/// Runs of whitespace to one space, ends trimmed: Rust's
/// `split_whitespace().join(" ")`.
pub fn collapse_ws(engine: DbEngine, t: &str) -> Option<String> {
    replace_class(engine, t, "[[:space:]]+", "\\s+").map(|r| trim(engine, &r))
}

/// ASCII punctuation to a space, then [`collapse_ws`]: the in-memory
/// "strip punctuation" normalisation. Engines whose locale is wider than
/// ASCII may count more marks as punctuation (documented).
///
/// MySQL 8 (ICU), Trino and Athena (Joni) and Postgres under an ICU locale
/// read `[[:punct:]]` / `\p{Punct}` as Unicode category P only, which leaves
/// out the ASCII symbols `$+<=>^`|~` that Rust's `is_ascii_punctuation`
/// includes, so those engines name them explicitly.
pub fn strip_punct(engine: DbEngine, t: &str) -> Option<String> {
    let (posix, java) = match engine {
        DbEngine::MySql | DbEngine::Postgres | DbEngine::Trino | DbEngine::Athena => {
            ("[[:punct:]]|[$+<=>^`|~]", "\\p{Punct}|[$+<=>^`|~]")
        }
        _ => ("[[:punct:]]", "\\p{Punct}"),
    };
    replace_class(engine, t, posix, java).and_then(|r| collapse_ws(engine, &r))
}

/// `s` as a string literal the engine reads back unchanged. The one place
/// these rules live: the server analyses compare with it, and every save and
/// table write (`sql_builders::sql_literal`) writes text with it.
///
/// Snowflake, ClickHouse, Redshift, Databricks and BigQuery read a backslash
/// in a literal as an escape, so it is doubled there (`C:\temp` would arrive
/// as `C:`, a tab and `emp`). BigQuery and Databricks do not take `''` for a
/// quote: Databricks joins `'it''s'` into `its`, so theirs is `\'`. A BigQuery
/// quoted literal cannot hold a raw line break. MySQL escapes with a backslash
/// unless `NO_BACKSLASH_ESCAPES` is set, so text with one goes as a hex
/// string, exact in both modes, with an introducer that keeps it text (a JSON
/// column needs that). SQL Server: see [`mssql_str_lit`]. Pinned against real
/// servers by the `*_text_survives_a_write_live` tests and, for Databricks,
/// against Spark's grammar.
pub fn str_lit(engine: DbEngine, s: &str) -> String {
    match engine {
        DbEngine::BigQuery => format!(
            "'{}'",
            s.replace('\\', "\\\\")
                .replace('\'', "\\'")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
        ),
        DbEngine::Databricks => {
            format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
        }
        DbEngine::MySql if s.contains('\\') => {
            let hex: String = s.bytes().map(|byte| format!("{byte:02x}")).collect();
            format!("_utf8mb4 X'{hex}'")
        }
        DbEngine::ClickHouse | DbEngine::Snowflake | DbEngine::Redshift => {
            format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
        }
        DbEngine::Mssql => mssql_str_lit(s),
        _ => format!("'{}'", s.replace('\'', "''")),
    }
}

/// A SQL Server literal. Text outside ASCII gets `N'..'`: a plain literal
/// goes through the database's code page first, which turned `中文` into
/// `??`. ASCII stays plain, so comparing it with a VARCHAR key can still use
/// the key's index. Inside a literal T-SQL reads a backslash before a line
/// break as a line continuation and drops both, so that backslash is spliced
/// in as `NCHAR(92)`.
fn mssql_str_lit(s: &str) -> String {
    let prefix = if s.is_ascii() { "" } else { "N" };
    let mut out = format!("{prefix}'");
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\'' => out.push_str("''"),
            '\\' if matches!(chars.peek(), Some('\n' | '\r')) => {
                out.push_str(&format!("' + NCHAR(92) + {prefix}'"));
            }
            _ => out.push(ch),
        }
    }
    out.push('\'');
    out
}

/// A typed date or time column: the connectors' `Date32` and
/// `Timestamp(..)`. A text column holding dates is not one.
pub fn is_time_type(data_type: &str) -> bool {
    data_type.starts_with("Date") || data_type.starts_with("Timestamp")
}

/// The start of the `interval` bucket holding `col` (a quoted, typed DATE
/// or TIMESTAMP column), as the engine spells it. Weeks start on Monday on
/// every engine, as DuckDB's `date_trunc('week')` (the file path): BigQuery,
/// SQL Server, Snowflake and Exasol start them elsewhere by default or by
/// session setting, so their weeks are spelled out.
pub fn date_bucket(engine: DbEngine, col: &str, interval: Interval) -> String {
    use Interval::*;
    let unit = interval.unit();
    match engine {
        DbEngine::Postgres
        | DbEngine::Redshift
        | DbEngine::Trino
        | DbEngine::Athena
        | DbEngine::Databricks => format!("DATE_TRUNC('{unit}', {col})"),
        DbEngine::Snowflake => match interval {
            Week => format!("DATEADD(day, 1 - DAYOFWEEKISO({col}), DATE_TRUNC('day', {col}))"),
            _ => format!("DATE_TRUNC('{unit}', {col})"),
        },
        // 1900-01-01 was a Monday.
        DbEngine::Exasol => match interval {
            Week => format!(
                "ADD_DAYS(DATE '1900-01-01', FLOOR(DAYS_BETWEEN({col}, DATE '1900-01-01') / 7) * 7)"
            ),
            _ => format!("DATE_TRUNC('{unit}', {col})"),
        },
        DbEngine::Oracle => {
            let f = match interval {
                Minute => "MI",
                Hour => "HH24",
                Day => "DD",
                Week => "IW",
                Month => "MM",
                Quarter => "Q",
                Year => "YYYY",
            };
            format!("TRUNC({col}, '{f}')")
        }
        // DATETRUNC needs SQL Server 2022. Day 0 is 1900-01-01, a Monday.
        // ponytail: dates before 1900 give a negative remainder (week off by
        // one); DATETRUNC(iso_week, ..) once 2022 is the floor.
        DbEngine::Mssql => match interval {
            Week => format!(
                "DATEADD(day, -(DATEDIFF(day, 0, {col}) % 7), DATEADD(day, DATEDIFF(day, 0, {col}), 0))"
            ),
            _ => format!("DATEADD({unit}, DATEDIFF({unit}, 0, {col}), 0)"),
        },
        DbEngine::MySql => match interval {
            Minute => format!("TIMESTAMP(DATE_FORMAT({col}, '%Y-%m-%d %H:%i:00'))"),
            Hour => format!("TIMESTAMP(DATE_FORMAT({col}, '%Y-%m-%d %H:00:00'))"),
            Day => format!("TIMESTAMP(DATE({col}))"),
            Week => format!("TIMESTAMP(DATE({col}) - INTERVAL WEEKDAY({col}) DAY)"),
            Month => format!("TIMESTAMP(DATE_FORMAT({col}, '%Y-%m-01'))"),
            Quarter => format!(
                "TIMESTAMP(MAKEDATE(YEAR({col}), 1) + INTERVAL (QUARTER({col}) - 1) QUARTER)"
            ),
            Year => format!("TIMESTAMP(MAKEDATE(YEAR({col}), 1))"),
        },
        DbEngine::ClickHouse => {
            let f = match interval {
                Minute => "toStartOfMinute",
                Hour => "toStartOfHour",
                Day => "toStartOfDay",
                Week => "toMonday",
                Month => "toStartOfMonth",
                Quarter => "toStartOfQuarter",
                Year => "toStartOfYear",
            };
            format!("toDateTime({f}({col}))")
        }
        DbEngine::BigQuery => {
            let part = match interval {
                Week => "WEEK(MONDAY)".to_string(),
                _ => unit.to_uppercase(),
            };
            format!("TIMESTAMP_TRUNC(CAST({col} AS TIMESTAMP), {part})")
        }
    }
}

/// Seconds since 1970-01-01 00:00:00 of a TIMESTAMP column, as a double:
/// the local chart's DateTime axis (`chart::cell_to_f64`). The time is read
/// as the connector shows it: a naive TIMESTAMP as stored, a Postgres
/// TIMESTAMPTZ as its true epoch (the connector shows it in UTC), so the
/// session time zone never moves the axis there. ponytail: Databricks'
/// TIMESTAMP, Snowflake's TIMESTAMP_LTZ and ClickHouse's DateTime on a
/// non-UTC server follow that zone, and Oracle drops fractional seconds;
/// a non-UTC session shifts the axis.
pub fn epoch_seconds(engine: DbEngine, col: &str) -> String {
    let secs = match engine {
        DbEngine::Postgres | DbEngine::Redshift => format!("EXTRACT(EPOCH FROM {col})"),
        DbEngine::MySql => {
            format!("TIMESTAMPDIFF(MICROSECOND, '1970-01-01 00:00:00', {col}) / 1000000")
        }
        DbEngine::Mssql => format!("DATEDIFF_BIG(millisecond, '19700101', {col}) / 1000.0"),
        DbEngine::Oracle => format!("(CAST({col} AS DATE) - DATE '1970-01-01') * 86400"),
        DbEngine::Snowflake => {
            format!("DATE_PART(epoch_microsecond, CAST({col} AS TIMESTAMP_NTZ)) / 1000000")
        }
        DbEngine::BigQuery => format!("UNIX_MICROS(CAST({col} AS TIMESTAMP)) / 1000000"),
        DbEngine::ClickHouse => {
            format!("toUnixTimestamp64Micro(toDateTime64({col}, 6, 'UTC')) / 1000000")
        }
        DbEngine::Trino | DbEngine::Athena => format!(
            "date_diff('millisecond', TIMESTAMP '1970-01-01 00:00:00', CAST({col} AS TIMESTAMP)) / 1000.0"
        ),
        DbEngine::Databricks => format!("unix_micros(CAST({col} AS TIMESTAMP)) / 1000000"),
        DbEngine::Exasol => {
            format!("SECONDS_BETWEEN(CAST({col} AS TIMESTAMP), TIMESTAMP '1970-01-01 00:00:00')")
        }
    };
    as_float(engine, &secs)
}

/// Days since 1970-01-01 of a DATE column, as a double: the local chart's
/// Date axis. Postgres before 14 has no EXTRACT from a DATE and goes through
/// TIMESTAMPTZ (the session time zone); the cast keeps it at midnight.
pub fn epoch_days(engine: DbEngine, col: &str) -> String {
    let col = match engine {
        DbEngine::Postgres | DbEngine::Redshift => format!("CAST({col} AS TIMESTAMP)"),
        _ => col.to_string(),
    };
    format!("{} / 86400", epoch_seconds(engine, &col))
}

/// `a` modulo `b`. BigQuery, Oracle and Exasol have no `%` operator.
pub fn modulo(engine: DbEngine, a: &str, b: usize) -> String {
    match engine {
        DbEngine::BigQuery | DbEngine::Oracle | DbEngine::Exasol => format!("MOD({a}, {b})"),
        _ => format!("({a}) % {b}"),
    }
}

#[cfg(test)]
#[path = "dialect_tests.rs"]
mod tests;
