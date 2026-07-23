//! SQL DDL targets: Postgres, MySQL, SQLite, Databricks, Snowflake.
//! All produce a `CREATE TABLE` statement with one column per
//! `ColumnInfo`. The dialect tweaks come from two places: the
//! type-mapping table (`pg_type`, `mysql_type`, `sqlite_type`,
//! `databricks_type`, `snowflake_type`) and the identifier quoting
//! style (`"name"` for Postgres / SQLite / Snowflake, backticks for
//! MySQL / Databricks).
//!
//! Identifiers are emitted **bare** when they're already valid
//! (`[A-Za-z_][A-Za-z0-9_]*`) - the common case for real data, so the
//! DDL stays clean. Quoting only kicks in for names that genuinely
//! can't work bare: spaces, punctuation, or a leading digit.
//!
//! Octa doesn't track nullability per column today, so every column
//! is emitted without `NOT NULL`. If we ever add nullability metadata,
//! it lands here.

use crate::data::ColumnInfo;
use crate::data::schema_export::{is_safe_ident, sanitize_ident, unknown_marker};

pub fn export_postgres(columns: &[ColumnInfo], table_name: &str) -> String {
    build_create_table(columns, table_name, Dialect::Postgres)
}

pub fn export_mysql(columns: &[ColumnInfo], table_name: &str) -> String {
    build_create_table(columns, table_name, Dialect::Mysql)
}

pub fn export_sqlite(columns: &[ColumnInfo], table_name: &str) -> String {
    build_create_table(columns, table_name, Dialect::Sqlite)
}

pub fn export_databricks(columns: &[ColumnInfo], table_name: &str) -> String {
    build_create_table(columns, table_name, Dialect::Databricks)
}

pub fn export_snowflake(columns: &[ColumnInfo], table_name: &str) -> String {
    build_create_table(columns, table_name, Dialect::Snowflake)
}

pub fn export_mssql(columns: &[ColumnInfo], table_name: &str) -> String {
    build_create_table(columns, table_name, Dialect::Mssql)
}

/// The dialects live-DB write-back can target (`crate::db`). A subset of the
/// export dialects, exposed so `db::create_table_sql` reuses one type-mapping
/// source of truth.
#[derive(Debug, Clone, Copy)]
pub enum LiveSqlDialect {
    Postgres,
    Mysql,
    Mssql,
    Snowflake,
    Databricks,
    ClickHouse,
    Exasol,
    BigQuery,
}

/// The internal dialect backing one live-write dialect. Single mapping point,
/// so `create_table_qualified` and `column_type_sql` can never disagree.
fn live_dialect(dialect: LiveSqlDialect) -> Dialect {
    match dialect {
        LiveSqlDialect::Postgres => Dialect::Postgres,
        LiveSqlDialect::Mysql => Dialect::Mysql,
        LiveSqlDialect::Mssql => Dialect::Mssql,
        LiveSqlDialect::Snowflake => Dialect::Snowflake,
        LiveSqlDialect::Databricks => Dialect::Databricks,
        LiveSqlDialect::ClickHouse => Dialect::ClickHouse,
        LiveSqlDialect::Exasol => Dialect::Exasol,
        LiveSqlDialect::BigQuery => Dialect::BigQuery,
    }
}

/// CREATE TABLE DDL for a live-DB write: schema-qualified, both identifiers
/// emitted verbatim through the dialect's quoting (never sanitised - the
/// target is a real database where the exact name matters), no header
/// comment, no trailing newline.
pub fn create_table_qualified(
    columns: &[ColumnInfo],
    dialect: LiveSqlDialect,
    schema: &str,
    table: &str,
) -> String {
    let d = live_dialect(dialect);
    let target = if schema.is_empty() {
        d.quote_ident(table)
    } else {
        format!("{}.{}", d.quote_ident(schema), d.quote_ident(table))
    };
    let cols: Vec<String> = columns
        .iter()
        .map(|c| {
            format!(
                "{} {}",
                d.quote_ident(&c.name),
                d.live_column_type(&c.data_type)
            )
        })
        .collect();
    format!(
        "CREATE TABLE {} ({}){}",
        target,
        cols.join(", "),
        d.table_suffix()
    )
}

/// The dialect's SQL type name for one Arrow type-name string. Exposed for
/// the live write-back's `ALTER TABLE ADD` (one type-mapping source of truth
/// with `create_table_qualified`).
pub fn column_type_sql(dialect: LiveSqlDialect, data_type: &str) -> String {
    live_dialect(dialect).live_column_type(data_type)
}

