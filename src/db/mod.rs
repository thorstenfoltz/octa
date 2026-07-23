//! Live database connections: Postgres, MySQL/MariaDB, SQL Server, Redshift,
//! ClickHouse, Exasol, Snowflake, Databricks, BigQuery.
//!
//! One file per engine behind the [`DbConnector`] trait (the
//! `src/formats/mod.rs` drop-in model), so a new engine is one more file, not
//! a refactor. The wire-protocol engines' client crates are async;
//! [`runtime`] blocks on them the same way `crate::cloud::runtime` does, so
//! everything above the trait stays sync; the warehouse engines (Snowflake,
//! Databricks, BigQuery) talk REST/HTTP via [`rest`] instead.
//!
//! [`DbEngine::duckdb_attachable`] marks the engines DuckDB can `ATTACH`
//! natively (Postgres/MySQL/Redshift); the rest are imported table-by-table.
//! [`DbConnector::fetch_batches`] gives every connector a paged read, which
//! [`copy`]'s universal lane uses to copy between any two engines.
//!
//! Connections are saved in `AppSettings.db_connections`; the secret
//! (password / token) lives in the system keyring keyed by the connection's
//! frozen `id` (`src/ui/settings/db_secrets.rs`). Writes are gated per
//! connection: [`ensure_write_allowed`] is the single chokepoint every
//! surface (GUI, SQL panel, CLI, MCP, chat) routes through.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::data::DataTable;

pub mod auth;
pub mod bigquery;
pub mod clickhouse;
pub mod copy;
pub mod databricks;
pub mod exasol;
pub mod mssql;
pub mod mysql;
pub mod postgres;
pub(crate) mod rest;
pub mod snowflake;
pub mod write_back;

/// Supported database engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DbEngine {
    #[default]
    Postgres,
    MySql,
    Mssql,
    Redshift,
    ClickHouse,
    Exasol,
    Snowflake,
    Databricks,
    BigQuery,
}

impl DbEngine {
    pub const ALL: &[DbEngine] = &[
        DbEngine::Postgres,
        DbEngine::MySql,
        DbEngine::Mssql,
        DbEngine::Redshift,
        DbEngine::ClickHouse,
        DbEngine::Exasol,
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
            DbEngine::Redshift => "Amazon Redshift",
            DbEngine::ClickHouse => "ClickHouse",
            DbEngine::Exasol => "Exasol",
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
            DbEngine::Redshift => 5439,
            DbEngine::ClickHouse => 8123,
            DbEngine::Exasol => 8563,
            DbEngine::Snowflake | DbEngine::Databricks | DbEngine::BigQuery => 443,
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
            // Postgres, Redshift, Exasol, Snowflake
            _ => format!("\"{}\"", ident.replace('"', "\"\"")),
        }
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
            DbEngine::Redshift => &[Password, AwsIam],
            DbEngine::ClickHouse | DbEngine::Exasol => &[Password],
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
            DbEngine::Snowflake | DbEngine::Databricks | DbEngine::BigQuery
        )
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;

    #[test]
    fn all_lists_nine_engines() {
        assert_eq!(DbEngine::ALL.len(), 9);
        assert!(DbEngine::ALL.contains(&DbEngine::Snowflake));
    }

    #[test]
    fn quoting_matches_dialect() {
        assert_eq!(DbEngine::ClickHouse.quote_ident("a`b"), "`a``b`");
        assert_eq!(DbEngine::Snowflake.quote_ident("a\"b"), "\"a\"\"b\"");
        assert_eq!(DbEngine::BigQuery.quote_ident("x"), "`x`");
    }

    #[test]
    fn has_catalogs_only_warehouses() {
        for e in [
            DbEngine::Snowflake,
            DbEngine::Databricks,
            DbEngine::BigQuery,
        ] {
            assert!(e.has_catalogs(), "{e:?} should have catalogs");
        }
        for e in [
            DbEngine::Postgres,
            DbEngine::MySql,
            DbEngine::Mssql,
            DbEngine::Redshift,
            DbEngine::ClickHouse,
            DbEngine::Exasol,
        ] {
            assert!(!e.has_catalogs(), "{e:?} should not have catalogs");
        }
    }

    #[test]
    fn bigquery_quotes_with_backticks() {
        assert_eq!(DbEngine::BigQuery.quote_ident("a`b"), "`a``b`");
    }

    #[test]
    fn duckdb_attachable_only_pg_mysql_redshift() {
        assert!(DbEngine::Postgres.duckdb_attachable());
        assert!(DbEngine::Redshift.duckdb_attachable());
        assert!(!DbEngine::Snowflake.duckdb_attachable());
        assert!(!DbEngine::ClickHouse.duckdb_attachable());
    }

    #[test]
    fn supported_auth_gating() {
        assert_eq!(
            DbEngine::Snowflake.supported_auth(),
            &[
                DbAuthKind::KeyPairJwt,
                DbAuthKind::Password,
                DbAuthKind::OAuthBrowser,
                DbAuthKind::OAuthClientCredentials
            ]
        );
        assert_eq!(
            DbEngine::BigQuery.supported_auth(),
            &[DbAuthKind::GcpAdc, DbAuthKind::GcpServiceAccount]
        );
        assert_eq!(
            DbEngine::Databricks.supported_auth(),
            &[
                DbAuthKind::Token,
                DbAuthKind::AzureAd,
                DbAuthKind::OAuthClientCredentials,
                DbAuthKind::OAuthBrowser
            ]
        );
        assert_eq!(
            DbEngine::ClickHouse.supported_auth(),
            &[DbAuthKind::Password]
        );
    }

    #[test]
    fn dbauth_serde_roundtrip() {
        let a = DbAuth::KeyPairJwt {
            private_key_path: "/k.pem".into(),
        };
        let s = serde_json::to_string(&a).unwrap();
        assert_eq!(serde_json::from_str::<DbAuth>(&s).unwrap(), a);
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use crate::data::{CellValue, ColumnInfo, DataTable};

    #[test]
    fn paged_sql_dialects() {
        assert_eq!(
            paged_sql(DbEngine::Postgres, "SELECT * FROM t", 100, 200),
            "SELECT * FROM (SELECT * FROM t) AS _octa_page LIMIT 100 OFFSET 200"
        );
        assert!(
            paged_sql(DbEngine::Mssql, "SELECT * FROM t", 100, 0)
                .contains("OFFSET 0 ROWS FETCH NEXT 100 ROWS ONLY")
        );
    }

    /// 4-line test-only parser: pull (limit, offset) back out of a paged sql.
    fn parse_limit_offset(sql: &str, engine: DbEngine) -> (usize, usize) {
        let grab = |kw: &str| -> usize {
            sql.split(kw)
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
        };
        match engine {
            DbEngine::Mssql => (grab("FETCH NEXT"), grab("OFFSET")),
            _ => (grab("LIMIT"), grab("OFFSET")),
        }
    }

    struct FakeConn {
        rows: usize,
        engine: DbEngine,
    }
    impl DbConnector for FakeConn {
        fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn engine(&self) -> DbEngine {
            self.engine
        }
        fn execute(&mut self, _: &str) -> anyhow::Result<u64> {
            Ok(0)
        }
        fn write_table(
            &mut self,
            _: Option<&str>,
            _: &str,
            _: &str,
            _: DbWriteMode,
            _: &DataTable,
        ) -> anyhow::Result<DbWriteReport> {
            unimplemented!()
        }
        fn query(&mut self, sql: &str) -> anyhow::Result<DataTable> {
            let (limit, offset) = parse_limit_offset(sql, self.engine);
            let n = self.rows.saturating_sub(offset).min(limit);
            let mut t = DataTable::empty();
            t.columns = vec![ColumnInfo {
                name: "id".into(),
                data_type: "Int64".into(),
            }];
            t.rows = (0..n)
                .map(|i| vec![CellValue::Int((offset + i) as i64)])
                .collect();
            Ok(t)
        }
    }

    #[test]
    fn fetch_batches_streams_all_without_cap() {
        let mut c = FakeConn {
            rows: 250,
            engine: DbEngine::Postgres,
        };
        let mut total = 0;
        c.fetch_batches("SELECT * FROM t", 100, &mut |b| {
            total += b.row_count();
            Ok(())
        })
        .unwrap();
        assert_eq!(total, 250); // 100 + 100 + 50, no cap
    }
}

