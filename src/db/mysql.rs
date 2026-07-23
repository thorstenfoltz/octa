//! MySQL / MariaDB connector (mysql_async + rustls). SQL goes to the server
//! verbatim (real MySQL dialect).
//!
//! TLS: enforced for every token auth mode (AWS RDS IAM, Azure AD, GCP IAM
//! all require it and send the token via the cleartext plugin, safe inside
//! TLS); password auth keeps the driver's default so local/dev servers
//! without TLS keep working.
//! ponytail: no per-connection TLS toggle yet; add one to `DbConnection` if
//! a password-auth server requires TLS.

use anyhow::{Context, Result};
use mysql_async::prelude::Queryable;

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::{DbAuth, DbConnection, DbConnector, DbWriteMode, DbWriteReport, auth, runtime};

pub struct MySqlConnector {
    conn: mysql_async::Conn,
    conn_label: String,
    /// The server-side session this connector owns, used to kill its work
    /// from a second connection.
    session_id: Option<String>,
    /// A clone of the connection settings plus its secret, so the cancel
    /// closure can open a throwaway connection. Both are already in memory
    /// for the lifetime of the connector.
    reconnect: (DbConnection, Option<String>),
}

impl MySqlConnector {
    pub fn connect(conn: &DbConnection, stored: Option<&str>) -> Result<Self> {
        let password = auth::resolve_password(conn, stored)?;
        let mut builder = mysql_async::OptsBuilder::default()
            .ip_or_hostname(conn.host.clone())
            .tcp_port(conn.port)
            .db_name(Some(conn.database.clone()))
            .user(Some(conn.username.clone()))
            .pass(Some(password));
        if !matches!(conn.auth, DbAuth::Password) {
            // Token auth (RDS IAM / Azure AD / Cloud SQL IAM) is only
            // accepted over TLS, and the token is sent via the cleartext
            // plugin (safe inside TLS).
            builder = builder
                .ssl_opts(mysql_async::SslOpts::default())
                .enable_cleartext_plugin(true);
        }
        let mut client = runtime()
            .block_on(mysql_async::Conn::new(builder))
            .with_context(|| {
                format!(
                    "connecting to {}@{}:{}/{}",
                    conn.username, conn.host, conn.port, conn.database
                )
            })?;
        let session_id: Option<String> = runtime()
            .block_on(client.query_first::<u64, _>("SELECT CONNECTION_ID()"))
            .ok()
            .flatten()
            .map(|id| id.to_string());
        Ok(Self {
            conn: client,
            conn_label: conn.name.clone(),
            session_id,
            reconnect: (conn.clone(), stored.map(str::to_string)),
        })
    }
}

/// Map a MySQL column type to the Arrow-name strings Octa uses.
fn mysql_type_to_arrow(t: mysql_async::consts::ColumnType) -> &'static str {
    use mysql_async::consts::ColumnType::*;
    match t {
        MYSQL_TYPE_TINY | MYSQL_TYPE_SHORT | MYSQL_TYPE_LONG | MYSQL_TYPE_LONGLONG
        | MYSQL_TYPE_INT24 | MYSQL_TYPE_YEAR => "Int64",
        MYSQL_TYPE_FLOAT | MYSQL_TYPE_DOUBLE => "Float64",
        MYSQL_TYPE_DATE | MYSQL_TYPE_NEWDATE => "Date32",
        MYSQL_TYPE_DATETIME
        | MYSQL_TYPE_TIMESTAMP
        | MYSQL_TYPE_DATETIME2
        | MYSQL_TYPE_TIMESTAMP2 => "Timestamp(Microsecond, None)",
        _ => "Utf8",
    }
}