#[derive(Debug, Clone, Copy)]
enum Dialect {
    Postgres,
    Mysql,
    Sqlite,
    Databricks,
    Snowflake,
    Mssql,
    ClickHouse,
    Exasol,
    BigQuery,
}

impl Dialect {
    /// Render `ident` for use in DDL. An identifier that is already
    /// valid is emitted bare; one with spaces, punctuation, or a
    /// leading digit is wrapped in the dialect's quote characters,
    /// with the quote char itself escaped by doubling.
    fn quote_ident(self, ident: &str) -> String {
        if is_safe_ident(ident) {
            return ident.to_string();
        }
        match self {
            // Postgres / SQLite / Snowflake accept double-quoted
            // identifiers. Escape an embedded `"` as `""`.
            Dialect::Postgres | Dialect::Sqlite | Dialect::Snowflake => {
                format!("\"{}\"", ident.replace('"', "\"\""))
            }
            // MySQL and Databricks (Spark SQL) use backticks; embedded
            // ` becomes ``.
            Dialect::Mysql | Dialect::Databricks => {
                format!("`{}`", ident.replace('`', "``"))
            }
            // SQL Server brackets; embedded ] becomes ]].
            Dialect::Mssql => format!("[{}]", ident.replace(']', "]]")),
            // BigQuery and ClickHouse use backticks; embedded ` becomes ``.
            Dialect::BigQuery | Dialect::ClickHouse => {
                format!("`{}`", ident.replace('`', "``"))
            }
            // Exasol accepts double-quoted identifiers.
            Dialect::Exasol => format!("\"{}\"", ident.replace('"', "\"\"")),
        }
    }

    fn map_type(self, data_type: &str) -> String {
        match self {
            Dialect::Postgres => pg_type(data_type),
            Dialect::Mysql => mysql_type(data_type),
            Dialect::Sqlite => sqlite_type(data_type),
            Dialect::Databricks => databricks_type(data_type),
            Dialect::Snowflake => snowflake_type(data_type),
            Dialect::Mssql => mssql_type(data_type),
            Dialect::ClickHouse => clickhouse_type(data_type),
            Dialect::Exasol => exasol_type(data_type),
            Dialect::BigQuery => bigquery_type(data_type),
        }
    }

    fn header_comment(self) -> &'static str {
        match self {
            Dialect::Postgres => "-- Generated by octa - Postgres dialect",
            Dialect::Mysql => "-- Generated by octa - MySQL dialect",
            Dialect::Sqlite => "-- Generated by octa - SQLite dialect",
            Dialect::Databricks => "-- Generated by octa - Databricks dialect",
            Dialect::Snowflake => "-- Generated by octa - Snowflake dialect",
            Dialect::Mssql => "-- Generated by octa - SQL Server dialect",
            Dialect::ClickHouse => "-- Generated by octa - ClickHouse dialect",
            Dialect::Exasol => "-- Generated by octa - Exasol dialect",
            Dialect::BigQuery => "-- Generated by octa - BigQuery dialect",
        }
    }

    /// Text appended after the closing parenthesis of a live CREATE TABLE.
    /// Only ClickHouse needs one; every other dialect emits nothing, so
    /// their output is unchanged.
    fn table_suffix(self) -> &'static str {
        match self {
            Dialect::ClickHouse => " ENGINE = MergeTree() ORDER BY tuple()",
            _ => "",
        }
    }

    /// The column type as written in a live CREATE TABLE. ClickHouse columns
    /// are NOT NULL by default, so its types are wrapped to accept nulls.
    fn live_column_type(self, data_type: &str) -> String {
        let t = self.map_type(data_type);
        match self {
            Dialect::ClickHouse => format!("Nullable({t})"),
            _ => t,
        }
    }
}

fn build_create_table(columns: &[ColumnInfo], table_name: &str, dialect: Dialect) -> String {
    // `sanitize_ident` already coerces the table name to a valid
    // identifier, so it is always emitted bare.
    let table_ident = sanitize_ident(table_name);
    let mut out = String::new();
    out.push_str(dialect.header_comment());
    out.push('\n');
    out.push_str(&format!("CREATE TABLE {} (\n", table_ident));
    let last = columns.len().saturating_sub(1);
    for (idx, col) in columns.iter().enumerate() {
        let col_ident = dialect.quote_ident(&col.name);
        let col_type = dialect.map_type(&col.data_type);
        let comma = if idx == last { "" } else { "," };
        out.push_str(&format!("    {} {}{}\n", col_ident, col_type, comma));
    }
    out.push_str(");\n");
    out
}

