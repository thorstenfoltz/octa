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
        // tiberius 0.13 fails any round trip that stalls 30 s; an analytic
        // query sorting a big table easily does, and the user can cancel.
        config.command_timeout(None);
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
                // `config.get_addr()` would be the database's own address;
                // behind a jump host the socket goes to the loopback forward
                // instead, while `config.host` stays the name tiberius
                // validates the certificate against.
                let (dial_host, dial_port) = conn.dial_target();
                let tcp = if conn.is_tunnelled() {
                    tokio::net::TcpStream::connect((dial_host.as_str(), dial_port)).await?
                } else {
                    tokio::net::TcpStream::connect(config.get_addr()).await?
                };
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

/// One cell from the value the driver decoded, by its actual variant.
///
/// The old code picked a Rust type from the declared column type and read
/// everything unlisted as text, which failed for DECIMAL, MONEY,
/// uniqueidentifier, varbinary, time and xml and turned them into NULL. A
/// database tab writes back whole rows, so a save then wrote that NULL over
/// the real value. The match has no catch-all on purpose: a variant a future
/// driver adds fails to compile instead of reading as NULL.
fn tds_cell(data: &tiberius::ColumnData<'static>, offset_as_timestamp: bool) -> CellValue {
    use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
    use tiberius::{ColumnData as D, FromSql};
    let cell = match data {
        D::U8(v) => v.map(|x| CellValue::Int(x.into())),
        D::I16(v) => v.map(|x| CellValue::Int(x.into())),
        D::I32(v) => v.map(|x| CellValue::Int(x.into())),
        D::I64(v) => v.map(CellValue::Int),
        // Through its shortest text, so a REAL 1.1 reads as 1.1, not as the
        // 1.100000023841858 a plain widening gives.
        D::F32(v) => v
            .and_then(|x| x.to_string().parse().ok())
            .map(CellValue::Float),
        // FLOAT, and MONEY / SMALLMONEY, which the driver decodes as f64.
        D::F64(v) => v.map(CellValue::Float),
        D::Bit(v) => v.map(CellValue::Bool),
        D::String(v) => v.as_ref().map(|s| CellValue::String(s.to_string())),
        // SQL Server prints a GUID in capitals.
        D::Guid(v) => v.map(|g| CellValue::String(g.to_string().to_uppercase())),
        D::Binary(v) => v.as_ref().map(|b| CellValue::Binary(b.to_vec())),
        // Exact text: a DECIMAL has no f64 representation.
        D::Numeric(v) => v.map(|n| CellValue::String(numeric_text(n))),
        D::Xml(v) => v.as_ref().map(|x| CellValue::String(x.to_string())),
        D::Date(_) => NaiveDate::from_sql(data)
            .ok()
            .flatten()
            .map(|d| CellValue::Date(d.format("%Y-%m-%d").to_string())),
        D::Time(_) => NaiveTime::from_sql(data).ok().flatten().map(|t| {
            CellValue::String(format!(
                "{}{}",
                t.format("%H:%M:%S"),
                super::fraction_text(t.nanosecond())
            ))
        }),
        D::DateTime2(_) | D::SmallDateTime(_) => NaiveDateTime::from_sql(data)
            .ok()
            .flatten()
            .map(|d| CellValue::DateTime(super::datetime_text(&d))),
        // DATETIME counts in 1/300 s. SQL Server prints it to the millisecond
        // and refuses a DATETIME literal with more digits, so a save of the
        // exact 3.333 ms would fail.
        D::DateTime(_) => NaiveDateTime::from_sql(data).ok().flatten().map(|d| {
            let ms = (d.nanosecond() + 500_000) / 1_000_000;
            let whole = d.with_nanosecond(0).unwrap_or(d);
            let d = whole + chrono::Duration::milliseconds(ms.into());
            CellValue::DateTime(super::datetime_text(&d))
        }),
        // Text with its own offset by default: the column stores the offset,
        // so a save writes it back. As a timestamp (the connection's choice,
        // `DbConnection::datetimeoffset_as_timestamp`) it is the moment in
        // UTC, which SQL Server reads back as +00:00: the same moment.
        D::DateTimeOffset(_) => DateTime::<FixedOffset>::from_sql(data)
            .ok()
            .flatten()
            .map(|d| {
                if offset_as_timestamp {
                    CellValue::DateTime(super::datetime_text(&d.naive_utc()))
                } else {
                    CellValue::String(format!(
                        "{} {}",
                        super::datetime_text(&d.naive_local()),
                        d.format("%:z")
                    ))
                }
            }),
    };
    cell.unwrap_or(CellValue::Null)
}