/// How a connection authenticates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DbAuth {
    /// Password from the keyring (`db_secrets`).
    #[default]
    Password,
    /// AWS RDS IAM token, minted per connect via
    /// `aws rds generate-db-auth-token` (Postgres / MySQL). Without the
    /// `sso_*` fields the browser part is the user running `aws sso login`;
    /// with them Octa drives the IAM Identity Center device-authorization flow
    /// in-app and mints role credentials itself (see `db::auth::aws_sso_*`).
    AwsIam {
        #[serde(default)]
        region: Option<String>,
        /// IAM Identity Center portal start URL (e.g.
        /// `https://acme.awsapps.com/start`). Set = in-app browser sign-in.
        #[serde(default)]
        sso_start_url: Option<String>,
        /// Identity Center region (falls back to `region` when omitted).
        #[serde(default)]
        sso_region: Option<String>,
        /// AWS account id the role lives in.
        #[serde(default)]
        sso_account_id: Option<String>,
        /// IAM role name to assume for database access.
        #[serde(default)]
        sso_role: Option<String>,
    },
    /// Microsoft Entra (Azure AD) token via `az account get-access-token`.
    /// SQL Server plus Azure Database for PostgreSQL / MySQL flexible
    /// servers (the token resource differs per engine). The browser part is
    /// the user running `az login`.
    AzureAd,
    /// Google Cloud SQL IAM database auth token via
    /// `gcloud sql generate-login-token` (Postgres / MySQL). The browser
    /// part is the user running `gcloud auth login`; the username must be
    /// the IAM principal (e.g. user@example.com, or the service-account
    /// name without the `.gserviceaccount.com` suffix for MySQL).
    GcpIam,
    /// Static access token / personal access token (Databricks PAT).
    Token,
    /// Snowflake key-pair auth: a locally minted JWT signed with the RSA
    /// private key at `private_key_path`.
    KeyPairJwt { private_key_path: String },
    /// OAuth 2.0 client-credentials (machine-to-machine) grant. `token_url`
    /// defaults per engine when omitted.
    OAuthClientCredentials {
        client_id: String,
        #[serde(default)]
        token_url: Option<String>,
    },
    /// OAuth external-browser SSO: Octa opens the browser and catches the
    /// redirect on an ephemeral localhost port (Snowflake).
    OAuthBrowser,
    /// Google Application Default Credentials (`gcloud auth application-default
    /// login` / ambient metadata) for BigQuery.
    GcpAdc,
    /// BigQuery service-account key JSON at `key_path`.
    GcpServiceAccount { key_path: String },
}