/// Convert one wire value. Plain `query()` uses MySQL's *text* protocol,
/// where every non-NULL value arrives as `Bytes` - so `Bytes` is re-typed
/// from the column's declared type instead of landing as text wholesale.
fn mysql_value_to_cell(v: &mysql_async::Value, col_type: &str) -> CellValue {
    use mysql_async::Value::*;
    match v {
        NULL => CellValue::Null,
        Int(i) => CellValue::Int(*i),
        UInt(u) => i64::try_from(*u)
            .map(CellValue::Int)
            .unwrap_or_else(|_| CellValue::String(u.to_string())),
        Float(f) => CellValue::Float(*f as f64),
        Double(d) => CellValue::Float(*d),
        Bytes(b) => {
            let s = String::from_utf8_lossy(b).to_string();
            match col_type {
                "Int64" => s
                    .parse::<i64>()
                    .map(CellValue::Int)
                    .unwrap_or(CellValue::String(s)),
                "Float64" => s
                    .parse::<f64>()
                    .map(CellValue::Float)
                    .unwrap_or(CellValue::String(s)),
                "Date32" => CellValue::Date(s),
                t if t.starts_with("Timestamp") => CellValue::DateTime(s),
                _ => CellValue::String(s),
            }
        }
        Date(y, m, d, hh, mm, ss, _us) => {
            if *hh == 0 && *mm == 0 && *ss == 0 {
                CellValue::Date(format!("{y:04}-{m:02}-{d:02}"))
            } else {
                CellValue::DateTime(format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}"))
            }
        }
        Time(neg, days, hh, mm, ss, _us) => {
            let sign = if *neg { "-" } else { "" };
            let hours = u32::from(*hh) + *days * 24;
            CellValue::String(format!("{sign}{hours:02}:{mm:02}:{ss:02}"))
        }
    }
}

impl DbConnector for MySqlConnector {
    fn engine(&self) -> super::DbEngine {
        super::DbEngine::MySql
    }

    fn list_schemas(&mut self, _catalog: Option<&str>) -> Result<Vec<String>> {
        let rows: Vec<String> = runtime()
            .block_on(self.conn.query(
                "SELECT schema_name FROM information_schema.schemata \
                 WHERE schema_name NOT IN \
                 ('mysql', 'information_schema', 'performance_schema', 'sys') \
                 ORDER BY schema_name",
            ))
            .context("listing schemas")?;
        Ok(rows)
    }

    fn list_tables(&mut self, _catalog: Option<&str>, schema: &str) -> Result<Vec<String>> {
        let rows: Vec<String> = runtime()
            .block_on(self.conn.exec(
                "SELECT table_name FROM information_schema.tables \
                 WHERE table_schema = ? ORDER BY table_name",
                (schema,),
            ))
            .with_context(|| format!("listing tables of schema {schema}"))?;
        Ok(rows)
    }

    fn query(&mut self, sql: &str) -> Result<DataTable> {
        // Stream rows and stop at the initial-load row cap; `drop_result`
        // drains the remaining protocol packets without materialising them,
        // so an unbounded SELECT cannot exhaust memory.
        let cap = crate::formats::initial_load_rows();
        let rows: Vec<mysql_async::Row> = runtime()
            .block_on(async {
                let mut result = self.conn.query_iter(sql).await?;
                let mut rows = Vec::new();
                while let Some(row) = result.next().await? {
                    rows.push(row);
                    if rows.len() >= cap {
                        break;
                    }
                }
                result.drop_result().await?;
                Ok::<_, mysql_async::Error>(rows)
            })
            .with_context(|| format!("querying '{}'", self.conn_label))?;
        let mut table = DataTable::empty();
        let Some(first) = rows.first() else {
            return Ok(table);
        };
        table.columns = first
            .columns_ref()
            .iter()
            .map(|c| ColumnInfo {
                name: c.name_str().to_string(),
                data_type: mysql_type_to_arrow(c.column_type()).to_string(),
            })
            .collect();
        let col_types: Vec<String> = table.columns.iter().map(|c| c.data_type.clone()).collect();
        table.rows = rows
            .iter()
            .map(|row| {
                (0..row.columns_ref().len())
                    .map(|i| {
                        row.as_ref(i)
                            .map(|v| mysql_value_to_cell(v, &col_types[i]))
                            .unwrap_or(CellValue::Null)
                    })
                    .collect()
            })
            .collect();
        Ok(table)
    }

    fn execute(&mut self, sql: &str) -> Result<u64> {
        runtime()
            .block_on(async {
                let result = self.conn.query_iter(sql).await?;
                let affected = result.affected_rows();
                result.drop_result().await?;
                Ok::<u64, mysql_async::Error>(affected)
            })
            .with_context(|| format!("executing on '{}'", self.conn_label))
    }

    fn write_table(
        &mut self,
        catalog: Option<&str>,
        schema: &str,
        table: &str,
        mode: DbWriteMode,
        data: &DataTable,
    ) -> Result<DbWriteReport> {
        super::reject_catalog(self.engine(), catalog)?;
        super::write_table_generic(
            self,
            super::DbEngine::MySql,
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
        Some(Box::new(move || {
            super::kill_via_new_connection(conn.clone(), secret.clone(), session_id.clone());
        }))
    }
}