/// A DECIMAL as SQL Server prints it: `99.50`, `-0.000123`, and `123` with
/// no point when the scale is 0 (the driver's own Display writes `123.0`).
fn numeric_text(n: tiberius::numeric::Numeric) -> String {
    if n.scale() == 0 {
        n.int_part().to_string()
    } else {
        n.to_string()
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
        // SQL Server has no read-only session, and a batch needs no semicolon
        // between statements, so `SELECT 1 DELETE FROM t` is two. Without
        // "Allow writes" the batch runs in a transaction that is always rolled
        // back; `db::write_risk` refuses the words that could end it early.
        let read_only = !self.reconnect.0.allow_writes;
        let client = &mut self.client;
        let rows = runtime()
            .block_on(async {
                use futures_util::TryStreamExt;
                if read_only {
                    client
                        .simple_query("BEGIN TRANSACTION")
                        .await?
                        .into_results()
                        .await?;
                }
                let rows = async {
                    let mut stream = client.simple_query(sql).await?;
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
                }
                .await;
                if read_only {
                    // After a failed batch too: nothing it did may stay.
                    let rolled_back = async {
                        client
                            .simple_query("IF @@TRANCOUNT > 0 ROLLBACK")
                            .await?
                            .into_results()
                            .await
                    }
                    .await;
                    let rows = rows?;
                    rolled_back?;
                    return Ok(rows);
                }
                rows
            })
            .with_context(|| format!("querying '{}'", self.conn_label))?;
        let offset_as_timestamp = self.reconnect.0.datetimeoffset_as_timestamp;
        let mut table = DataTable::empty();
        let Some(first) = rows.first() else {
            return Ok(table);
        };
        table.columns = first
            .columns()
            .iter()
            .map(|c| ColumnInfo {
                name: c.name().to_string(),
                data_type: tds_type_to_arrow(c.column_type(), offset_as_timestamp).to_string(),
            })
            .collect();
        table.rows = rows
            .iter()
            .map(|row| {
                row.cells()
                    .map(|(_, data)| tds_cell(data, offset_as_timestamp))
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
            // No SSH credential needed: `conn` is the copy `db::connect` built,
            // so it already carries the open tunnel's port and reuses it.
            super::kill_via_new_connection(conn.clone(), secret.clone(), None, session_id.clone());
        }))
    }
}

/// Map a TDS column type to the Arrow-name strings Octa uses.
fn tds_type_to_arrow(t: tiberius::ColumnType, offset_as_timestamp: bool) -> &'static str {
    use tiberius::ColumnType::*;
    match t {
        Int1 | Int2 | Int4 | Int8 | Intn => "Int64",
        Float4 | Float8 | Floatn | Money | Money4 => "Float64",
        Bit | Bitn => "Boolean",
        Daten => "Date32",
        Datetime | Datetime2 | Datetime4 | Datetimen => "Timestamp(Microsecond, None)",
        DatetimeOffsetn if offset_as_timestamp => "Timestamp(Microsecond, None)",
        BigVarBin | BigBinary | Image => "Binary",
        // DECIMAL as exact text, and by default DATETIMEOFFSET as text with
        // its offset (see `tds_cell`), with the strings, GUIDs, times and xml.
        _ => "Utf8",
    }
}
