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
    for e in [ClickHouse, Snowflake, Redshift] {
        assert_eq!(str_lit(e, s), r"'O''Brien \\ 50%'", "{e:?}");
    }
    for e in [BigQuery, Databricks] {
        assert_eq!(str_lit(e, s), r"'O\'Brien \\ 50%'", "{e:?}");
    }
    // Hex, exact with and without NO_BACKSLASH_ESCAPES; plain without one.
    assert_eq!(str_lit(MySql, r"a\b"), "_utf8mb4 X'615c62'");
    assert_eq!(str_lit(MySql, "it's"), "'it''s'");
    // N only where a code page could lose characters.
    assert_eq!(str_lit(Mssql, "Zürich"), "N'Zürich'");
    assert_eq!(str_lit(Mssql, "it's"), "'it''s'");
}

#[test]
fn a_line_break_survives_where_a_literal_treats_it_specially() {
    // BigQuery: no raw line break inside a quoted literal.
    assert_eq!(str_lit(BigQuery, "a\nb\r"), r"'a\nb\r'");
    // SQL Server drops a backslash plus line break as a line continuation.
    assert_eq!(str_lit(Mssql, "a\\\nb"), "'a' + NCHAR(92) + '\nb'");
    assert_eq!(str_lit(Mssql, "ü\\\r\nb"), "N'ü' + NCHAR(92) + N'\r\nb'");
    // A backslash anywhere else is an ordinary character there.
    assert_eq!(str_lit(Mssql, r"C:\temp"), r"'C:\temp'");
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
