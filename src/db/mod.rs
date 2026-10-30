//! Live database connections: Postgres, MySQL/MariaDB, SQL Server, Oracle,
//! Redshift, ClickHouse, Exasol, Trino, Athena, Snowflake, Databricks,
//! BigQuery, plus DuckDB and SQLite files (see [`file_db`]).
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

use crate::data::{CellValue, ColumnInfo, DataTable};

pub mod athena;
pub mod auth;
pub mod bigquery;
pub mod clickhouse;
pub mod copy;
pub mod databricks;
pub(crate) mod duckdb_file;
mod error;
pub use error::ServerError;
pub mod exasol;
pub mod export;
pub mod fetch_table;
pub mod file_db;
pub mod ident_fix;
pub mod mssql;
pub mod mysql;
pub mod oracle;
pub mod postgres;
pub mod pushdown;
pub mod relationships;
pub(crate) mod rest;
pub mod sigv4;
pub mod snowflake;
pub(crate) mod sql_lex;
pub(crate) mod sqlite_file;
pub mod ssh_tunnel;
pub mod tls;
pub mod trino;
pub mod write_back;

pub use ident_fix::{quote_rejected_identifier, with_identifier_fix};

mod engine;
mod sql_builders;

pub use engine::DbEngine;
pub use sql_builders::{
    RowKeyCandidate, choose_row_key, column_names_from_metadata, count_sql, create_table_sql,
    paged_sql, reject_catalog, row_key_sql, select_all_sql, select_sample_sql, table_metadata_sql,
    table_search_sql,
};
pub(crate) use sql_builders::{
    drop_if_exists_sql, insert_rows_sql, kill_sql, live_dialect_for, qualified_name, sql_literal,
};

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

/// Default [`DbConnection::query_timeout_secs`]. Sixty seconds is what the
/// HTTP engines already allowed before the field existed (a 30s server-side
/// wait plus 60 polls half a second apart), so an existing `settings.toml`
/// that predates it behaves exactly as it did.
pub const DEFAULT_QUERY_TIMEOUT_SECS: u32 = 60;

fn default_query_timeout_secs() -> u32 {
    DEFAULT_QUERY_TIMEOUT_SECS
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
    /// Athena workgroup (`primary` when unset). Athena refuses a query that
    /// names none, so this is a field rather than a constant.
    #[serde(default)]
    pub athena_workgroup: Option<String>,
    /// S3 URI Athena writes query results to (`s3://bucket/prefix/`). Not
    /// needed when the workgroup enforces its own output location, which is
    /// why it is optional rather than required.
    #[serde(default)]
    pub athena_output_location: Option<String>,
    /// SQL Server only: read DATETIMEOFFSET as a timestamp in UTC, so charts
    /// and date features work and a save stores the same moment at +00:00.
    /// Off (the default) reads it as text with its own offset, which a save
    /// keeps exactly.
    #[serde(default)]
    pub datetimeoffset_as_timestamp: bool,
    /// How many seconds to keep waiting for a statement that is making no
    /// progress, before giving up. Default 60.
    ///
    /// Only the HTTP engines read it ([`DbEngine::polls_for_results`]): they
    /// submit a statement and then ask the server, over and over, whether it
    /// has finished, so the give-up point is Octa's to choose. The wire
    /// protocols hand that decision to their driver and their server.
    #[serde(default = "default_query_timeout_secs")]
    pub query_timeout_secs: u32,
    /// Reach the database through this jump host. None = connect directly,
    /// which is the default and costs nothing.
    #[serde(default)]
    pub ssh: Option<ssh_tunnel::SshTunnel>,
    /// SSL mode, CA file and Connect via (PostgreSQL, Redshift, MySQL).
    /// Absent in an older `settings.toml`, which then gets `prefer`.
    #[serde(default)]
    pub tls: tls::DbTls,
    /// Loopback port of the live tunnel, filled in by [`connect`] for the
    /// duration of one connect. Never persisted: it belongs to a running
    /// tunnel, so a port written to `settings.toml` would be stale by the next
    /// launch.
    #[serde(skip)]
    pub tunnel_port: Option<u16>,
}

impl DbConnection {
    /// Where to actually open the socket: the tunnel's loopback port when one
    /// is up, else Connect via (`tls.connect_via`) on the connection's port,
    /// else the connection's own host and port.
    ///
    /// `host` and `port` deliberately keep naming the *database* even while
    /// tunnelled, so TLS verification, Entra token audiences and error text are
    /// all unaffected by how the bytes get there. Only the address dialled
    /// changes.
    pub fn dial_target(&self) -> (String, u16) {
        match self.tunnel_port {
            Some(p) => ("127.0.0.1".to_string(), p),
            None => match self.tls.connect_via.as_deref().map(str::trim) {
                Some(via) if !via.is_empty() => (via.to_string(), self.port),
                _ => (self.host.clone(), self.port),
            },
        }
    }

