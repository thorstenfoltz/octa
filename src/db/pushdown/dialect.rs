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

/// `s` as a string literal the engine reads back unchanged.
///
/// MySQL, ClickHouse, Snowflake, Redshift, Databricks and BigQuery read a
/// backslash in a literal as an escape, so it is doubled there; BigQuery and Databricks do not take
/// `''` for a quote inside a literal, so theirs is `\'`. SQL Server converts a
/// literal without `N` to the database's code page, which turns text outside
/// it into `?` before it is ever compared. ponytail: MySQL's
/// NO_BACKSLASH_ESCAPES mode would read the doubled backslash as two; detect
/// the mode if a server ever runs with it.
pub fn str_lit(engine: DbEngine, s: &str) -> String {
    match engine {
        DbEngine::BigQuery | DbEngine::Databricks => {
            format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
        }
        DbEngine::MySql | DbEngine::ClickHouse | DbEngine::Snowflake | DbEngine::Redshift => {
            format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
        }
        DbEngine::Mssql => format!("N'{}'", s.replace('\'', "''")),
        _ => format!("'{}'", s.replace('\'', "''")),
    }
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
mod tests {
    use super::*;
    use DbEngine::*;

    #[test]
    fn first_n_rows_is_spelled_per_engine() {
        assert_eq!(limit_clause(Postgres, 5), " LIMIT 5");
        assert_eq!(
            limit_clause(Mssql, 5),
            " OFFSET 0 ROWS FETCH NEXT 5 ROWS ONLY"
        );
        assert_eq!(
            limit_clause(Oracle, 5),
            " OFFSET 0 ROWS FETCH NEXT 5 ROWS ONLY"
        );
    }

    #[test]
    fn oracle_takes_no_as_before_a_table_alias() {
        assert_eq!(subquery_alias(Oracle, "s"), " s");
        assert_eq!(subquery_alias(Postgres, "s"), " AS s");
    }

    /// Oracle stores '' as NULL, and `x <> ''` is never true there, so the
    /// empty-string half would count every row as missing.
    #[test]
    fn present_skips_the_empty_string_test_on_oracle() {
        assert_eq!(present(Oracle, "\"a\""), "\"a\" IS NOT NULL");
        assert_eq!(
            present(Postgres, "\"a\""),
            "\"a\" IS NOT NULL AND CAST(\"a\" AS VARCHAR) <> ''"
        );
    }

    /// Exasol, like Oracle, turns '' into NULL on insert.
    #[test]
    fn present_skips_the_empty_string_test_on_exasol() {
        assert_eq!(present(Exasol, "\"a\""), "\"a\" IS NOT NULL");
    }

    /// Postgres, Redshift, BigQuery and Snowflake refuse CAST(bool AS
    /// DOUBLE); SQL Server's bit cannot stand as a CASE condition.
    #[test]
    fn booleans_become_numbers_per_engine() {
        let case = "CASE WHEN x THEN 1.0 WHEN NOT x THEN 0.0 END";
        for e in [Postgres, Redshift, BigQuery, Snowflake, Oracle, ClickHouse] {
            assert_eq!(as_number(e, "x", "Boolean"), case, "{e:?}");
        }
        assert_eq!(as_number(Mssql, "x", "Boolean"), "CAST(x AS FLOAT)");
        assert_eq!(as_number(MySql, "x", "Boolean"), "CAST(x AS DOUBLE)");
        assert_eq!(
            as_number(Postgres, "x", "Int64"),
            "CAST(x AS DOUBLE PRECISION)"
        );
    }

    #[test]
    fn quartiles_per_engine() {
        let pg = quartiles_sql(Postgres, "t", "\"x\"").unwrap();
        assert!(pg.starts_with(
            "SELECT PERCENTILE_CONT(0.25) WITHIN GROUP (ORDER BY CAST(\"x\" AS DOUBLE PRECISION))"
        ));
        assert!(
            quartiles_sql(Mssql, "t", "[x]")
                .unwrap()
                .starts_with("SELECT TOP 1 ")
        );
        assert!(
            quartiles_sql(BigQuery, "t", "`x`")
                .unwrap()
                .ends_with(" LIMIT 1")
        );
        assert!(
            quartiles_sql(ClickHouse, "t", "`x`")
                .unwrap()
                .contains("quantileExactInclusive(0.25)")
        );
        assert!(
            quartiles_sql(Trino, "t", "\"x\"")
                .unwrap()
                .contains("approx_percentile(")
        );
        assert_eq!(quartiles_sql(MySql, "t", "`x`"), None);
    }

    #[test]
    fn pearson_uses_corr_where_the_engine_has_it() {
        assert_eq!(
            pearson_select(Postgres, "a", "b"),
            ("CORR(a, b)".to_string(), 1)
        );
        assert_eq!(pearson_select(ClickHouse, "a", "b").0, "corr(a, b)");
        assert_eq!(pearson_select(MySql, "a", "b").1, 6);
        assert_eq!(pearson_select(Mssql, "a", "b").1, 6);
    }

    #[test]
    fn float_literals_are_parenthesised_exponent_form() {
        assert_eq!(float_lit(-3.5), "(-3.5e0)");
        assert_eq!(float_lit(1e300), "(1e300)");
    }

    #[test]
    fn avg_rank_partitions_by_text_on_bigquery() {
        let bq_result = avg_rank(BigQuery, "`x`");
        assert!(bq_result.contains("PARTITION BY CAST(`x` AS STRING)"));
    }

    #[test]
    fn avg_rank_partitions_by_column_on_postgres() {
        let pg_result = avg_rank(Postgres, "\"x\"");
        assert!(pg_result.contains("PARTITION BY \"x\")"));
    }

    /// Every engine gets a non-empty spelling for each helper function.
    #[test]
    fn every_engine_has_every_spelling() {
        for &e in DbEngine::ALL {
            assert!(!length_fn(e).is_empty());
            assert!(!stddev_fn(e).is_empty());
            assert!(as_float(e, "x").starts_with("CAST(x AS "));
        }
    }

    #[test]
    fn keys_are_trimmed_text_with_blank_as_null() {
        assert_eq!(
            key_text(Postgres, "\"a\""),
            "NULLIF(TRIM(CAST(\"a\" AS VARCHAR)), '')"
        );
        assert_eq!(
            key_text(Mssql, "[a]"),
            "NULLIF(LTRIM(RTRIM(CAST([a] AS NVARCHAR(MAX)))), '')"
        );
        assert_eq!(group_text(MySql, "`a`"), "NULLIF(CAST(`a` AS CHAR), '')");
    }

    #[test]
    fn leading_zeros_per_engine() {
        assert_eq!(strip_leading_zeros(Postgres, "x").unwrap(), "LTRIM(x, '0')");
        assert_eq!(
            strip_leading_zeros(MySql, "x").unwrap(),
            "TRIM(LEADING '0' FROM x)"
        );
        assert_eq!(strip_leading_zeros(Mssql, "x"), None);
        assert_eq!(lower(ClickHouse, "x"), "lowerUTF8(x)");
    }

    #[test]
    fn exact_compares_bytes_where_collations_fold() {
        assert_eq!(exact(MySql, "LOWER(x)"), "CAST(LOWER(x) AS BINARY)");
        assert_eq!(exact(Mssql, "x"), "x COLLATE Latin1_General_100_BIN2");
        assert_eq!(exact(Postgres, "x"), "x");
        assert_eq!(exact(ClickHouse, "x"), "x");
    }

    #[test]
    fn collapse_whitespace_per_engine() {
        let want = [
            (
                Postgres,
                "TRIM(REGEXP_REPLACE(x, '[[:space:]]+', ' ', 'g'))",
            ),
            (Redshift, "TRIM(REGEXP_REPLACE(x, '[[:space:]]+', ' '))"),
            (Oracle, "TRIM(REGEXP_REPLACE(x, '[[:space:]]+', ' '))"),
            (MySql, "TRIM(REGEXP_REPLACE(x, '[[:space:]]+', ' '))"),
            (Snowflake, "TRIM(REGEXP_REPLACE(x, '[[:space:]]+', ' '))"),
            (BigQuery, "TRIM(REGEXP_REPLACE(x, '[[:space:]]+', ' '))"),
            (Exasol, "TRIM(REGEXP_REPLACE(x, '[[:space:]]+', ' '))"),
            (ClickHouse, "TRIM(replaceRegexpAll(x, '[[:space:]]+', ' '))"),
            (Databricks, "TRIM(regexp_replace(x, '\\\\s+', ' '))"),
            (Trino, "TRIM(regexp_replace(x, '\\s+', ' '))"),
            (Athena, "TRIM(regexp_replace(x, '\\s+', ' '))"),
        ];
        for (e, sql) in want {
            assert_eq!(collapse_ws(e, "x").as_deref(), Some(sql), "{e:?}");
        }
        assert_eq!(collapse_ws(Mssql, "x"), None);
    }

    #[test]
    fn strip_punctuation_per_engine() {
        let want = [
            (
                Postgres,
                "REGEXP_REPLACE(x, '[[:punct:]]|[$+<=>^`|~]', ' ', 'g')",
            ),
            (Redshift, "REGEXP_REPLACE(x, '[[:punct:]]', ' ')"),
            (Oracle, "REGEXP_REPLACE(x, '[[:punct:]]', ' ')"),
            (MySql, "REGEXP_REPLACE(x, '[[:punct:]]|[$+<=>^`|~]', ' ')"),
            (Snowflake, "REGEXP_REPLACE(x, '[[:punct:]]', ' ')"),
            (BigQuery, "REGEXP_REPLACE(x, '[[:punct:]]', ' ')"),
            (Exasol, "REGEXP_REPLACE(x, '[[:punct:]]', ' ')"),
            (ClickHouse, "replaceRegexpAll(x, '[[:punct:]]', ' ')"),
            (Databricks, "regexp_replace(x, '\\\\p{Punct}', ' ')"),
            (Trino, "regexp_replace(x, '\\p{Punct}|[$+<=>^`|~]', ' ')"),
            (Athena, "regexp_replace(x, '\\p{Punct}|[$+<=>^`|~]', ' ')"),
        ];
        for (e, inner) in want {
            assert_eq!(strip_punct(e, "x"), collapse_ws(e, inner), "{e:?}");
        }
        assert_eq!(strip_punct(Mssql, "x"), None);
    }

    #[test]
    fn a_literal_reads_back_unchanged_on_every_engine() {
        let s = r"O'Brien \ 50%";
        for e in [Postgres, Oracle, Trino, Athena, Exasol] {
            assert_eq!(str_lit(e, s), r"'O''Brien \ 50%'", "{e:?}");
        }
        for e in [MySql, ClickHouse, Snowflake, Redshift] {
            assert_eq!(str_lit(e, s), r"'O''Brien \\ 50%'", "{e:?}");
        }
        for e in [BigQuery, Databricks] {
            assert_eq!(str_lit(e, s), r"'O\'Brien \\ 50%'", "{e:?}");
        }
        assert_eq!(str_lit(Mssql, "Zürich"), "N'Zürich'");
    }

    #[test]
    fn distinct_counts_compare_text_exactly_and_leave_numbers_alone() {
        use DbEngine::*;
        assert_eq!(distinct_key(MySql, "`s`", "Utf8"), "CAST(`s` AS BINARY)");
        assert_eq!(
            distinct_key(Mssql, "[s]", "Utf8"),
            "[s] COLLATE Latin1_General_100_BIN2"
        );
        assert_eq!(
            distinct_key(Mssql, "[n]", "Int64"),
            "[n]",
            "no COLLATE on a number"
        );
        assert_eq!(distinct_key(Postgres, "\"s\"", "Utf8"), "\"s\"");
    }

    #[test]
    fn a_time_column_is_a_typed_one() {
        assert!(is_time_type("Date32"));
        assert!(is_time_type("Timestamp(Microsecond, None)"));
        assert!(!is_time_type("Utf8"));
        assert!(!is_time_type("Int64"));
    }

    #[test]
    fn every_engine_starts_its_weeks_on_monday() {
        use crate::data::timeseries::Interval::Week;
        let w = |e| date_bucket(e, "t", Week);
        assert_eq!(w(Postgres), "DATE_TRUNC('week', t)");
        assert_eq!(w(Oracle), "TRUNC(t, 'IW')");
        assert!(w(Mssql).contains("% 7"), "{}", w(Mssql));
        assert!(w(MySql).contains("WEEKDAY(t)"), "{}", w(MySql));
        assert_eq!(w(ClickHouse), "toDateTime(toMonday(t))");
        assert_eq!(
            w(BigQuery),
            "TIMESTAMP_TRUNC(CAST(t AS TIMESTAMP), WEEK(MONDAY))"
        );
        assert!(w(Snowflake).contains("DAYOFWEEKISO(t)"), "{}", w(Snowflake));
        assert!(w(Exasol).contains("DATE '1900-01-01'"), "{}", w(Exasol));
        for e in [Redshift, Trino, Athena, Databricks] {
            assert_eq!(w(e), "DATE_TRUNC('week', t)", "{e:?}");
        }
    }

    #[test]
    fn months_per_engine() {
        use crate::data::timeseries::Interval::Month;
        let m = |e| date_bucket(e, "t", Month);
        assert_eq!(m(Oracle), "TRUNC(t, 'MM')");
        assert_eq!(m(Mssql), "DATEADD(month, DATEDIFF(month, 0, t), 0)");
        assert_eq!(m(MySql), "TIMESTAMP(DATE_FORMAT(t, '%Y-%m-01'))");
        assert_eq!(m(ClickHouse), "toDateTime(toStartOfMonth(t))");
        assert_eq!(m(BigQuery), "TIMESTAMP_TRUNC(CAST(t AS TIMESTAMP), MONTH)");
        assert_eq!(m(Exasol), "DATE_TRUNC('month', t)");
    }

    /// The Postgres spelling, run by DuckDB, against DuckDB's own
    /// `date_trunc` over the same timestamps (the file path), for every
    /// interval. 2024-01-07 is a Sunday: its week starts 2024-01-01.
    #[test]
    fn postgres_buckets_match_the_file_path() {
        use crate::data::timeseries::Interval;
        use crate::data::{CellValue, ColumnInfo};
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "ts".into(),
            data_type: "Timestamp(Microsecond, None)".into(),
        }];
        t.rows = [
            "2024-01-07 13:45:12",
            "2024-02-29 00:00:00",
            "2023-12-31 23:59:59",
        ]
        .iter()
        .map(|s| vec![CellValue::DateTime(s.to_string())])
        .collect();
        for &i in Interval::ALL {
            let server = date_bucket(Postgres, "\"ts\"", i);
            let sql = format!(
                "SELECT {server}, date_trunc('{}', TRY_CAST(\"ts\" AS TIMESTAMP)) FROM data",
                i.unit()
            );
            let out = crate::sql::run_query(&t, &sql).unwrap().table;
            for r in 0..out.row_count() {
                assert_eq!(
                    out.get(r, 0).map(|v| v.to_string()),
                    out.get(r, 1).map(|v| v.to_string()),
                    "{i:?} row {r}"
                );
            }
        }
        let week = crate::sql::run_query(
            &t,
            &format!(
                "SELECT {} FROM data",
                date_bucket(Postgres, "\"ts\"", Interval::Week)
            ),
        )
        .unwrap()
        .table;
        assert!(
            week.get(0, 0)
                .unwrap()
                .to_string()
                .starts_with("2024-01-01")
        );
    }

    #[test]
    fn epoch_seconds_per_engine() {
        // TIMESTAMPTZ: the true epoch, never a cast through the session zone.
        assert_eq!(
            epoch_seconds(Postgres, "t"),
            "CAST(EXTRACT(EPOCH FROM t) AS DOUBLE PRECISION)"
        );
        assert!(epoch_seconds(Snowflake, "t").contains("CAST(t AS TIMESTAMP_NTZ)"));
        assert!(epoch_seconds(Databricks, "t").contains("unix_micros(CAST(t AS TIMESTAMP))"));
        assert!(epoch_seconds(Mssql, "t").contains("DATEDIFF_BIG(millisecond, '19700101', t)"));
        assert!(epoch_seconds(MySql, "t").contains("TIMESTAMPDIFF(MICROSECOND"));
        assert!(epoch_seconds(Oracle, "t").contains("DATE '1970-01-01'"));
        assert!(epoch_seconds(BigQuery, "t").contains("UNIX_MICROS"));
        assert!(epoch_seconds(ClickHouse, "t").contains("toDateTime64(t, 6, 'UTC')"));
        assert!(epoch_seconds(Exasol, "t").contains("SECONDS_BETWEEN"));
        for e in [Trino, Athena] {
            assert!(
                epoch_seconds(e, "t").contains("date_diff('millisecond'"),
                "{e:?}"
            );
        }
    }

    #[test]
    fn epoch_days_keeps_a_postgres_date_at_midnight() {
        assert_eq!(
            epoch_days(Postgres, "d"),
            "CAST(EXTRACT(EPOCH FROM CAST(d AS TIMESTAMP)) AS DOUBLE PRECISION) / 86400"
        );
        assert_eq!(
            epoch_days(Mssql, "d"),
            format!("{} / 86400", epoch_seconds(Mssql, "d"))
        );
    }

    #[test]
    fn modulo_per_engine() {
        assert_eq!(modulo(Postgres, "rn - 1", 4), "(rn - 1) % 4");
        assert_eq!(modulo(Mssql, "rn - 1", 4), "(rn - 1) % 4");
        for e in [Oracle, BigQuery, Exasol] {
            assert_eq!(modulo(e, "rn - 1", 4), "MOD(rn - 1, 4)", "{e:?}");
        }
    }

    /// The Postgres spelling, run by DuckDB, against the local chart's own
    /// reading of the same cells (`chart::cell_to_f64`: seconds for a
    /// DateTime, days for a Date).
    #[test]
    fn postgres_epoch_matches_the_local_chart() {
        use crate::data::chart::cell_to_f64;
        use crate::data::{CellValue, ColumnInfo};
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "ts".into(),
                data_type: "Timestamp(Microsecond, None)".into(),
            },
            ColumnInfo {
                name: "d".into(),
                data_type: "Date32".into(),
            },
        ];
        let ts = CellValue::DateTime("2024-01-07 13:45:12".into());
        let d = CellValue::Date("2024-02-29".into());
        t.rows = vec![vec![ts.clone(), d.clone()]];
        let sql = format!(
            "SELECT {}, {} FROM data",
            epoch_seconds(Postgres, "\"ts\""),
            epoch_days(Postgres, "\"d\"")
        );
        let out = crate::sql::run_query(&t, &sql).unwrap().table;
        let got = |c| super::super::cell_f64(out.get(0, c).unwrap()).unwrap();
        assert_eq!(got(0), cell_to_f64(&ts).unwrap());
        assert_eq!(got(1), cell_to_f64(&d).unwrap());
    }
}
