use super::*;

#[test]
fn live_snowflake_ddl_uses_snowflake_types() {
    let cols = vec![
        ColumnInfo {
            name: "id".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "label".to_string(),
            data_type: "Utf8".to_string(),
        },
    ];
    let sql = create_table_qualified(&cols, LiveSqlDialect::Snowflake, "analytics", "orders");
    assert_eq!(
        sql,
        "CREATE TABLE \"analytics\".\"orders\" (\"id\" BIGINT, \"label\" VARCHAR)"
    );
}

#[test]
fn live_databricks_ddl_uses_databricks_types() {
    let cols = vec![ColumnInfo {
        name: "label".to_string(),
        data_type: "Utf8".to_string(),
    }];
    let sql = create_table_qualified(&cols, LiveSqlDialect::Databricks, "main", "t");
    assert_eq!(sql, "CREATE TABLE `main`.`t` (`label` STRING)");
}

#[test]
fn live_clickhouse_ddl_wraps_nullable_and_adds_engine() {
    let cols = vec![
        ColumnInfo {
            name: "id".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "label".to_string(),
            data_type: "Utf8".to_string(),
        },
    ];
    let sql = create_table_qualified(&cols, LiveSqlDialect::ClickHouse, "default", "events");
    assert_eq!(
        sql,
        "CREATE TABLE `default`.`events` (`id` Nullable(Int64), `label` Nullable(String)) \
             ENGINE = MergeTree() ORDER BY tuple()"
    );
}

#[test]
fn live_bigquery_ddl_uses_standard_sql_types() {
    let cols = vec![
        ColumnInfo {
            name: "id".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "ok".to_string(),
            data_type: "Boolean".to_string(),
        },
        ColumnInfo {
            name: "label".to_string(),
            data_type: "Utf8".to_string(),
        },
    ];
    let sql = create_table_qualified(&cols, LiveSqlDialect::BigQuery, "analytics", "orders");
    assert_eq!(
        sql,
        "CREATE TABLE `analytics`.`orders` (`id` INT64, `ok` BOOL, `label` STRING)"
    );
}

#[test]
fn live_exasol_ddl_uses_exasol_types() {
    let cols = vec![
        ColumnInfo {
            name: "id".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "label".to_string(),
            data_type: "Utf8".to_string(),
        },
    ];
    let sql = create_table_qualified(&cols, LiveSqlDialect::Exasol, "s", "t");
    assert_eq!(
        sql,
        "CREATE TABLE \"s\".\"t\" (\"id\" DECIMAL(19,0), \"label\" VARCHAR(2000000))"
    );
}

/// The DDL Octa sends to Oracle has to stay inside what it can read back:
/// the driver never decodes BINARY_FLOAT / BINARY_DOUBLE, so a decimal
/// column written as one would come back unreadable from the table Octa
/// just created.
#[test]
fn oracle_ddl_writes_only_types_octa_can_read_back() {
    assert_eq!(oracle_type("Float64"), "NUMBER");
    assert_eq!(oracle_type("Float32"), "NUMBER");
    for t in ["Int64", "Float64", "Boolean", "Utf8", "Date32", "Binary"] {
        let sql = oracle_type(t);
        assert!(
            !sql.contains("BINARY_FLOAT") && !sql.contains("BINARY_DOUBLE"),
            "{t} maps to {sql}, which Octa cannot read back"
        );
    }
    // No BOOLEAN before 23c, and VARCHAR2 is the bounded text type.
    assert_eq!(oracle_type("Boolean"), "NUMBER(1)");
    assert_eq!(oracle_type("Utf8"), "VARCHAR2(4000)");
    assert_eq!(oracle_type("Timestamp(Microsecond, None)"), "TIMESTAMP");
}

/// A naive timestamp must export as a naive column in every dialect that
/// has both, whatever spelling the reader used for it.
///
/// The regression: the zoned test was `contains("None")`, which is a fact
/// about one spelling rather than about the type. Parquet and Arrow IPC
/// name a naive column `Timestamp(us)` (Arrow's own Display omits the
/// `None`) and ORC names it plain `Timestamp`, so all three exported as
/// TIMESTAMPTZ / DATETIMEOFFSET / TIMESTAMP_TZ - a timezone the data
/// never had, attached by a CREATE TABLE the user would then load into.
#[test]
fn a_naive_timestamp_never_exports_as_a_zoned_column() {
    // Every spelling of "timestamp, no zone" that reaches this module.
    for naive in [
        "Timestamp(Microsecond, None)",
        "Timestamp(\u{b5}s)",
        "Timestamp(ms)",
        "Timestamp",
    ] {
        assert_eq!(pg_type(naive), "TIMESTAMP", "postgres, {naive}");
        assert_eq!(mssql_type(naive), "DATETIME2", "mssql, {naive}");
        assert_eq!(snowflake_type(naive), "TIMESTAMP_NTZ", "snowflake, {naive}");
        assert_eq!(oracle_type(naive), "TIMESTAMP", "oracle, {naive}");
        assert_eq!(trino_type(naive), "TIMESTAMP(6)", "trino, {naive}");
    }

    // And a zoned one still exports zoned, in both spellings.
    for zoned in [
        "Timestamp(Microsecond, Some(\"Europe/Brussels\"))",
        "Timestamp(\u{b5}s, \"Europe/Brussels\")",
    ] {
        assert_eq!(pg_type(zoned), "TIMESTAMPTZ", "postgres, {zoned}");
        assert_eq!(mssql_type(zoned), "DATETIMEOFFSET", "mssql, {zoned}");
        assert_eq!(snowflake_type(zoned), "TIMESTAMP_TZ", "snowflake, {zoned}");
        assert_eq!(
            oracle_type(zoned),
            "TIMESTAMP WITH TIME ZONE",
            "oracle, {zoned}"
        );
        assert_eq!(
            trino_type(zoned),
            "TIMESTAMP(6) WITH TIME ZONE",
            "trino, {zoned}"
        );
    }
}