/// Fieldless mirror of [`DbAuth`] for gating pickers per engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbAuthKind {
    Password,
    AwsIam,
    AzureAd,
    GcpIam,
    Token,
    KeyPairJwt,
    OAuthClientCredentials,
    OAuthBrowser,
    GcpAdc,
    GcpServiceAccount,
}

impl DbAuthKind {
    /// i18n key suffix under `[db]`; the label itself comes from `t()`.
    pub fn i18n_key(self) -> &'static str {
        match self {
            DbAuthKind::Password => "auth_password",
            DbAuthKind::AwsIam => "auth_aws_iam",
            DbAuthKind::AzureAd => "auth_azure_ad",
            DbAuthKind::GcpIam => "auth_gcp_iam",
            DbAuthKind::Token => "auth_token",
            DbAuthKind::KeyPairJwt => "auth_keypair",
            DbAuthKind::OAuthClientCredentials => "auth_oauth_m2m",
            DbAuthKind::OAuthBrowser => "auth_oauth_browser",
            DbAuthKind::GcpAdc => "auth_gcp_adc",
            DbAuthKind::GcpServiceAccount => "auth_gcp_sa",
        }
    }
}

impl DbAuth {
    /// The fieldless kind of this auth method.
    pub fn kind(&self) -> DbAuthKind {
        match self {
            DbAuth::Password => DbAuthKind::Password,
            DbAuth::AwsIam { .. } => DbAuthKind::AwsIam,
            DbAuth::AzureAd => DbAuthKind::AzureAd,
            DbAuth::GcpIam => DbAuthKind::GcpIam,
            DbAuth::Token => DbAuthKind::Token,
            DbAuth::KeyPairJwt { .. } => DbAuthKind::KeyPairJwt,
            DbAuth::OAuthClientCredentials { .. } => DbAuthKind::OAuthClientCredentials,
            DbAuth::OAuthBrowser => DbAuthKind::OAuthBrowser,
            DbAuth::GcpAdc => DbAuthKind::GcpAdc,
            DbAuth::GcpServiceAccount { .. } => DbAuthKind::GcpServiceAccount,
        }
    }
}

/// A saved database connection (secret excluded; that lives in the keyring).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DbConnection {
    /// Frozen at creation; names the keyring entry. Never regenerate.
    pub id: String,
    pub name: String,
    pub engine: DbEngine,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    #[serde(default)]
    pub auth: DbAuth,
    /// Writes (INSERT/UPDATE/DDL, write-back) are refused while false.
    /// Default false: read-only until the user explicitly opts in.
    #[serde(default)]
    pub allow_writes: bool,
    /// BYO OAuth client id for native browser sign-in (Azure AD / GCP IAM
    /// fallback when the vendor CLI is unavailable). None = browser sign-in not
    /// configured; only the CLI path is available.
    #[serde(default)]
    pub oauth_client_id: Option<String>,
    /// Azure tenant id/domain for the browser authorize/token endpoints (Azure
    /// only; ignored for GCP). None with Azure = use "organizations".
    #[serde(default)]
    pub oauth_tenant: Option<String>,
}

impl DbConnection {
    /// Mint a unique frozen id for a new connection.
    pub fn fresh_id() -> String {
        format!(
            "db-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )
    }
}

/// Write mode for [`DbConnector::write_table`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbWriteMode {
    /// CREATE the table (error if it exists) then insert.
    Create,
    /// INSERT into the existing table.
    Append,
    /// DROP IF EXISTS + CREATE + insert.
    Replace,
}

/// Outcome of a [`DbConnector::write_table`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbWriteReport {
    pub rows_written: usize,
    pub created: bool,
}

/// Wrap `base_sql` in a `LIMIT`/`OFFSET` page for the given engine. Used by
/// the universal copy lane to stream a table in bounded batches. Most engines
/// take standard `LIMIT n OFFSET m`; SQL Server needs the `OFFSET ... FETCH`
/// form (which requires an `ORDER BY`, so a stable no-op sort is supplied).
pub(crate) fn paged_sql(engine: DbEngine, base_sql: &str, limit: usize, offset: usize) -> String {
    match engine {
        DbEngine::Mssql => format!(
            "SELECT * FROM ({base_sql}) AS _octa_page \
             ORDER BY (SELECT NULL) OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY"
        ),
        _ => format!("SELECT * FROM ({base_sql}) AS _octa_page LIMIT {limit} OFFSET {offset}"),
    }
}

