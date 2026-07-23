//! SQL Server connector (tiberius + rustls over tokio). SQL goes to the
//! server verbatim (real T-SQL).
//!
//! TLS: `Required` for Azure AD auth (Azure SQL only speaks TLS); the
//! driver default otherwise so local/dev servers keep working.
//! ponytail: no per-connection TLS toggle yet; add one to `DbConnection` if
//! a password-auth server requires TLS.

use anyhow::{Context, Result};
use tiberius::{AuthMethod, Config};
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::{
    CancelFlag, DbAuth, DbConnection, DbConnector, DbWriteMode, DbWriteReport, auth, runtime,
};

pub struct MssqlConnector {
    client: tiberius::Client<Compat<tokio::net::TcpStream>>,
    conn_label: String,
    /// The server-side session this connector owns, used to kill its work
    /// from a second connection.
    session_id: Option<String>,
    /// A clone of the connection settings plus its secret, so the cancel
    /// closure can open a throwaway connection. Both are already in memory
    /// for the lifetime of the connector.
    reconnect: (DbConnection, Option<String>),
    /// Set when this connector's session has been killed. MSSQL `KILL` ends
    /// the session rather than the statement, so every later call on this
    /// connector must fail and let the cache reconnect.
    killed: CancelFlag,
}

impl MssqlConnector {
    pub fn connect(conn: &DbConnection, stored: Option<&str>) -> Result<Self> {
        let secret = auth::resolve_password(conn, stored)?;
        let mut config = Config::new();
        config.host(&conn.host);
        config.port(conn.port);
        config.database(&conn.database);
        match conn.auth {
            DbAuth::AzureAd => {
                config.authentication(AuthMethod::aad_token(secret));
                config.encryption(tiberius::EncryptionLevel::Required);
            }
            _ => {
                config.authentication(AuthMethod::sql_server(&conn.username, &secret));
                // On-prem SQL Server almost always runs a self-signed
                // certificate; every mainstream client (sqlcmd -C, SSMS's
                // "Trust server certificate") accepts it by default, so
                // strict validation here would just fail to connect. The
                // TLS channel still encrypts; Azure AD connections above
                // keep full validation (Azure serves real certificates).
                config.trust_cert();
            }
        }
        let mut client = runtime()
            .block_on(async {
                let tcp = tokio::net::TcpStream::connect(config.get_addr()).await?;
                tcp.set_nodelay(true)?;
                tiberius::Client::connect(config, tcp.compat_write())
                    .await
                    .map_err(anyhow::Error::from)
            })
            .with_context(|| {
                format!(
                    "connecting to {}@{}:{}/{}",
                    conn.username, conn.host, conn.port, conn.database
                )
            })?;
        // `@@SPID` is a smallint; failing to read it just means no cancel
        // handle, which is the pre-existing behaviour.
        let session_id = runtime()
            .block_on(async {
                let stream = client.simple_query("SELECT @@SPID").await?;
                stream.into_first_result().await
            })
            .ok()
            .and_then(|rows| {
                rows.first()
                    .and_then(|r| r.try_get::<i16, _>(0).ok().flatten())
            })
            .map(|id| id.to_string());
        Ok(Self {
            client,
            conn_label: conn.name.clone(),
            session_id,
            reconnect: (conn.clone(), stored.map(str::to_string)),
            killed: CancelFlag::new(),
        })
    }

    /// Refuse to use a connector whose session a cancel has killed; the
    /// connection cache drops it on the error and reconnects.
    fn ensure_alive(&self) -> Result<()> {
        if self.killed.is_cancelled() {
            anyhow::bail!("this SQL Server session was ended by a cancel; reconnecting");
        }
        Ok(())
    }

    fn string_column_query(&mut self, sql: &str, what: &str) -> Result<Vec<String>> {
        let rows = runtime()
            .block_on(async {
                let stream = self.client.simple_query(sql).await?;
                stream.into_first_result().await
            })
            .with_context(|| format!("{what} on '{}'", self.conn_label))?;
        Ok(rows
            .iter()
            .filter_map(|r| r.get::<&str, _>(0).map(|s| s.to_string()))
            .collect())
    }
}

/// Extract one cell via typed `try_get`, keyed on the mapped Arrow bucket
/// (mirrors the Postgres connector). Degrades to text, then Null.
fn tds_cell(row: &tiberius::Row, i: usize, arrow_type: &str) -> CellValue {
    match arrow_type {
        "Int64" => {
            // Intn covers TINYINT..BIGINT; try widest to narrowest.
            if let Ok(Some(v)) = row.try_get::<i64, _>(i) {
                return CellValue::Int(v);
            }
            if let Ok(Some(v)) = row.try_get::<i32, _>(i) {
                return CellValue::Int(v as i64);
            }
            if let Ok(Some(v)) = row.try_get::<i16, _>(i) {
                return CellValue::Int(v as i64);
            }
            if let Ok(Some(v)) = row.try_get::<u8, _>(i) {
                return CellValue::Int(v as i64);
            }
            CellValue::Null
        }
        "Float64" => {
            if let Ok(Some(v)) = row.try_get::<f64, _>(i) {
                return CellValue::Float(v);
            }
            if let Ok(Some(v)) = row.try_get::<f32, _>(i) {
                return CellValue::Float(v as f64);
            }
            CellValue::Null
        }
        "Boolean" => row
            .try_get::<bool, _>(i)
            .ok()
            .flatten()
            .map(CellValue::Bool)
            .unwrap_or(CellValue::Null),
        "Date32" => row
            .try_get::<chrono::NaiveDate, _>(i)
            .ok()
            .flatten()
            .map(|d| CellValue::Date(d.format("%Y-%m-%d").to_string()))
            .unwrap_or(CellValue::Null),
        t if t.starts_with("Timestamp") => {
            if let Ok(Some(v)) = row.try_get::<chrono::NaiveDateTime, _>(i) {
                return CellValue::DateTime(v.format("%Y-%m-%d %H:%M:%S").to_string());
            }
            if let Ok(Some(v)) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(i) {
                return CellValue::DateTime(v.format("%Y-%m-%d %H:%M:%S").to_string());
            }
            CellValue::Null
        }
        _ => row
            .try_get::<&str, _>(i)
            .ok()
            .flatten()
            .map(|s| CellValue::String(s.to_string()))
            .unwrap_or(CellValue::Null),
    }
}