#[test]
fn exasol_decimal_width_holds_each_integer_range() {
    // DECIMAL(p,0) holds p digits, so every integer type needs a width
    // wide enough for its maximum value. Int32 reaching 2,147,483,647
    // once sat in DECIMAL(9,0), which rejects any value above 999,999,999
    // at insert time with DDL that still looked valid.
    for (arrow_type, max_value) in [
        ("Int8", 127i128),
        ("Int16", 32_767),
        ("UInt8", 255),
        ("UInt16", 65_535),
        ("Int32", 2_147_483_647),
        ("UInt32", 4_294_967_295),
        ("Int64", 9_223_372_036_854_775_807),
        ("UInt64", 18_446_744_073_709_551_615),
    ] {
        let sql = exasol_type(arrow_type);
        let digits: u32 = sql
            .trim_start_matches("DECIMAL(")
            .split(',')
            .next()
            .and_then(|d| d.parse().ok())
            .unwrap_or_else(|| panic!("{arrow_type} did not map to a DECIMAL: {sql}"));
        let capacity = 10i128.pow(digits) - 1;
        assert!(
            capacity >= max_value,
            "{arrow_type} maps to {sql}, which holds {capacity} but must hold {max_value}"
        );
    }
}

#[test]
fn bigquery_quotes_awkward_identifiers_with_backticks() {
    let cols = vec![ColumnInfo {
        name: "order total".to_string(),
        data_type: "Float64".to_string(),
    }];
    let sql = create_table_qualified(&cols, LiveSqlDialect::BigQuery, "", "t");
    assert_eq!(sql, "CREATE TABLE `t` (`order total` FLOAT64)");
}

/// The live CREATE is followed by an INSERT whose column list is quoted
/// by `DbEngine::quote_ident`, which always quotes. If this DDL emits a
/// safe-looking identifier bare, Postgres folds it to lower case (and
/// Snowflake to upper), and the INSERT then addresses a column that does
/// not exist. Every table with a capital letter in a header - which is
/// most CSVs - failed to write, reporting only "inserting rows".
#[test]
fn live_ddl_quotes_identifiers_so_the_insert_can_find_them() {
    let cols = vec![ColumnInfo {
        name: "FL_DATE".to_string(),
        data_type: "Date32".to_string(),
    }];
    for (dialect, quoted) in [
        (LiveSqlDialect::Postgres, "\"FL_DATE\""),
        (LiveSqlDialect::Snowflake, "\"FL_DATE\""),
        (LiveSqlDialect::Exasol, "\"FL_DATE\""),
        (LiveSqlDialect::Mysql, "`FL_DATE`"),
        (LiveSqlDialect::Databricks, "`FL_DATE`"),
        (LiveSqlDialect::ClickHouse, "`FL_DATE`"),
        (LiveSqlDialect::BigQuery, "`FL_DATE`"),
        (LiveSqlDialect::Mssql, "[FL_DATE]"),
    ] {
        let sql = create_table_qualified(&cols, dialect, "public", "MyTable");
        assert!(
            sql.contains(quoted),
            "{dialect:?} emitted {sql}, which does not quote the column"
        );
        // The table name folds too, and DROP / INSERT always quote it.
        assert!(
            !sql.contains("TABLE public.MyTable"),
            "{dialect:?} left the table name bare: {sql}"
        );
    }
}

#[test]
fn existing_dialects_gain_no_table_suffix() {
    let cols = vec![ColumnInfo {
        name: "id".to_string(),
        data_type: "Int64".to_string(),
    }];
    assert_eq!(
        create_table_qualified(&cols, LiveSqlDialect::Postgres, "public", "t"),
        "CREATE TABLE \"public\".\"t\" (\"id\" BIGINT)"
    );
}