    /// Whether this connect is going through a jump host.
    pub fn is_tunnelled(&self) -> bool {
        self.tunnel_port.is_some()
    }

    /// A saved DuckDB or SQLite file: the path goes in `database`, which the
    /// server engines use for a database name; nothing else is set.
    pub fn file(id: String, name: &str, engine: DbEngine, path: &str, allow_writes: bool) -> Self {
        DbConnection {
            id,
            name: name.to_string(),
            engine,
            host: String::new(),
            port: 0,
            database: path.to_string(),
            username: String::new(),
            auth: DbAuth::Password,
            allow_writes,
            oauth_client_id: None,
            oauth_tenant: None,
            athena_workgroup: None,
            athena_output_location: None,
            datetimeoffset_as_timestamp: false,
            query_timeout_secs: DEFAULT_QUERY_TIMEOUT_SECS,
            ssh: None,
            tls: tls::DbTls::default(),
            tunnel_port: None,
        }
    }

    /// Where the connection points, for a list or a tooltip: the file path
    /// for DuckDB and SQLite, `host:port/database` for a server.
    pub fn target_text(&self) -> String {
        if self.engine.is_file() {
            self.database.clone()
        } else {
            format!("{}:{}/{}", self.host, self.port, self.database)
        }
    }

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
    /// Refuse, before anything is sent, rows the server would store wrongly
    /// without saying so (MySQL clips a TIME outside its range). The default
    /// accepts everything. Not labelled by `LabelledConnector`: a refusal
    /// already names the connection.
    fn check_rows_writable(
        &self,
        _columns: &[ColumnInfo],
        _rows: &[&[CellValue]],
    ) -> anyhow::Result<()> {
        Ok(())
    }
    /// A thread-safe handle that best-effort cancels the in-flight statement
    /// from another thread (the connector itself is mutably borrowed while a
    /// query runs, so cancellation cannot go through `&self`). `None` when
    /// the engine cannot cancel.
    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        None
    }

    /// Bytes `sql` would read, without running it. Only an engine with a dry
    /// run answers (BigQuery); Explain shows this in place of a plan there.
    fn dry_run_bytes(&mut self, _sql: &str) -> anyhow::Result<u64> {
        anyhow::bail!("{} has no dry run", self.engine().label())
    }
}

/// Open a connection. `secret` is the resolved password/token (None for auth
/// modes that mint their own token; the connector calls [`auth`] as needed).
/// Every error, the connect's own and the connector's later, starts with a
/// sentence saying whether the server sent it ([`error`]).
pub fn connect(
    conn: &DbConnection,
    secret: Option<&str>,
    ssh_secret: Option<&str>,
) -> anyhow::Result<Box<dyn DbConnector>> {
    open(conn, secret, ssh_secret)
        .map(|c| error::labelled(c, &conn.name))
        .map_err(|e| error::label_connect(conn.engine, &conn.name, e))
}

fn open(
    conn: &DbConnection,
    secret: Option<&str>,
    ssh_secret: Option<&str>,
) -> anyhow::Result<Box<dyn DbConnector>> {
    // Open the jump-host forward first, if this connection has one, and hand
    // the engines a copy that knows the loopback port. They dial
    // `conn.dial_target()` and are otherwise unaware a tunnel exists; `host`
    // and `port` still name the database, which is what keeps TLS verification
    // and token audiences correct.
    // A file is on this computer: no jump host, even if one was saved.
    let tunnelled = if conn.engine.is_file() {
        conn.clone()
    } else {
        ssh_tunnel::with_tunnel(conn, ssh_secret)?
    };
    let conn = &tunnelled;
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
        DbEngine::Oracle => Ok(Box::new(oracle::OracleConnector::connect(conn, secret)?)),
        DbEngine::ClickHouse => Ok(Box::new(clickhouse::ClickHouseConnector::connect(
            conn, secret,
        )?)),
        DbEngine::Exasol => Ok(Box::new(exasol::ExasolConnector::connect(conn, secret)?)),
        DbEngine::Trino => Ok(Box::new(trino::TrinoConnector::connect(conn, secret)?)),
        DbEngine::Athena => Ok(Box::new(athena::AthenaConnector::connect(conn, secret)?)),
        DbEngine::Snowflake => Ok(Box::new(snowflake::SnowflakeConnector::connect(
            conn, secret,
        )?)),
        DbEngine::Databricks => Ok(Box::new(databricks::DatabricksConnector::connect(
            conn, secret,
        )?)),
        DbEngine::BigQuery => Ok(Box::new(bigquery::BigQueryConnector::connect(
            conn, secret,
        )?)),
        DbEngine::DuckDb => Ok(Box::new(
            file_db::FileConnector::<duckdb_file::DuckDbFile>::connect(conn)?,
        )),
        DbEngine::Sqlite => Ok(Box::new(
            file_db::FileConnector::<sqlite_file::SqliteFile>::connect(conn)?,
        )),
    }
}