impl DbConnector for MssqlConnector {
    fn engine(&self) -> super::DbEngine {
        super::DbEngine::Mssql
    }

    fn list_schemas(&mut self, _catalog: Option<&str>) -> Result<Vec<String>> {
        self.string_column_query(
            "SELECT name FROM sys.schemas \
             WHERE name NOT IN ('sys', 'INFORMATION_SCHEMA', 'guest', \
             'db_owner', 'db_accessadmin', 'db_securityadmin', 'db_ddladmin', \
             'db_backupoperator', 'db_datareader', 'db_datawriter', \
             'db_denydatareader', 'db_denydatawriter') \
             ORDER BY name",
            "listing schemas",
        )
    }

    fn list_tables(&mut self, _catalog: Option<&str>, schema: &str) -> Result<Vec<String>> {
        let quoted = schema.replace('\'', "''");
        self.string_column_query(
            &format!(
                "SELECT t.name FROM sys.tables t \
                 JOIN sys.schemas s ON t.schema_id = s.schema_id \
                 WHERE s.name = '{quoted}' ORDER BY t.name"
            ),
            "listing tables",
        )
    }

    fn query(&mut self, sql: &str) -> Result<DataTable> {
        self.ensure_alive()?;
        // Collect the first result set up to the initial-load row cap, then
        // drain the rest without storing it (a TDS stream must be consumed
        // before the connection can be reused), so an unbounded SELECT
        // cannot exhaust memory.
        let cap = crate::formats::initial_load_rows();
        let rows = runtime()
            .block_on(async {
                use futures_util::TryStreamExt;
                let mut stream = self.client.simple_query(sql).await?;
                let mut rows = Vec::new();
                while let Some(item) = stream.try_next().await? {
                    if let tiberius::QueryItem::Row(row) = item
                        && row.result_index() == 0
                        && rows.len() < cap
                    {
                        rows.push(row);
                    }
                }
                Ok::<_, tiberius::error::Error>(rows)
            })
            .with_context(|| format!("querying '{}'", self.conn_label))?;
        let mut table = DataTable::empty();
        let Some(first) = rows.first() else {
            return Ok(table);
        };
        table.columns = first
            .columns()
            .iter()
            .map(|c| ColumnInfo {
                name: c.name().to_string(),
                data_type: tds_type_to_arrow(c.column_type()).to_string(),
            })
            .collect();
        let col_types: Vec<String> = table.columns.iter().map(|c| c.data_type.clone()).collect();
        table.rows = rows
            .iter()
            .map(|row| {
                (0..col_types.len())
                    .map(|i| tds_cell(row, i, &col_types[i]))
                    .collect()
            })
            .collect();
        Ok(table)
    }

    fn execute(&mut self, sql: &str) -> Result<u64> {
        self.ensure_alive()?;
        // tiberius' `execute` goes through sp_executesql, which requires
        // BEGIN/COMMIT to balance *within one call* - so transaction control
        // must run as a plain batch instead.
        let first = sql
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        if matches!(first.as_str(), "BEGIN" | "COMMIT" | "ROLLBACK") {
            runtime()
                .block_on(async {
                    let stream = self.client.simple_query(sql).await?;
                    stream.into_results().await
                })
                .with_context(|| format!("executing on '{}'", self.conn_label))?;
            return Ok(0);
        }
        let result = runtime()
            .block_on(self.client.execute(sql, &[]))
            .with_context(|| format!("executing on '{}'", self.conn_label))?;
        Ok(result.total())
    }

    fn write_table(
        &mut self,
        catalog: Option<&str>,
        schema: &str,
        table: &str,
        mode: DbWriteMode,
        data: &DataTable,
    ) -> Result<DbWriteReport> {
        self.ensure_alive()?;
        super::reject_catalog(self.engine(), catalog)?;
        super::write_table_generic(
            self,
            super::DbEngine::Mssql,
            None,
            schema,
            table,
            mode,
            data,
        )
    }

    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        let session_id = self.session_id.clone()?;
        let (conn, secret) = self.reconnect.clone();
        let killed = self.killed.clone();
        Some(Box::new(move || {
            killed.cancel();
            super::kill_via_new_connection(conn.clone(), secret.clone(), session_id.clone());
        }))
    }
}

/// Map a TDS column type to the Arrow-name strings Octa uses.
fn tds_type_to_arrow(t: tiberius::ColumnType) -> &'static str {
    use tiberius::ColumnType::*;
    match t {
        Int1 | Int2 | Int4 | Int8 | Intn => "Int64",
        Float4 | Float8 | Floatn => "Float64",
        Bit | Bitn => "Boolean",
        Daten => "Date32",
        Datetime | Datetime2 | Datetime4 | Datetimen | DatetimeOffsetn => {
            "Timestamp(Microsecond, None)"
        }
        _ => "Utf8",
    }
}