fn pg_type(data_type: &str) -> String {
    match data_type {
        "Int8" | "Int16" => "SMALLINT".to_string(),
        "Int32" => "INTEGER".to_string(),
        "Int64" => "BIGINT".to_string(),
        // Postgres has no unsigned integer types - widen one bucket
        // and document the lift via the `unknown_marker` for UInt64
        // (which truly can't round-trip).
        "UInt8" => "SMALLINT".to_string(),
        "UInt16" => "INTEGER".to_string(),
        "UInt32" => "BIGINT".to_string(),
        "UInt64" => format!("NUMERIC(20, 0) /* {} (no unsigned in pg) */", data_type),
        "Float16" | "Float32" => "REAL".to_string(),
        "Float64" => "DOUBLE PRECISION".to_string(),
        "Boolean" => "BOOLEAN".to_string(),
        "Utf8" | "LargeUtf8" | "String" => "TEXT".to_string(),
        "Date32" | "Date64" => "DATE".to_string(),
        "Binary" | "LargeBinary" => "BYTEA".to_string(),
        other => {
            if other.starts_with("Timestamp") {
                // Postgres distinguishes between TIMESTAMP and TIMESTAMP WITH
                // TIME ZONE; Arrow's Timestamp has the same dichotomy via the
                // tz parameter ("Timestamp(Microsecond, Some(\"UTC\"))" vs
                // "...None"). Cheap check for "None" / "Some".
                if other.contains("None") {
                    "TIMESTAMP".to_string()
                } else {
                    "TIMESTAMPTZ".to_string()
                }
            } else {
                format!("TEXT /* {} */", unknown_marker(other))
            }
        }
    }
}

fn mysql_type(data_type: &str) -> String {
    match data_type {
        "Int8" => "TINYINT".to_string(),
        "Int16" => "SMALLINT".to_string(),
        "Int32" => "INT".to_string(),
        "Int64" => "BIGINT".to_string(),
        "UInt8" => "TINYINT UNSIGNED".to_string(),
        "UInt16" => "SMALLINT UNSIGNED".to_string(),
        "UInt32" => "INT UNSIGNED".to_string(),
        "UInt64" => "BIGINT UNSIGNED".to_string(),
        "Float16" | "Float32" => "FLOAT".to_string(),
        "Float64" => "DOUBLE".to_string(),
        // MySQL's BOOLEAN is an alias for TINYINT(1); use the alias
        // for readability.
        "Boolean" => "BOOLEAN".to_string(),
        "Utf8" | "LargeUtf8" | "String" => "TEXT".to_string(),
        "Date32" | "Date64" => "DATE".to_string(),
        "Binary" => "BLOB".to_string(),
        "LargeBinary" => "LONGBLOB".to_string(),
        other => {
            if other.starts_with("Timestamp") {
                "DATETIME".to_string()
            } else {
                format!("TEXT /* {} */", unknown_marker(other))
            }
        }
    }
}

fn sqlite_type(data_type: &str) -> String {
    // SQLite uses type *affinity*, not strict types - five buckets:
    // INTEGER / REAL / TEXT / BLOB / NUMERIC. We pick the closest
    // affinity for each Arrow type.
    match data_type {
        "Int8" | "Int16" | "Int32" | "Int64" | "UInt8" | "UInt16" | "UInt32" | "UInt64" => {
            "INTEGER".to_string()
        }
        "Float16" | "Float32" | "Float64" => "REAL".to_string(),
        // SQLite has no boolean type - store as 0/1 INTEGER.
        "Boolean" => "INTEGER".to_string(),
        "Utf8" | "LargeUtf8" | "String" => "TEXT".to_string(),
        "Date32" | "Date64" => "TEXT".to_string(),
        "Binary" | "LargeBinary" => "BLOB".to_string(),
        other => {
            if other.starts_with("Timestamp") {
                "TEXT".to_string()
            } else {
                format!("TEXT /* {} */", unknown_marker(other))
            }
        }
    }
}

