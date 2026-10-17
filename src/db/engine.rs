//! The engines Octa connects to, and what each one can and cannot do:
//! labels, ports, identifier quoting, auth methods, catalog levels.

use serde::{Deserialize, Serialize};

use super::DbAuthKind;

/// Supported database engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DbEngine {
    #[default]
    Postgres,
    MySql,
    Mssql,
    Oracle,
    Redshift,
    ClickHouse,
    Exasol,
    Trino,
    Athena,
    Snowflake,
    Databricks,
    BigQuery,
}

impl DbEngine {
    pub const ALL: &[DbEngine] = &[
        DbEngine::Postgres,
        DbEngine::MySql,
        DbEngine::Mssql,
        DbEngine::Oracle,
        DbEngine::Redshift,
        DbEngine::ClickHouse,
        DbEngine::Exasol,
        DbEngine::Trino,
        DbEngine::Athena,
        DbEngine::Snowflake,
        DbEngine::Databricks,
        DbEngine::BigQuery,
    ];

    /// Human-readable label for pickers and status lines.
    pub fn label(self) -> &'static str {
        match self {
            DbEngine::Postgres => "PostgreSQL",
            DbEngine::MySql => "MySQL / MariaDB",
            DbEngine::Mssql => "SQL Server",
            DbEngine::Oracle => "Oracle",
            DbEngine::Redshift => "Amazon Redshift",
            DbEngine::ClickHouse => "ClickHouse",
            DbEngine::Exasol => "Exasol",
            DbEngine::Trino => "Trino",
            DbEngine::Athena => "Amazon Athena",
            DbEngine::Snowflake => "Snowflake",
            DbEngine::Databricks => "Databricks",
            DbEngine::BigQuery => "Google BigQuery",
        }
    }

    /// The engine's conventional TCP port.
    pub fn default_port(self) -> u16 {
        match self {
            DbEngine::Postgres => 5432,
            DbEngine::MySql => 3306,
            DbEngine::Mssql => 1433,
            DbEngine::Oracle => 1521,
            DbEngine::Redshift => 5439,
            DbEngine::ClickHouse => 8123,
            DbEngine::Exasol => 8563,
            // Trino's own default is 8080, but that is the plaintext port a
            // laptop coordinator listens on; a served cluster is TLS.
            DbEngine::Trino => 8443,
            DbEngine::Athena | DbEngine::Snowflake | DbEngine::Databricks | DbEngine::BigQuery => {
                443
            }
        }
    }

    /// Quote an identifier in the engine's own dialect (embedded quote
    /// characters are doubled).
    pub fn quote_ident(self, ident: &str) -> String {
        match self {
            DbEngine::MySql | DbEngine::ClickHouse | DbEngine::Databricks | DbEngine::BigQuery => {
                format!("`{}`", ident.replace('`', "``"))
            }
            DbEngine::Mssql => format!("[{}]", ident.replace(']', "]]")),
            // Postgres, Redshift, Oracle, Exasol, Trino, Snowflake
            _ => format!("\"{}\"", ident.replace('"', "\"\"")),
        }
    }

    /// SQL for "this value, as text" - the cast a case-insensitive contains
    /// needs before it can compare a number or a date against a search box.
    ///
    /// Every engine has one; none of them spell it the same way.
    pub(crate) fn as_text(self, expr: &str) -> String {
        match self {
            DbEngine::MySql => format!("CAST({expr} AS CHAR)"),
            DbEngine::Mssql => format!("CAST({expr} AS NVARCHAR(MAX))"),
            DbEngine::Oracle => format!("TO_CHAR({expr})"),
            DbEngine::ClickHouse => format!("toString({expr})"),
            // Databricks (Spark) refuses VARCHAR without a length.
            DbEngine::BigQuery | DbEngine::Databricks => format!("CAST({expr} AS STRING)"),
            // Exasol requires a length; this is its maximum.
            DbEngine::Exasol => format!("CAST({expr} AS VARCHAR(2000000))"),
            // A bare VARCHAR is VARCHAR(256) on Redshift, too short for text.
            DbEngine::Redshift => format!("CAST({expr} AS VARCHAR(65535))"),
            // Postgres, Trino, Athena, Snowflake
            _ => format!("CAST({expr} AS VARCHAR)"),
        }
    }

    /// A `WHERE` fragment matching rows where **any** of `columns` contains
    /// `needle`, honouring the search bar's `Aa` toggle.
    ///
    /// The needle becomes a single-quoted literal with its quotes doubled and
    /// its LIKE wildcards escaped, and the identifiers are quoted by the
    /// engine's own rule, so nothing the user typed is ever read as SQL. A
    /// case-insensitive comparison is `UPPER(...) LIKE UPPER(...)` rather than
    /// `ILIKE`, which only Postgres and its relatives have.
    ///
    /// Returns `None` when there are no columns to match against.
    pub fn contains_predicate(
        self,
        columns: &[String],
        needle: &str,
        case_sensitive: bool,
    ) -> Option<String> {
        let needle = needle.trim();
        if needle.is_empty() || columns.is_empty() {
            return None;
        }
        // `\` first, or it would escape the escapes added after it.
        let literal = needle
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
            .replace('\'', "''");
        let clauses: Vec<String> = columns
            .iter()
            .map(|c| {
                let col = self.as_text(&self.quote_ident(c));
                if case_sensitive {
                    format!("{col} LIKE '%{literal}%' ESCAPE '\\'")
                } else {
                    format!("UPPER({col}) LIKE UPPER('%{literal}%') ESCAPE '\\'")
                }
            })
            .collect();
        Some(format!("({})", clauses.join(" OR ")))
    }

    /// Whether a plain `UPDATE ... WHERE` / `DELETE ... WHERE` edits one row,
    /// which is what the full-row write-back fallback needs.
    ///
    /// ClickHouse spells it `ALTER TABLE ... UPDATE` and applies it
    /// asynchronously, and the three catalog warehouses have limited or
    /// non-transactional DML, so a table without a key stays read-only there
    /// rather than being edited by a statement that may not do what the
    /// confirmation dialog just promised.
    pub fn supports_row_update(self) -> bool {
        matches!(
            self,
            DbEngine::Postgres
                | DbEngine::MySql
                | DbEngine::Mssql
                | DbEngine::Oracle
                | DbEngine::Redshift
                | DbEngine::Exasol
        )
    }

    /// Whether the server itself keeps a connection without "Allow writes"
    /// from writing: a read-only session on PostgreSQL and MySQL / MariaDB,
    /// ClickHouse's `readonly` setting, and on SQL Server a transaction that
    /// is rolled back after every batch. The others rely on Octa's check of
    /// the statement and on the rights of the database user.
    pub fn enforces_read_only(self) -> bool {
        matches!(
            self,
            DbEngine::Postgres | DbEngine::MySql | DbEngine::Mssql | DbEngine::ClickHouse
        )
    }

    /// Whether DuckDB can `ATTACH` this engine natively (via its
    /// `postgres`/`mysql` extensions). Redshift speaks the Postgres wire
    /// protocol so it rides the same extension.
    pub fn duckdb_attachable(self) -> bool {
        matches!(
            self,
            DbEngine::Postgres | DbEngine::MySql | DbEngine::Redshift
        )
    }

    /// Auth methods this engine offers, in picker order (first = default).
    pub fn supported_auth(self) -> &'static [DbAuthKind] {
        use DbAuthKind::*;
        match self {
            DbEngine::Postgres | DbEngine::MySql => &[Password, AwsIam, AzureAd, GcpIam],
            DbEngine::Mssql => &[Password, AzureAd],
            // Oracle wallets and Kerberos are out of scope, so password is the
            // whole list rather than the first entry of one.
            DbEngine::Oracle => &[Password],
            DbEngine::Redshift => &[Password, AwsIam],
            DbEngine::ClickHouse | DbEngine::Exasol => &[Password],
            // Basic auth over TLS, or the bearer a Trino cluster's OIDC
            // provider issues (browser SSO is the common enterprise setup).
            DbEngine::Trino => &[Password, OAuthBrowser, Token],
            // Athena signs every request; the credentials come from the same
            // chain as the other AWS surfaces, so there is no password to give.
            DbEngine::Athena => &[AwsIam],
            DbEngine::Snowflake => &[KeyPairJwt, Password, OAuthBrowser, OAuthClientCredentials],
            DbEngine::Databricks => &[Token, AzureAd, OAuthClientCredentials, OAuthBrowser],
            DbEngine::BigQuery => &[GcpAdc, GcpServiceAccount],
        }
    }

    /// Whether this engine has a top `catalog` level above `schema`
    /// (`catalog.schema.table`) that Octa browses. Snowflake (database),
    /// Databricks (catalog), BigQuery (project). The other engines are either
    /// two-level or reach only their connected database.
    pub fn has_catalogs(self) -> bool {
        matches!(
            self,
            DbEngine::Trino | DbEngine::Snowflake | DbEngine::Databricks | DbEngine::BigQuery
        )
    }

    /// Whether the engine reaches its results by asking "are you done yet?"
    /// over HTTP, which is what makes a give-up point Octa's to choose.
    ///
    /// These five submit a statement and then poll (or, for Trino, follow a
    /// chain of `nextUri` links) until the server says it has finished. The
    /// wire protocols block on a socket inside their driver instead, so their
    /// timeout belongs to the driver and the server, not to
    /// `query_timeout_secs`, and the Settings field is hidden for them rather
    /// than shown doing nothing.
    pub fn polls_for_results(self) -> bool {
        matches!(
            self,
            DbEngine::Trino
                | DbEngine::Athena
                | DbEngine::Snowflake
                | DbEngine::Databricks
                | DbEngine::BigQuery
        )
    }

    /// Whether the engine has foreign keys to read at all. ClickHouse is the
    /// one that does not: it has no referential constraints of any kind, so
    /// there is nothing for the relationship map to ask it. The rest declare
    /// them, though Redshift, Snowflake and BigQuery accept a declaration
    /// without enforcing it.
    pub fn has_foreign_keys(self) -> bool {
        !matches!(
            self,
            DbEngine::ClickHouse | DbEngine::Trino | DbEngine::Athena
        )
    }

    /// Whether a declared foreign key is also a fact about the rows.
    ///
    /// The four warehouses accept `REFERENCES` and never check it, so a child
    /// value pointing at a parent that does not exist is possible there. That
    /// is the difference between reading the catalog and measuring the data,
    /// and it is why the relationship map offers to measure.
    pub fn enforces_foreign_keys(self) -> bool {
        matches!(
            self,
            DbEngine::Postgres
                | DbEngine::MySql
                | DbEngine::Mssql
                | DbEngine::Oracle
                | DbEngine::Exasol
        )
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod engine_tests;