/// Open a throwaway connection and issue the engine's kill statement.
/// Best effort: the statement may already have finished, the user may lack
/// the permission, and neither must surface as a query error.
pub(crate) fn kill_via_new_connection(
    conn: DbConnection,
    secret: Option<String>,
    ssh_secret: Option<String>,
    session_id: String,
) {
    let Some(sql) = kill_sql(conn.engine, &session_id) else {
        return;
    };
    if let Ok(mut killer) = connect(&conn, secret.as_deref(), ssh_secret.as_deref()) {
        let _ = killer.execute(&sql);
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
    let all_rows: Vec<&[CellValue]> = data.rows.iter().map(Vec::as_slice).collect();
    connector.check_rows_writable(&data.columns, &all_rows)?;
    let mut table = table.to_string();
    let mut names: Vec<String> = data.columns.iter().map(|c| c.name.clone()).collect();
    if matches!(mode, DbWriteMode::Append) {
        // Every identifier below is quoted, so an existing table has to be
        // named the way the server stores it: Oracle folds an unquoted
        // `orders (id)` to `ORDERS (ID)`, where `"id"` is another column.
        // Best effort and before BEGIN: a catalog the user cannot read keeps
        // the data's names, and the INSERT then says what is wrong.
        let listing =
            relationships::scan_columns(connector, catalog, &[schema.to_string()], Some(&table));
        if let Some((real, columns)) = listing
            .ok()
            .and_then(|all| relationships::table_columns(&all, &table))
        {
            for n in &mut names {
                if let Some(s) = relationships::server_name(&columns, n) {
                    *n = s.to_string();
                }
            }
            table = real;
        }
    }
    let table = table.as_str();
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
                .execute(&drop_if_exists_sql(engine, &target))
                .context("dropping the existing table")?;
        }
        if matches!(mode, DbWriteMode::Create | DbWriteMode::Replace) {
            connector
                .execute(&create_table_sql(engine, schema, table, &data.columns))
                .context("creating the target table")?;
            created = true;
        }
        let col_list = names
            .iter()
            .map(|n| engine.quote_ident(n))
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
                .execute(&insert_rows_sql(engine, &target, &col_list, &values))
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

/// A timestamp as `YYYY-MM-DD HH:MM:SS` plus the fractional seconds it has,
/// trailing zeros dropped (`.5`, `.789012`, `.1234567`); whole seconds get
/// none. The connectors used to print whole seconds only, and since a
/// database tab writes back whole rows, a save cut every fraction off.
pub(crate) fn datetime_text(d: &chrono::NaiveDateTime) -> String {
    use chrono::Timelike;
    format!(
        "{}{}",
        d.format("%Y-%m-%d %H:%M:%S"),
        fraction_text(d.nanosecond())
    )
}

/// `.` plus `nanos` as decimal digits without trailing zeros, or nothing for
/// a whole second.
pub(crate) fn fraction_text(nanos: u32) -> String {
    if nanos == 0 {
        return String::new();
    }
    format!(".{}", format!("{nanos:09}").trim_end_matches('0'))
}

/// Statements a read-only connection runs, by their first word. A list of
/// what may run rather than of what may not, so a verb Octa does not know (a
/// procedure call, a code block, a grant, a vacuum) is refused, not let
/// through: a procedure can commit and then write in a transaction of its
/// own, past even the server's read-only session.
const READ_WORDS: &[&str] = &[
    "SELECT", "WITH", "SHOW", "DESCRIBE", "DESC", "EXPLAIN", "VALUES", "TABLE", "USE", "LIST",
    "HELP", "EXISTS",
];

/// SQL Server batches also open with these. A read-only connection runs every
/// batch in a transaction it then rolls back (see `mssql.rs`), so what they
/// lead into cannot stay.
const MSSQL_READ_WORDS: &[&str] = &["DECLARE", "IF", "BEGIN", "WHILE", "PRINT"];

/// Why `sql` could change data on a read-only `engine` connection although
/// its first word names no write, or `None`. The reason is a sentence of its
/// own, read after "Octa did not send this ...". The text checks are blunt on
/// purpose and count words in strings and comments too: each engine lexes
/// those its own way, and only the blunt reading cannot be fooled. The
/// exceptions read the text the way the engine lexes it ([`sql_lex`]): the
/// SQL Server and Redshift words and a Databricks `WITH`
/// ([`databricks::with_write_risk`]), because there the blunt reading refused
/// everyday queries that only mention a write word in text or a comment. Each
/// falls back to the blunt reading where the engine's own could differ.
pub fn write_risk(engine: DbEngine, sql: &str) -> Option<String> {
    let first = crate::sql::first_keyword(sql);
    let mssql = engine == DbEngine::Mssql;
    let readable = READ_WORDS.contains(&first.as_str())
        || (mssql && MSSQL_READ_WORDS.contains(&first.as_str()));
    let start = crate::sql::statement_start(sql);
    if !readable && !start.is_empty() {
        // The word as typed, so a typo is plain to see ("SELCT").
        let word: String = start
            .split_whitespace()
            .next()
            .unwrap_or("")
            .chars()
            .take(30)
            .collect();
        return Some(if mssql {
            format!(
                "The batch starts with {word}, and on SQL Server a read-only connection only \
                 runs batches that start with SELECT, WITH, DECLARE, IF, BEGIN, WHILE or PRINT, \
                 since anything else may call a procedure."
            )
        } else {
            format!(
                "The statement starts with {word}, and a read-only connection only runs \
                 statements that start with SELECT, WITH, SHOW, DESCRIBE or EXPLAIN."
            )
        });
    }
    // MySQL and BigQuery run every statement in the text they are sent
    // (mysql_async turns on multi-statements, BigQuery runs scripts). The
    // others refuse a second statement themselves (PostgreSQL prepares
    // first, the REST warehouses take one per call), and SQL Server's batch
    // is rolled back as a whole.
    if matches!(engine, DbEngine::MySql | DbEngine::BigQuery)
        && crate::sql::has_inner_semicolon(sql)
    {
        return Some(format!(
            "On {} a semicolon before the end could start a second statement that writes, so \
             send one statement.",
            engine.label()
        ));
    }
    let words = || {
        sql.split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .map(str::to_ascii_uppercase)
    };
    let has = |list: &[&str]| words().any(|w| list.contains(&w.as_str()));
    let why = match engine {
        // Ending the transaction would let what follows stay. A COMMIT can
        // hide in dynamic SQL, so EXEC counts too.
        // Text and comments are read the engine's way (`sql_lex`), so only a
        // word that will run counts; ambiguous text counts every word.
        DbEngine::Mssql
            if sql_lex::has_code_word(
                sql,
                sql_lex::Dialect::TSql,
                &["COMMIT", "ROLLBACK", "EXEC", "EXECUTE"],
            ) =>
        {
            "On SQL Server COMMIT, ROLLBACK, EXEC and EXECUTE are refused in a batch (in text \
             or a comment they are fine), since they could end the transaction Octa rolls \
             every batch back in."
        }
        DbEngine::Redshift
            if sql_lex::has_code_word(sql, sql_lex::Dialect::Redshift, &["INTO"]) =>
        {
            "On Amazon Redshift SELECT ... INTO creates a table, so INTO is refused outside text \
             and comments, and anywhere in a query holding a backslash or a dollar sign."
        }
        // Spark lets a CTE list lead into a write; what it leads into decides.
        DbEngine::Databricks if first == "WITH" => return databricks::with_write_risk(sql),
        _ if first == "EXPLAIN" && !engine.enforces_read_only() && has(&["ANALYZE", "ANALYSE"]) => {
            "EXPLAIN ANALYZE runs the statement it explains."
        }
        _ => return None,
    };
    Some(why.into())
}

/// The single write gate every surface routes through. Refuses when the
/// connection is read-only and the action writes: either an explicit write
/// action (`sql: None`) or a statement that starts with a write keyword or that
/// [`write_risk`] has a reason for. Call it for every statement, not only for
/// mutations: the read-only connection's own checks live here too. Where the
/// server can hold the connection read-only itself
/// ([`DbEngine::enforces_read_only`]), the connector also asks it to.
pub fn ensure_write_allowed(conn: &DbConnection, sql: Option<&str>) -> anyhow::Result<()> {
    if conn.allow_writes {
        return Ok(());
    }
    // Led by "Octa did not send this", so a refusal never reads like an
    // error the database returned (those name the engine, see `error`).
    let why = match sql {
        None => Some("It would change data.".to_string()),
        Some(q) if crate::sql::is_mutation(q) => Some("The statement changes data.".to_string()),
        Some(q) => write_risk(conn.engine, q),
    };
    if let Some(why) = why {
        anyhow::bail!(
            "Octa did not send this to '{}', because the connection is read-only. {why} Turn on \
             \"Allow writes\" for it in Settings -> Databases to allow it.",
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

#[cfg(test)]
#[path = "mod_batch_tests.rs"]
mod batch_tests;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