/// A shared cancel latch. Cloned into a connector's cancel closure, which
/// runs on another thread while the connector itself is mutably borrowed by
/// the in-flight query.
#[derive(Clone, Default)]
pub struct CancelFlag(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl CancelFlag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Clear the latch before starting a new statement, so one cancel does
    /// not poison every later query on the same connector.
    pub fn reset(&self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Sync interface every engine implements. `query` sends the SQL text
/// verbatim (native dialect); type mapping into [`DataTable`] happens in the
/// engine file.
pub trait DbConnector: Send {
    /// Top namespace level for three-level engines; empty = no catalog level
    /// (today's two-level behaviour, so most engines inherit this default).
    fn list_catalogs(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn list_schemas(&mut self, catalog: Option<&str>) -> anyhow::Result<Vec<String>>;
    fn list_tables(&mut self, catalog: Option<&str>, schema: &str) -> anyhow::Result<Vec<String>>;
    fn query(&mut self, sql: &str) -> anyhow::Result<DataTable>;
    /// Which engine this connector speaks (drives dialect-specific paging).
    fn engine(&self) -> DbEngine;
    /// Stream every row of `sql` in `batch_rows`-sized pages, uncapped (no
    /// `initial_load_rows` limit), handing each non-empty page to `sink`. The
    /// universal copy lane for engines DuckDB can't ATTACH. The default pages
    /// via [`paged_sql`] + [`query`]; a connector with native cursors may
    /// override.
    fn fetch_batches(
        &mut self,
        sql: &str,
        batch_rows: usize,
        sink: &mut dyn FnMut(DataTable) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let engine = self.engine();
        let mut offset = 0usize;
        loop {
            let page = paged_sql(engine, sql, batch_rows, offset);
            let batch = self.query(&page)?;
            let n = batch.row_count();
            if n > 0 {
                sink(batch)?;
            }
            if n < batch_rows {
                break;
            }
            offset += n;
        }
        Ok(())
    }
    /// Run a statement that returns no rows; the result is rows affected.
    fn execute(&mut self, sql: &str) -> anyhow::Result<u64>;
    /// `catalog` is `Some` only for three-level engines
    /// (Snowflake/Databricks/BigQuery); every other engine must be passed
    /// `None` and will error if given one.
    fn write_table(
        &mut self,
        catalog: Option<&str>,
        schema: &str,
        table: &str,
        mode: DbWriteMode,
        data: &DataTable,
    ) -> anyhow::Result<DbWriteReport>;
    /// A thread-safe handle that best-effort cancels the in-flight statement
    /// from another thread (the connector itself is mutably borrowed while a
    /// query runs, so cancellation cannot go through `&self`). `None` when
    /// the engine cannot cancel.
    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        None
    }
}

/// Open a connection. `secret` is the resolved password/token (None for auth
/// modes that mint their own token; the connector calls [`auth`] as needed).
pub fn connect(conn: &DbConnection, secret: Option<&str>) -> anyhow::Result<Box<dyn DbConnector>> {
    match conn.engine {
        DbEngine::Postgres => Ok(Box::new(postgres::PostgresConnector::connect_with_dialect(
            conn,
            secret,
            postgres::PgDialect::Postgres,
        )?)),
        DbEngine::Redshift => Ok(Box::new(postgres::PostgresConnector::connect_with_dialect(
            conn,
            secret,
            postgres::PgDialect::Redshift,
        )?)),
        DbEngine::MySql => Ok(Box::new(mysql::MySqlConnector::connect(conn, secret)?)),
        DbEngine::Mssql => Ok(Box::new(mssql::MssqlConnector::connect(conn, secret)?)),
        DbEngine::ClickHouse => Ok(Box::new(clickhouse::ClickHouseConnector::connect(
            conn, secret,
        )?)),
        DbEngine::Exasol => Ok(Box::new(exasol::ExasolConnector::connect(conn, secret)?)),
        DbEngine::Snowflake => Ok(Box::new(snowflake::SnowflakeConnector::connect(
            conn, secret,
        )?)),
        DbEngine::Databricks => Ok(Box::new(databricks::DatabricksConnector::connect(
            conn, secret,
        )?)),
        DbEngine::BigQuery => Ok(Box::new(bigquery::BigQueryConnector::connect(
            conn, secret,
        )?)),
    }
}

/// The statement that stops the work running in `session_id`, or `None` for
/// an engine that cancels some other way.
///
/// The id is validated as digits-only before it reaches a concatenated
/// statement: it arrives from the server as an integer, so anything else is
/// a bug or an injection attempt, never a legitimate id.
pub(crate) fn kill_sql(engine: DbEngine, session_id: &str) -> Option<String> {
    if session_id.is_empty() || !session_id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match engine {
        DbEngine::MySql => Some(format!("KILL QUERY {session_id}")),
        // MSSQL KILL ends the whole session, not just the statement, and
        // needs ALTER ANY CONNECTION. The caller drops the connector after.
        DbEngine::Mssql => Some(format!("KILL {session_id}")),
        DbEngine::Exasol => Some(format!("KILL STATEMENT IN SESSION {session_id}")),
        _ => None,
    }
}

/// Open a throwaway connection and issue the engine's kill statement.
/// Best effort: the statement may already have finished, the user may lack
/// the permission, and neither must surface as a query error.
pub(crate) fn kill_via_new_connection(
    conn: DbConnection,
    secret: Option<String>,
    session_id: String,
) {
    let Some(sql) = kill_sql(conn.engine, &session_id) else {
        return;
    };
    if let Ok(mut killer) = connect(&conn, secret.as_deref()) {
        let _ = killer.execute(&sql);
    }
}

/// The SQL dialect a live engine speaks. The single mapping point for every
/// DDL statement Octa sends to a server, so CREATE TABLE and ALTER TABLE ADD
/// can never disagree about a type name.
pub(crate) fn live_dialect_for(
    engine: DbEngine,
) -> crate::data::schema_export::sql::LiveSqlDialect {
    use crate::data::schema_export::sql::LiveSqlDialect;
    match engine {
        DbEngine::MySql => LiveSqlDialect::Mysql,
        DbEngine::Mssql => LiveSqlDialect::Mssql,
        DbEngine::Snowflake => LiveSqlDialect::Snowflake,
        DbEngine::Databricks => LiveSqlDialect::Databricks,
        DbEngine::ClickHouse => LiveSqlDialect::ClickHouse,
        DbEngine::Exasol => LiveSqlDialect::Exasol,
        DbEngine::BigQuery => LiveSqlDialect::BigQuery,
        // Redshift genuinely speaks the Postgres dialect, so this arm is
        // correct rather than provisional.
        DbEngine::Postgres | DbEngine::Redshift => LiveSqlDialect::Postgres,
    }
}

/// CREATE TABLE DDL for a live write target, reusing the schema-export
/// type mappings (one source of truth per dialect).
pub fn create_table_sql(
    engine: DbEngine,
    schema: &str,
    table: &str,
    columns: &[crate::data::ColumnInfo],
) -> String {
    use crate::data::schema_export::sql::create_table_qualified;
    create_table_qualified(columns, live_dialect_for(engine), schema, table)
}

/// `[<catalog>.]<schema>.<table>` in the engine's own quoting. `catalog` is
/// set only for three-level engines (Snowflake/Databricks/BigQuery).
fn qualified_name(engine: DbEngine, catalog: Option<&str>, schema: &str, table: &str) -> String {
    let mut name = String::new();
    if let Some(cat) = catalog {
        name.push_str(&engine.quote_ident(cat));
        name.push('.');
    }
    if !schema.is_empty() {
        name.push_str(&engine.quote_ident(schema));
        name.push('.');
    }
    name.push_str(&engine.quote_ident(table));
    name
}

/// Reject a catalog passed to an engine that has no catalog level, naming the
/// engines that do. A silent ignore would write to the wrong table.
pub fn reject_catalog(engine: DbEngine, catalog: Option<&str>) -> anyhow::Result<()> {
    if let Some(cat) = catalog
        && !engine.has_catalogs()
    {
        anyhow::bail!(
            "{engine:?} has no catalog level, so 'catalog' ({cat}) cannot be used. \
             Catalogs are supported on Snowflake, Databricks and BigQuery."
        );
    }
    Ok(())
}

/// SELECT the first `n` rows of `[catalog.]schema.table` in the engine's own
/// dialect (SQL Server has no LIMIT clause; it spells the cap TOP).
pub fn select_sample_sql(
    engine: DbEngine,
    catalog: Option<&str>,
    schema: &str,
    table: &str,
    n: usize,
) -> String {
    let name = qualified_name(engine, catalog, schema, table);
    match engine {
        DbEngine::Mssql => format!("SELECT TOP {n} * FROM {name}"),
        _ => format!("SELECT * FROM {name} LIMIT {n}"),
    }
}

/// SQL returning the primary-key column names of `schema.table` in ordinal
/// order. One `information_schema` query serves all three engines (Postgres,
/// MySQL/MariaDB, SQL Server all expose these views); values are embedded as
/// string literals with `''` doubling. The `engine` parameter is kept in the
/// signature in case a dialect split ever becomes necessary.
pub fn primary_key_sql(
    engine: DbEngine,
    catalog: Option<&str>,
    schema: &str,
    table: &str,
) -> String {
    // Two-level engines only reach this; catalog engines skip the PK lookup
    // (they expose no discoverable PK). Kept in the signature for uniformity.
    let _ = (engine, catalog);
    let lit = |s: &str| format!("'{}'", s.replace('\'', "''"));
    format!(
        "SELECT kcu.column_name \
         FROM information_schema.table_constraints tc \
         JOIN information_schema.key_column_usage kcu \
           ON kcu.constraint_name = tc.constraint_name \
          AND kcu.table_schema = tc.table_schema \
          AND kcu.table_name = tc.table_name \
         WHERE tc.constraint_type = 'PRIMARY KEY' \
           AND tc.table_schema = {} AND tc.table_name = {} \
         ORDER BY kcu.ordinal_position",
        lit(schema),
        lit(table)
    )
}

/// SQL returning a table's metadata for the sidebar "Show metadata..." action,
/// shown as-is in a read-only tab. The result shape differs per engine: the
/// engines with a native `DESCRIBE` return columns (and, for Databricks
/// `EXTENDED`, a "Detailed Table Information" section with location / format /
/// owner / properties); the rest read `information_schema.columns`.
///
/// ponytail: table-level detail beyond columns rides along only where the
/// engine's own command carries it (Databricks today). Add per-engine
/// table-stats queries if a user asks for size/rows on the others.
pub fn table_metadata_sql(
    engine: DbEngine,
    catalog: Option<&str>,
    schema: &str,
    table: &str,
) -> String {
    let name = qualified_name(engine, catalog, schema, table);
    let lit = |s: &str| format!("'{}'", s.replace('\'', "''"));
    match engine {
        // Native DESCRIBE dialects. Databricks EXTENDED adds the detailed
        // table-information block on top of the column list.
        DbEngine::Databricks => format!("DESCRIBE TABLE EXTENDED {name}"),
        DbEngine::Snowflake => format!("DESCRIBE TABLE {name}"),
        DbEngine::ClickHouse => format!("DESCRIBE TABLE {name}"),
        DbEngine::Exasol => format!("DESCRIBE {name}"),
        // BigQuery has no DESCRIBE: read the dataset's INFORMATION_SCHEMA.
        DbEngine::BigQuery => {
            let prefix = match catalog {
                Some(project) => {
                    format!(
                        "{}.{}",
                        engine.quote_ident(project),
                        engine.quote_ident(schema)
                    )
                }
                None => engine.quote_ident(schema),
            };
            format!(
                "SELECT * FROM {prefix}.INFORMATION_SCHEMA.COLUMNS \
                 WHERE table_name = {} ORDER BY ordinal_position",
                lit(table)
            )
        }
        // Postgres / Redshift / MySQL / MariaDB / SQL Server: one shared
        // information_schema.columns query (all expose these views).
        DbEngine::Postgres | DbEngine::Redshift | DbEngine::MySql | DbEngine::Mssql => format!(
            "SELECT ordinal_position, column_name, data_type, is_nullable, \
             character_maximum_length, numeric_precision, numeric_scale, column_default \
             FROM information_schema.columns \
             WHERE table_schema = {} AND table_name = {} \
             ORDER BY ordinal_position",
            lit(schema),
            lit(table)
        ),
    }
}

/// Render one cell as a SQL literal in the engine's dialect. Strings quote
/// with `''` doubling; booleans are `TRUE`/`FALSE` except SQL Server's BIT
/// (`1`/`0`); NULL for null and non-finite floats.
pub(crate) fn sql_literal(engine: DbEngine, cell: &crate::data::CellValue) -> String {
    use crate::data::CellValue;
    let quote = |s: &str| format!("'{}'", s.replace('\'', "''"));
    match cell {
        CellValue::Null => "NULL".to_string(),
        CellValue::Int(i) => i.to_string(),
        CellValue::Float(f) => {
            if f.is_finite() {
                f.to_string()
            } else {
                "NULL".to_string()
            }
        }
        CellValue::Bool(b) => match engine {
            DbEngine::Mssql => if *b { "1" } else { "0" }.to_string(),
            _ => if *b { "TRUE" } else { "FALSE" }.to_string(),
        },
        CellValue::String(s)
        | CellValue::Date(s)
        | CellValue::DateTime(s)
        | CellValue::Nested(s) => quote(s),
        CellValue::Binary(_) => quote(&cell.to_string()),
    }
}

/// Rows per multi-row INSERT statement in [`write_table_generic`].
const INSERT_BATCH_ROWS: usize = 500;

/// Shared write-back implementation: DDL (Create/Replace) + batched
/// multi-row INSERTs, all through the connector's own `execute`, wrapped in
/// one transaction. Engine differences are confined to identifier quoting,
/// type mapping and literal rendering.
/// ponytail: literal-SQL inserts, not prepared statements/COPY; upgrade the
/// hot path per engine if bulk-write throughput ever matters.
pub(crate) fn write_table_generic(
    connector: &mut dyn DbConnector,
    engine: DbEngine,
    catalog: Option<&str>,
    schema: &str,
    table: &str,
    mode: DbWriteMode,
    data: &DataTable,
) -> anyhow::Result<DbWriteReport> {
    use anyhow::Context;
    if data.columns.is_empty() {
        anyhow::bail!("refusing to write a table with no columns");
    }
    let target = qualified_name(engine, catalog, schema, table);
    let begin = match engine {
        DbEngine::Mssql => "BEGIN TRANSACTION",
        _ => "BEGIN",
    };
    connector.execute(begin).context("starting transaction")?;
    let result = (|| -> anyhow::Result<bool> {
        let mut created = false;
        if matches!(mode, DbWriteMode::Replace) {
            connector
                .execute(&format!("DROP TABLE IF EXISTS {target}"))
                .context("dropping the existing table")?;
        }
        if matches!(mode, DbWriteMode::Create | DbWriteMode::Replace) {
            connector
                .execute(&create_table_sql(engine, schema, table, &data.columns))
                .context("creating the target table")?;
            created = true;
        }
        let col_list = data
            .columns
            .iter()
            .map(|c| engine.quote_ident(&c.name))
            .collect::<Vec<_>>()
            .join(", ");
        for chunk in data.rows.chunks(INSERT_BATCH_ROWS) {
            let values: Vec<String> = chunk
                .iter()
                .map(|row| {
                    let cells: Vec<String> = row.iter().map(|c| sql_literal(engine, c)).collect();
                    format!("({})", cells.join(", "))
                })
                .collect();
            connector
                .execute(&format!(
                    "INSERT INTO {target} ({col_list}) VALUES {}",
                    values.join(", ")
                ))
                .context("inserting rows")?;
        }
        Ok(created)
    })();
    match result {
        Ok(created) => {
            connector.execute("COMMIT").context("committing")?;
            Ok(DbWriteReport {
                rows_written: data.rows.len(),
                created,
            })
        }
        Err(e) => {
            let _ = connector.execute("ROLLBACK");
            Err(e)
        }
    }
}

/// The single write gate every surface routes through. Refuses when the
/// connection is read-only and the action writes: either an explicit write
/// action (`sql: None`) or a statement classified as a mutation by the same
/// leading-keyword sniff the SQL workspace uses.
pub fn ensure_write_allowed(conn: &DbConnection, sql: Option<&str>) -> anyhow::Result<()> {
    if conn.allow_writes {
        return Ok(());
    }
    let is_write = match sql {
        None => true,
        Some(q) => crate::sql::is_mutation(q),
    };
    if is_write {
        anyhow::bail!(
            "connection '{}' is read-only; turn on \"Allow writes\" for it in Settings -> Databases to permit this",
            conn.name
        );
    }
    Ok(())
}

/// Shared multi-thread tokio runtime for blocking on the async DB clients.
/// Built on first use, mirroring `crate::cloud::runtime`.
pub(crate) fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("building the db tokio runtime")
    })
}