fn mssql_type(data_type: &str) -> String {
    match data_type {
        "Int8" | "Int16" => "SMALLINT".to_string(),
        "Int32" => "INT".to_string(),
        "Int64" => "BIGINT".to_string(),
        // No unsigned integers in T-SQL - widen one bucket; UInt64 can't
        // round-trip and gets the marker.
        "UInt8" => "SMALLINT".to_string(),
        "UInt16" => "INT".to_string(),
        "UInt32" => "BIGINT".to_string(),
        "UInt64" => format!("NUMERIC(20, 0) /* {} (no unsigned in T-SQL) */", data_type),
        "Float16" | "Float32" => "REAL".to_string(),
        "Float64" => "FLOAT".to_string(),
        "Boolean" => "BIT".to_string(),
        "Utf8" | "LargeUtf8" | "String" => "NVARCHAR(MAX)".to_string(),
        "Date32" | "Date64" => "DATE".to_string(),
        "Binary" | "LargeBinary" => "VARBINARY(MAX)".to_string(),
        other => {
            if other.starts_with("Timestamp") {
                if other.contains("None") {
                    "DATETIME2".to_string()
                } else {
                    "DATETIMEOFFSET".to_string()
                }
            } else {
                format!("NVARCHAR(MAX) /* {} */", unknown_marker(other))
            }
        }
    }
}

fn databricks_type(data_type: &str) -> String {
    // Databricks SQL (Spark SQL / Delta Lake) type names.
    match data_type {
        "Int8" => "TINYINT".to_string(),
        "Int16" => "SMALLINT".to_string(),
        "Int32" => "INT".to_string(),
        "Int64" => "BIGINT".to_string(),
        // Spark SQL has no unsigned integer types - widen one bucket.
        // UInt64 can't round-trip; fall back to DECIMAL(20, 0).
        "UInt8" => "SMALLINT".to_string(),
        "UInt16" => "INT".to_string(),
        "UInt32" => "BIGINT".to_string(),
        "UInt64" => format!(
            "DECIMAL(20, 0) /* {} (no unsigned in Spark SQL) */",
            data_type
        ),
        "Float16" | "Float32" => "FLOAT".to_string(),
        "Float64" => "DOUBLE".to_string(),
        "Boolean" => "BOOLEAN".to_string(),
        "Utf8" | "LargeUtf8" | "String" => "STRING".to_string(),
        "Date32" | "Date64" => "DATE".to_string(),
        "Binary" | "LargeBinary" => "BINARY".to_string(),
        other => {
            if other.starts_with("Timestamp") {
                // TIMESTAMP is zoned (session tz); TIMESTAMP_NTZ is the
                // wall-clock type. Map the tz-less Arrow timestamp to
                // TIMESTAMP_NTZ, the zoned one to TIMESTAMP.
                if other.contains("None") {
                    "TIMESTAMP_NTZ".to_string()
                } else {
                    "TIMESTAMP".to_string()
                }
            } else {
                format!("STRING /* {} */", unknown_marker(other))
            }
        }
    }
}

fn snowflake_type(data_type: &str) -> String {
    // Snowflake's integer type names are all aliases of NUMBER(38, 0);
    // keep the readable spelling closest to the source width.
    match data_type {
        "Int8" => "TINYINT".to_string(),
        "Int16" => "SMALLINT".to_string(),
        "Int32" => "INTEGER".to_string(),
        "Int64" => "BIGINT".to_string(),
        // No unsigned types - widen one bucket; UInt64 can't round-trip.
        "UInt8" => "SMALLINT".to_string(),
        "UInt16" => "INTEGER".to_string(),
        "UInt32" => "BIGINT".to_string(),
        "UInt64" => format!(
            "NUMBER(20, 0) /* {} (no unsigned in Snowflake) */",
            data_type
        ),
        // Snowflake FLOAT / DOUBLE / REAL are all 64-bit IEEE doubles.
        "Float16" | "Float32" | "Float64" => "FLOAT".to_string(),
        "Boolean" => "BOOLEAN".to_string(),
        "Utf8" | "LargeUtf8" | "String" => "VARCHAR".to_string(),
        "Date32" | "Date64" => "DATE".to_string(),
        "Binary" | "LargeBinary" => "BINARY".to_string(),
        other => {
            if other.starts_with("Timestamp") {
                // TIMESTAMP_NTZ = no time zone, TIMESTAMP_TZ = tz-aware.
                if other.contains("None") {
                    "TIMESTAMP_NTZ".to_string()
                } else {
                    "TIMESTAMP_TZ".to_string()
                }
            } else {
                format!("VARCHAR /* {} */", unknown_marker(other))
            }
        }
    }
}

