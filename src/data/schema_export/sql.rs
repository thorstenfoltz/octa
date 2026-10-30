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
    Redshift,
    Mysql,
    Mssql,
    Oracle,
    Snowflake,
    Databricks,
    ClickHouse,
    Exasol,
    Trino,
    BigQuery,
    Sqlite,
}

/// The internal dialect backing one live-write dialect. Single mapping point,
/// so `create_table_qualified` and `column_type_sql` can never disagree.
fn live_dialect(dialect: LiveSqlDialect) -> Dialect {
    match dialect {
        LiveSqlDialect::Postgres => Dialect::Postgres,
        LiveSqlDialect::Redshift => Dialect::Redshift,
        LiveSqlDialect::Mysql => Dialect::Mysql,
        LiveSqlDialect::Mssql => Dialect::Mssql,
        LiveSqlDialect::Oracle => Dialect::Oracle,
        LiveSqlDialect::Snowflake => Dialect::Snowflake,
        LiveSqlDialect::Databricks => Dialect::Databricks,
        LiveSqlDialect::ClickHouse => Dialect::ClickHouse,
        LiveSqlDialect::Exasol => Dialect::Exasol,
        LiveSqlDialect::Trino => Dialect::Trino,
        LiveSqlDialect::BigQuery => Dialect::BigQuery,
        LiveSqlDialect::Sqlite => Dialect::Sqlite,
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
    // `quote_always`, not `quote_ident`: this DDL is paired with an INSERT
    // that always quotes, and an engine that folds unquoted identifiers
    // (Postgres to lower, Snowflake to upper) would otherwise create a
    // differently-named column from the one the INSERT addresses.
    let target = if schema.is_empty() {
        d.quote_always(table)
    } else {
        format!("{}.{}", d.quote_always(schema), d.quote_always(table))
    };
    let cols: Vec<String> = columns
        .iter()
        .map(|c| {
            format!(
                "{} {}",
                d.quote_always(&c.name),
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
    Redshift,
    Mysql,
    Sqlite,
    Databricks,
    Snowflake,
    Mssql,
    Oracle,
    ClickHouse,
    Exasol,
    Trino,
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
        self.quote_always(ident)
    }

    /// Like [`quote_ident`](Self::quote_ident) but never emits an identifier
    /// bare, for DDL that has to agree with SQL generated elsewhere.
    ///
    /// `db::write_table_generic` follows its CREATE with an INSERT whose
    /// column list is quoted by `DbEngine::quote_ident`, which always quotes.
    /// Emitting `ID` bare in the CREATE and `"ID"` in the INSERT made Postgres
    /// fold the created column to `id` and then fail the insert on a column
    /// that did not exist - so every table with a capital letter in a column
    /// or table name was unwritable. The two quote tables agree character for
    /// character on every engine; the bare-when-safe shortcut was the only
    /// difference, so dropping it here is what makes them one.
    fn quote_always(self, ident: &str) -> String {
        match self {
            // Postgres / SQLite / Snowflake accept double-quoted
            // identifiers. Escape an embedded `"` as `""`.
            Dialect::Postgres | Dialect::Redshift | Dialect::Sqlite | Dialect::Snowflake => {
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
            // Oracle, Exasol and Trino accept double-quoted identifiers.
            Dialect::Oracle | Dialect::Exasol | Dialect::Trino => {
                format!("\"{}\"", ident.replace('"', "\"\""))
            }
        }
    }

    fn map_type(self, data_type: &str) -> String {
        if let Some(kind) = crate::data::times::TimeKind::of_type(data_type) {
            return time_ddl(self, kind).to_string();
        }
        match self {
            Dialect::Postgres | Dialect::Redshift => pg_type(data_type),
            Dialect::Mysql => mysql_type(data_type),
            Dialect::Sqlite => sqlite_type(data_type),
            Dialect::Databricks => databricks_type(data_type),
            Dialect::Snowflake => snowflake_type(data_type),
            Dialect::Mssql => mssql_type(data_type),
            Dialect::ClickHouse => clickhouse_type(data_type),
            Dialect::Oracle => oracle_type(data_type),
            Dialect::Exasol => exasol_type(data_type),
            Dialect::Trino => trino_type(data_type),
            Dialect::BigQuery => bigquery_type(data_type),
        }
    }

    fn header_comment(self) -> &'static str {
        match self {
            Dialect::Postgres => "-- Generated by octa - Postgres dialect",
            Dialect::Redshift => "-- Generated by octa - Redshift dialect",
            Dialect::Mysql => "-- Generated by octa - MySQL dialect",
            Dialect::Sqlite => "-- Generated by octa - SQLite dialect",
            Dialect::Databricks => "-- Generated by octa - Databricks dialect",
            Dialect::Snowflake => "-- Generated by octa - Snowflake dialect",
            Dialect::Mssql => "-- Generated by octa - SQL Server dialect",
            Dialect::ClickHouse => "-- Generated by octa - ClickHouse dialect",
            Dialect::Oracle => "-- Generated by octa - Oracle dialect",
            Dialect::Exasol => "-- Generated by octa - Exasol dialect",
            Dialect::Trino => "-- Generated by octa - Trino dialect",
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

/// Column type for a clock Time or a Duration. Where a server has no such
/// type (SQL Server and Snowflake intervals, Oracle / Databricks / Exasol
/// TIME) the column is text and `sql_literal` writes the clock text.
/// ClickHouse keeps `String` because `Time64` needs a recent server.
fn time_ddl(dialect: Dialect, kind: crate::data::times::TimeKind) -> &'static str {
    use crate::data::times::TimeKind::{Duration, Time};
    match (dialect, kind) {
        (Dialect::Postgres, Time) | (Dialect::BigQuery, Time) => "TIME",
        (Dialect::Postgres, Duration) | (Dialect::BigQuery, Duration) => "INTERVAL",
        (Dialect::Redshift, Time) => "TIME",
        // INTERVAL is not a column type on Redshift.
        (Dialect::Redshift, Duration) => "VARCHAR(64)",
        (Dialect::Mysql, _) => "TIME(6)",
        (Dialect::Sqlite, _) => "TEXT",
        (Dialect::Databricks, Time) => "STRING",
        (Dialect::Databricks, Duration) => "INTERVAL DAY TO SECOND",
        (Dialect::Snowflake, Time) => "TIME(9)",
        (Dialect::Snowflake, Duration) | (Dialect::Trino, Duration) => "VARCHAR",
        (Dialect::Mssql, Time) => "TIME(7)",
        (Dialect::Mssql, Duration) => "NVARCHAR(64)",
        (Dialect::Oracle, Time) => "VARCHAR2(32)",
        (Dialect::Oracle, Duration) | (Dialect::Exasol, Duration) => "INTERVAL DAY(9) TO SECOND(9)",
        (Dialect::ClickHouse, _) => "String",
        (Dialect::Exasol, Time) => "VARCHAR(32)",
        (Dialect::Trino, Time) => "TIME(6)",
    }
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
                if super::super::timestamp_timezone(other).is_none() {
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
                if super::super::timestamp_timezone(other).is_none() {
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
                if super::super::timestamp_timezone(other).is_none() {
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
                if super::super::timestamp_timezone(other).is_none() {
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

/// Oracle column types. NUMBER is the only numeric type, so the integer
/// buckets are precisions of it; `BINARY_FLOAT`/`BINARY_DOUBLE` are the real
/// floating-point ones.
fn oracle_type(data_type: &str) -> String {
    match data_type {
        "Int8" | "UInt8" => "NUMBER(3,0)".to_string(),
        "Int16" | "UInt16" => "NUMBER(5,0)".to_string(),
        "Int32" | "UInt32" => "NUMBER(10,0)".to_string(),
        "Int64" => "NUMBER(19,0)".to_string(),
        "UInt64" => "NUMBER(20,0)".to_string(),
        // NOT BINARY_FLOAT / BINARY_DOUBLE, which would be the closer match:
        // `src/db/oracle.rs` cannot read those back (the driver does not decode
        // them), so writing one would create a column Octa itself then shows as
        // unreadable. NUMBER round-trips.
        "Float16" | "Float32" | "Float64" => "NUMBER".to_string(),
        // Oracle had no BOOLEAN in SQL before 23c, and NUMBER(1) is how every
        // schema older than that spells one.
        "Boolean" => "NUMBER(1)".to_string(),
        // VARCHAR2 is bounded at 4000 bytes. A wider column has to be a CLOB,
        // which no INSERT can fill with a literal anyway (Oracle caps a string
        // literal at 4000 too), so the bound is the honest choice here.
        "Utf8" | "LargeUtf8" | "String" => "VARCHAR2(4000)".to_string(),
        "Date32" | "Date64" => "DATE".to_string(),
        // Binary rows are written as the hex text `CellValue` displays, so the
        // column that receives them is text, not a BLOB.
        "Binary" | "LargeBinary" => "VARCHAR2(4000) /* binary as hex text */".to_string(),
        other if other.starts_with("Timestamp") => {
            if super::super::timestamp_timezone(other).is_none() {
                "TIMESTAMP".to_string()
            } else {
                "TIMESTAMP WITH TIME ZONE".to_string()
            }
        }
        other => format!("VARCHAR2(4000) /* {other} */"),
    }
}

/// Trino (ANSI SQL) column types. Trino's own type names, not the underlying
/// connector's: a `CREATE TABLE` goes through Trino, which translates into
/// whatever Hive, Iceberg or Delta wants.
fn trino_type(data_type: &str) -> String {
    match data_type {
        "Int8" | "Int16" | "UInt8" => "SMALLINT".to_string(),
        "Int32" | "UInt16" => "INTEGER".to_string(),
        "Int64" | "UInt32" => "BIGINT".to_string(),
        // Trino has no unsigned types; DECIMAL(20,0) holds UInt64's range.
        "UInt64" => "DECIMAL(20,0)".to_string(),
        "Float16" | "Float32" => "REAL".to_string(),
        "Float64" => "DOUBLE".to_string(),
        "Boolean" => "BOOLEAN".to_string(),
        "Utf8" | "LargeUtf8" | "String" => "VARCHAR".to_string(),
        "Date32" | "Date64" => "DATE".to_string(),
        "Binary" | "LargeBinary" => "VARBINARY".to_string(),
        other if other.starts_with("Timestamp") => {
            if super::super::timestamp_timezone(other).is_none() {
                "TIMESTAMP(6)".to_string()
            } else {
                "TIMESTAMP(6) WITH TIME ZONE".to_string()
            }
        }
        other => format!("VARCHAR /* {other} */"),
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
#[path = "sql_tests.rs"]
mod tests;