/// A rustls client config trusting the platform's native roots (ring
/// provider). Shared by the Postgres and MSSQL connectors. Built once:
/// loading the native cert store is expensive and this runs on every
/// connect and every Postgres cancel (clone is cheap, Arc-backed).
pub(crate) fn rustls_client_config() -> rustls::ClientConfig {
    static CFG: OnceLock<rustls::ClientConfig> = OnceLock::new();
    CFG.get_or_init(|| {
        let mut roots = rustls::RootCertStore::empty();
        for cert in rustls_native_certs::load_native_certs().certs {
            let _ = roots.add(cert);
        }
        rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("ring provider supports the default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth()
    })
    .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(allow_writes: bool) -> DbConnection {
        DbConnection {
            id: "db-1".into(),
            name: "prod".into(),
            engine: DbEngine::Postgres,
            host: "localhost".into(),
            port: 5432,
            database: "app".into(),
            username: "octa".into(),
            auth: DbAuth::Password,
            allow_writes,
            oauth_client_id: None,
            oauth_tenant: None,
        }
    }

    #[test]
    fn cancel_flag_is_shared_across_clones() {
        let flag = CancelFlag::new();
        let clone = flag.clone();
        assert!(!flag.is_cancelled());
        clone.cancel();
        assert!(flag.is_cancelled(), "a clone must share the same flag");
        flag.reset();
        assert!(!clone.is_cancelled());
    }

    #[test]
    fn kill_sql_per_engine() {
        assert_eq!(
            kill_sql(DbEngine::MySql, "42").as_deref(),
            Some("KILL QUERY 42")
        );
        assert_eq!(kill_sql(DbEngine::Mssql, "57").as_deref(), Some("KILL 57"));
        assert_eq!(
            kill_sql(DbEngine::Exasol, "1690000000000000000").as_deref(),
            Some("KILL STATEMENT IN SESSION 1690000000000000000")
        );
        // Engines that cancel some other way must not get a kill statement.
        assert!(kill_sql(DbEngine::Postgres, "1").is_none());
        assert!(kill_sql(DbEngine::BigQuery, "1").is_none());
    }

    #[test]
    fn kill_sql_rejects_a_non_numeric_session_id() {
        // Session ids come from the server as integers; anything else means
        // something is wrong, and it must never reach a concatenated
        // statement.
        assert!(kill_sql(DbEngine::MySql, "42; DROP TABLE t").is_none());
    }

    #[test]
    fn qualified_name_prefixes_the_catalog() {
        let sql = create_table_sql(
            DbEngine::Snowflake,
            "analytics",
            "orders",
            &[crate::data::ColumnInfo {
                name: "id".to_string(),
                data_type: "Int64".to_string(),
            }],
        );
        assert!(sql.contains("analytics.orders"), "got: {sql}");
        let qualified = qualified_name(
            DbEngine::Snowflake,
            Some("sales_prod"),
            "analytics",
            "orders",
        );
        assert_eq!(qualified, "\"sales_prod\".\"analytics\".\"orders\"");
    }

    #[test]
    fn qualified_name_skips_empty_schema() {
        assert_eq!(
            qualified_name(DbEngine::Postgres, None, "", "orders"),
            "\"orders\""
        );
        assert_eq!(
            qualified_name(DbEngine::Snowflake, Some("db"), "", "orders"),
            "\"db\".\"orders\""
        );
    }

    #[test]
    fn select_sample_sql_uses_each_dialect() {
        assert_eq!(
            select_sample_sql(DbEngine::Postgres, None, "public", "my table", 10),
            "SELECT * FROM \"public\".\"my table\" LIMIT 10"
        );
        assert_eq!(
            select_sample_sql(DbEngine::MySql, None, "app", "t", 5),
            "SELECT * FROM `app`.`t` LIMIT 5"
        );
        assert_eq!(
            select_sample_sql(DbEngine::Mssql, None, "dbo", "t", 5),
            "SELECT TOP 5 * FROM [dbo].[t]"
        );
    }

    #[test]
    fn select_sample_sql_qualifies_catalog() {
        // Databricks three-part, backticks.
        assert_eq!(
            select_sample_sql(DbEngine::Databricks, Some("main"), "sales", "orders", 5),
            "SELECT * FROM `main`.`sales`.`orders` LIMIT 5"
        );
        // BigQuery three-part, backticks.
        assert_eq!(
            select_sample_sql(DbEngine::BigQuery, Some("proj"), "ds", "tbl", 7),
            "SELECT * FROM `proj`.`ds`.`tbl` LIMIT 7"
        );
        // Snowflake three-part, double quotes.
        assert_eq!(
            select_sample_sql(DbEngine::Snowflake, Some("DB"), "PUBLIC", "T", 3),
            "SELECT * FROM \"DB\".\"PUBLIC\".\"T\" LIMIT 3"
        );
    }

    #[test]
    fn primary_key_sql_quotes_literals() {
        let sql = primary_key_sql(DbEngine::Postgres, None, "pub'lic", "orders");
        assert!(sql.contains("tc.table_schema = 'pub''lic'"), "{sql}");
        assert!(sql.contains("tc.table_name = 'orders'"), "{sql}");
        assert!(sql.contains("constraint_type = 'PRIMARY KEY'"), "{sql}");
        assert!(sql.ends_with("ORDER BY kcu.ordinal_position"), "{sql}");
    }

    #[test]
    fn metadata_sql_dialects() {
        // Databricks: three-part backtick name, EXTENDED for the detail block.
        assert_eq!(
            table_metadata_sql(DbEngine::Databricks, Some("main"), "sales", "orders"),
            "DESCRIBE TABLE EXTENDED `main`.`sales`.`orders`"
        );
        // Snowflake / ClickHouse / Exasol keep their DESCRIBE forms.
        assert_eq!(
            table_metadata_sql(DbEngine::Snowflake, Some("DB"), "PUBLIC", "T"),
            "DESCRIBE TABLE \"DB\".\"PUBLIC\".\"T\""
        );
        assert!(table_metadata_sql(DbEngine::Exasol, None, "s", "t").starts_with("DESCRIBE "));
        // BigQuery: dataset INFORMATION_SCHEMA.COLUMNS, table filtered by name.
        let bq = table_metadata_sql(DbEngine::BigQuery, Some("proj"), "ds", "tbl");
        assert!(
            bq.contains("`proj`.`ds`.INFORMATION_SCHEMA.COLUMNS"),
            "{bq}"
        );
        assert!(bq.contains("table_name = 'tbl'"), "{bq}");
        // Generic engines: information_schema.columns, literals quoted.
        let pg = table_metadata_sql(DbEngine::Postgres, None, "pub'lic", "orders");
        assert!(pg.contains("FROM information_schema.columns"), "{pg}");
        assert!(pg.contains("table_schema = 'pub''lic'"), "{pg}");
        assert!(pg.contains("table_name = 'orders'"), "{pg}");
    }

    #[test]
    fn connection_roundtrips_through_serde() {
        for auth in [
            DbAuth::AwsIam {
                region: Some("eu-central-1".into()),
                sso_start_url: None,
                sso_region: None,
                sso_account_id: None,
                sso_role: None,
            },
            DbAuth::AwsIam {
                region: None,
                sso_start_url: Some("https://acme.awsapps.com/start".into()),
                sso_region: Some("eu-central-1".into()),
                sso_account_id: Some("123456789012".into()),
                sso_role: Some("DBReader".into()),
            },
            DbAuth::AzureAd,
            DbAuth::GcpIam,
        ] {
            let mut c = conn(true);
            c.auth = auth;
            let toml = toml::to_string(&c).unwrap();
            let back: DbConnection = toml::from_str(&toml).unwrap();
            assert_eq!(c, back);
        }
    }

    #[test]
    fn old_connection_without_new_fields_still_loads() {
        // A settings.toml written before `auth`/`allow_writes` existed.
        let raw = r#"
id = "db-2"
name = "legacy"
engine = "MySql"
host = "h"
port = 3306
database = "d"
username = "u"
"#;
        let c: DbConnection = toml::from_str(raw).unwrap();
        assert_eq!(c.auth, DbAuth::Password);
        assert!(!c.allow_writes);
    }

    #[test]
    fn write_gate_refuses_mutations_on_readonly() {
        let c = conn(false);
        let err = ensure_write_allowed(&c, Some("UPDATE t SET x = 1"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("Allow writes"), "{err}");
        assert!(ensure_write_allowed(&c, Some("SELECT 1")).is_ok());
        assert!(ensure_write_allowed(&c, None).is_err());
    }

    #[test]
    fn write_gate_open_when_allowed() {
        let c = conn(true);
        assert!(ensure_write_allowed(&c, Some("DROP TABLE t")).is_ok());
        assert!(ensure_write_allowed(&c, None).is_ok());
    }

    #[test]
    fn default_ports_match_convention() {
        assert_eq!(DbEngine::Postgres.default_port(), 5432);
        assert_eq!(DbEngine::MySql.default_port(), 3306);
        assert_eq!(DbEngine::Mssql.default_port(), 1433);
    }

    #[test]
    fn ident_quoting_per_engine() {
        assert_eq!(DbEngine::Postgres.quote_ident("a\"b"), "\"a\"\"b\"");
        assert_eq!(DbEngine::MySql.quote_ident("a`b"), "`a``b`");
        assert_eq!(DbEngine::Mssql.quote_ident("a]b"), "[a]]b]");
    }

    #[test]
    fn fresh_ids_are_distinct() {
        assert_ne!(DbConnection::fresh_id(), DbConnection::fresh_id());
    }
}