/// ClickHouse column types. Every type is wrapped `Nullable(...)` by the
/// caller, because ClickHouse columns are NOT NULL by default and any table
/// carrying nulls would be rejected.
fn clickhouse_type(data_type: &str) -> String {
    match data_type {
        "Int8" => "Int8".to_string(),
        "Int16" => "Int16".to_string(),
        "Int32" => "Int32".to_string(),
        "Int64" => "Int64".to_string(),
        "UInt8" => "UInt8".to_string(),
        "UInt16" => "UInt16".to_string(),
        "UInt32" => "UInt32".to_string(),
        "UInt64" => "UInt64".to_string(),
        "Float16" | "Float32" => "Float32".to_string(),
        "Float64" => "Float64".to_string(),
        // ClickHouse has no dedicated boolean; UInt8 is the documented
        // spelling and Bool is an alias of it.
        "Boolean" => "Bool".to_string(),
        "Utf8" | "LargeUtf8" | "String" => "String".to_string(),
        "Date32" | "Date64" => "Date32".to_string(),
        "Binary" | "LargeBinary" => "String".to_string(),
        other if other.starts_with("Timestamp") => "DateTime64(6)".to_string(),
        other => format!("String /* {other} */"),
    }
}

/// Exasol column types. Exasol has no unbounded VARCHAR, so strings take the
/// maximum width (2,000,000) rather than silently truncating.
fn exasol_type(data_type: &str) -> String {
    match data_type {
        // Widths are sized by range: DECIMAL(9,0) holds 9 digits, so Int32
        // (up to 2,147,483,647) and UInt32 (4,294,967,295) both need more.
        "Int8" | "Int16" | "UInt8" | "UInt16" => "DECIMAL(9,0)".to_string(),
        "Int32" | "Int64" | "UInt32" => "DECIMAL(19,0)".to_string(),
        // DECIMAL tops out at 36 digits, which covers UInt64's 20.
        "UInt64" => "DECIMAL(20,0)".to_string(),
        "Float16" | "Float32" | "Float64" => "DOUBLE PRECISION".to_string(),
        "Boolean" => "BOOLEAN".to_string(),
        "Utf8" | "LargeUtf8" | "String" => "VARCHAR(2000000)".to_string(),
        "Date32" | "Date64" => "DATE".to_string(),
        // Exasol has no binary type; hex text is the usual carrier.
        "Binary" | "LargeBinary" => "VARCHAR(2000000) /* binary as text */".to_string(),
        other if other.starts_with("Timestamp") => "TIMESTAMP".to_string(),
        other => format!("VARCHAR(2000000) /* {other} */"),
    }
}

/// BigQuery GoogleSQL column types.
fn bigquery_type(data_type: &str) -> String {
    match data_type {
        "Int8" | "Int16" | "Int32" | "Int64" | "UInt8" | "UInt16" | "UInt32" => "INT64".to_string(),
        // INT64 is signed; UInt64's top bit cannot round-trip.
        "UInt64" => "NUMERIC /* UInt64 (no unsigned in BigQuery) */".to_string(),
        "Float16" | "Float32" | "Float64" => "FLOAT64".to_string(),
        "Boolean" => "BOOL".to_string(),
        "Utf8" | "LargeUtf8" | "String" => "STRING".to_string(),
        "Date32" | "Date64" => "DATE".to_string(),
        "Binary" | "LargeBinary" => "BYTES".to_string(),
        other if other.starts_with("Timestamp") => "TIMESTAMP".to_string(),
        other => format!("STRING /* {other} */"),
    }
}

#[cfg(test)]
mod tests {
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
            "CREATE TABLE analytics.orders (id BIGINT, label VARCHAR)"
        );
    }

    #[test]
    fn live_databricks_ddl_uses_databricks_types() {
        let cols = vec![ColumnInfo {
            name: "label".to_string(),
            data_type: "Utf8".to_string(),
        }];
        let sql = create_table_qualified(&cols, LiveSqlDialect::Databricks, "main", "t");
        assert_eq!(sql, "CREATE TABLE main.t (label STRING)");
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
            "CREATE TABLE default.events (id Nullable(Int64), label Nullable(String)) \
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
            "CREATE TABLE analytics.orders (id INT64, ok BOOL, label STRING)"
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
            "CREATE TABLE s.t (id DECIMAL(19,0), label VARCHAR(2000000))"
        );
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
        assert_eq!(sql, "CREATE TABLE t (`order total` FLOAT64)");
    }

    #[test]
    fn existing_dialects_gain_no_table_suffix() {
        let cols = vec![ColumnInfo {
            name: "id".to_string(),
            data_type: "Int64".to_string(),
        }];
        assert_eq!(
            create_table_qualified(&cols, LiveSqlDialect::Postgres, "public", "t"),
            "CREATE TABLE public.t (id BIGINT)"
        );
    }
}
