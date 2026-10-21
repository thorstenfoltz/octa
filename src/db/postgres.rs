//! PostgreSQL connector (tokio-postgres + rustls). SQL goes to the server
//! verbatim, so the user writes real Postgres dialect and the server's own
//! permissions apply.

use anyhow::{Context, Result};
use tokio_postgres::types::Type;

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::{DbConnection, DbConnector, DbWriteMode, DbWriteReport, auth, runtime};

/// Which Postgres-wire flavour a connector speaks. Redshift is wire-compatible
/// but has its own system catalogue views (`information_schema` on Redshift
/// omits late-binding and external/Spectrum objects).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgDialect {
    Postgres,
    Redshift,
}

/// Schema-listing SQL for the dialect.
fn list_schemas_sql(d: PgDialect) -> &'static str {
    match d {
        PgDialect::Redshift => {
            "SELECT schema_name FROM svv_redshift_schemas \
             WHERE schema_name NOT IN ('pg_catalog', 'information_schema') \
             ORDER BY schema_name"
        }
        PgDialect::Postgres => {
            "SELECT schema_name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('pg_catalog', 'information_schema') \
             ORDER BY schema_name"
        }
    }
}

/// Table-listing SQL for the dialect (schema literal single-quote escaped).
fn list_tables_sql(d: PgDialect, schema: &str) -> String {
    let s = schema.replace('\'', "''");
    match d {
        PgDialect::Redshift => format!(
            "SELECT table_name FROM svv_redshift_tables \
             WHERE schema_name = '{s}' ORDER BY table_name"
        ),
        PgDialect::Postgres => format!(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = '{s}' ORDER BY table_name"
        ),
    }
}

pub struct PostgresConnector {
    client: tokio_postgres::Client,
    cancel: tokio_postgres::CancelToken,
    /// The connection's TLS config, so a cancel speaks to the server exactly
    /// as the connect did (a default config would refuse the very servers an
    /// SSL mode was chosen for).
    tls: rustls::ClientConfig,
    conn_label: String,
    dialect: PgDialect,
    /// A connection without "Allow writes": every transaction is read-only
    /// (see [`READ_ONLY_SQL`]). Plain PostgreSQL only; Redshift is untested.
    read_only: bool,
}

/// Makes every later transaction of the session read-only, so the server
/// refuses a write the statement check missed: `WITH x AS (DELETE ...)
/// SELECT`, `SELECT ... INTO`, a function that writes. `query` sends it again
/// before every statement, because `SELECT set_config(...)` turns it off
/// and passes for a read.
const READ_ONLY_SQL: &str = "SET default_transaction_read_only TO on";

impl PostgresConnector {
    /// Connect as plain PostgreSQL, encrypted as the connection's SSL mode
    /// says. `stored` is the keyring secret (password auth); IAM auth mints a
    /// token via the aws CLI instead.
    pub fn connect(conn: &DbConnection, stored: Option<&str>) -> Result<Self> {
        Self::connect_with_dialect(conn, stored, PgDialect::Postgres)
    }

    /// Connect, tagging the connector with a Postgres-wire dialect (plain
    /// Postgres or Redshift). Only the catalogue SQL and reported engine
    /// differ; the wire protocol and TLS setup are identical.
    pub fn connect_with_dialect(
        conn: &DbConnection,
        stored: Option<&str>,
        dialect: PgDialect,
    ) -> Result<Self> {
        let password = auth::resolve_password(conn, stored)?;
        // Through a jump host or Connect via the socket goes elsewhere while
        // `host` stays the name on the certificate. tokio-postgres validates
        // the certificate against `host` and dials `hostaddr`, so either way
        // TLS is checked exactly as for a direct connection. `port` is set
        // once: tokio-postgres keeps a list, and a second entry for one host
        // is "invalid number of ports" (which is how a tunnelled connect
        // failed before it ever dialled).
        let (dial_host, dial_port) = conn.dial_target();
        let mut cfg = tokio_postgres::Config::new();
        cfg.host(&conn.host)
            .port(dial_port)
            .dbname(&conn.database)
            .user(&conn.username)
            .password(&password);
        if dial_host != conn.host {
            let addr: std::net::IpAddr = dial_host.parse().with_context(|| {
                format!(
                    "{dial_host} is not an IP address (Connect via takes one, such as 127.0.0.1)"
                )
            })?;
            cfg.hostaddr(addr);
        }
        let (ssl_mode, tls) =
            super::tls::postgres_tls(&conn.tls, super::tls::effective_mode(conn))?;
        cfg.ssl_mode(ssl_mode);
        let connector = tokio_postgres_rustls::MakeRustlsConnect::new(tls.clone());
        let (client, connection) = runtime()
            .block_on(cfg.connect(connector))
            .with_context(|| {
                format!(
                    "connecting to {}@{}:{}/{}",
                    conn.username, conn.host, conn.port, conn.database
                )
            })
            .map_err(super::tls::certificate_hint)?;
        let cancel = client.cancel_token();
        // The connection future drives the socket; it ends when the client drops.
        runtime().spawn(async move {
            let _ = connection.await;
        });
        // Octa shows `timestamptz` in UTC and saves it back as that text.
        // Postgres reads text without a zone in the session's zone, which
        // initdb takes from the machine, so on a server in Germany a save
        // moved every such value by an hour or two. In UTC the two agree.
        runtime()
            .block_on(client.batch_execute("SET timezone TO 'UTC'"))
            .context("setting the session time zone to UTC")?;
        // Octa writes text with `''` for a quote and the backslash as itself,
        // which is only right while this is on. It is the default since 9.1;
        // a server that turns it off would read `C:\temp` as `C:`, a tab and
        // `emp`. Redshift has no such setting.
        if dialect == PgDialect::Postgres {
            runtime()
                .block_on(client.batch_execute("SET standard_conforming_strings TO on"))
                .context("switching on standard_conforming_strings")?;
        }
        let read_only = dialect == PgDialect::Postgres && !conn.allow_writes;
        if read_only {
            runtime()
                .block_on(client.batch_execute(READ_ONLY_SQL))
                .context("making the session read-only")?;
        }
        Ok(Self {
            client,
            cancel,
            tls,
            conn_label: conn.name.clone(),
            dialect,
            read_only,
        })
    }
}

/// Map a Postgres wire type to the Arrow-name strings the rest of Octa uses
/// (same vocabulary as `duckdb_type_to_arrow`). NUMERIC arrives as text: a
/// lossless decimal has no f64 representation.
fn pg_type_to_arrow(t: &Type) -> &'static str {
    match *t {
        Type::INT2 | Type::INT4 | Type::INT8 => "Int64",
        Type::FLOAT4 | Type::FLOAT8 => "Float64",
        Type::BOOL => "Boolean",
        Type::DATE => "Date32",
        Type::TIMESTAMP | Type::TIMESTAMPTZ => "Timestamp(Microsecond, None)",
        // Exact decimal text: a lossless NUMERIC has no f64 representation.
        Type::NUMERIC => "Utf8",
        _ => "Utf8",
    }
}

/// Turn one value, as the text Postgres itself prints, into a cell.
///
/// Values arrive over the text protocol (see `query`), so every type has a
/// form here. The binary protocol used before handed the driver a fixed list
/// of types and turned the rest (`uuid`, `json`, `bytea`, `interval`, arrays,
/// enums, ...) into NULL, and since a database tab writes back whole rows,
/// saving any edit then wrote that NULL over the real value on the server.
///
/// The types Octa understands are parsed into the same cells the binary
/// decoders produced. Everything else, and any value those parses cannot read
/// (`infinity`, a BC date, a non-ISO `DateStyle`), keeps Postgres's own text:
/// Postgres accepts that text back as input, so a write-back round-trips it.
fn text_to_cell(ty: &Type, text: Option<&str>) -> CellValue {
    use chrono::{DateTime, NaiveDate, NaiveDateTime};
    let Some(s) = text else {
        return CellValue::Null;
    };
    let typed = match *ty {
        Type::INT2 | Type::INT4 | Type::INT8 => s.parse().ok().map(CellValue::Int),
        Type::FLOAT4 | Type::FLOAT8 => s.parse().ok().map(CellValue::Float),
        Type::BOOL => match s {
            "t" => Some(CellValue::Bool(true)),
            "f" => Some(CellValue::Bool(false)),
            _ => None,
        },
        Type::DATE => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .ok()
            .map(|d| CellValue::Date(d.format("%Y-%m-%d").to_string())),
        Type::TIMESTAMP => NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
            .ok()
            .map(|d| CellValue::DateTime(super::datetime_text(&d))),
        // Printed in the session's time zone; shown in UTC, as before.
        Type::TIMESTAMPTZ => DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f%#z")
            .ok()
            .map(|d| CellValue::DateTime(super::datetime_text(&d.naive_utc()))),
        // `bytea_output = hex`, the default since Postgres 9.0.
        Type::BYTEA => s
            .strip_prefix("\\x")
            .and_then(crate::data::hex_decode)
            .map(CellValue::Binary),
        _ => None,
    };
    typed.unwrap_or_else(|| CellValue::String(s.to_string()))
}

impl DbConnector for PostgresConnector {
    fn engine(&self) -> super::DbEngine {
        match self.dialect {
            PgDialect::Postgres => super::DbEngine::Postgres,
            PgDialect::Redshift => super::DbEngine::Redshift,
        }
    }

    fn list_schemas(&mut self, _catalog: Option<&str>) -> Result<Vec<String>> {
        let rows = runtime()
            .block_on(self.client.query(list_schemas_sql(self.dialect), &[]))
            .context("listing schemas")?;
        Ok(rows.iter().map(|r| r.get::<_, String>(0)).collect())
    }

    fn list_tables(&mut self, _catalog: Option<&str>, schema: &str) -> Result<Vec<String>> {
        let rows = runtime()
            .block_on(
                self.client
                    .query(&list_tables_sql(self.dialect, schema), &[]),
            )
            .with_context(|| format!("listing tables of schema {schema}"))?;
        Ok(rows.iter().map(|r| r.get::<_, String>(0)).collect())
    }

    fn query(&mut self, sql: &str) -> Result<DataTable> {
        // Describe the statement for its column names and types (nothing
        // runs), then run it over the text protocol, so every value arrives
        // as Postgres prints it (see `text_to_cell`). Stream the rows and
        // stop at the initial-load row cap so an unbounded SELECT cannot
        // exhaust memory; dropping the stream discards the remainder.
        let cap = crate::formats::initial_load_rows();
        runtime()
            .block_on(async {
                use futures_util::TryStreamExt;
                use tokio_postgres::SimpleQueryMessage;
                let statement = if self.read_only {
                    // Both go out before either answer comes back (the
                    // client queues them in this order), so this costs no
                    // extra round trip.
                    let (_, statement) = tokio::try_join!(
                        self.client.batch_execute(READ_ONLY_SQL),
                        self.client.prepare(sql)
                    )?;
                    statement
                } else {
                    self.client.prepare(sql).await?
                };
                let mut table = DataTable::empty();
                table.columns = statement
                    .columns()
                    .iter()
                    .map(|c| ColumnInfo {
                        name: c.name().to_string(),
                        data_type: pg_type_to_arrow(c.type_()).to_string(),
                    })
                    .collect();
                let stream = self.client.simple_query_raw(sql).await?;
                tokio::pin!(stream);
                while let Some(message) = stream.try_next().await? {
                    let SimpleQueryMessage::Row(row) = message else {
                        continue;
                    };
                    table.rows.push(
                        statement
                            .columns()
                            .iter()
                            .enumerate()
                            .map(|(i, c)| text_to_cell(c.type_(), row.try_get(i).ok().flatten()))
                            .collect(),
                    );
                    if table.rows.len() >= cap {
                        break;
                    }
                }
                Ok::<_, tokio_postgres::Error>(table)
            })
            .with_context(|| format!("querying '{}'", self.conn_label))
    }

    fn execute(&mut self, sql: &str) -> Result<u64> {
        runtime()
            .block_on(self.client.execute(sql, &[]))
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
            super::DbEngine::Postgres,
            None,
            schema,
            table,
            mode,
            data,
        )
    }

    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        let token = self.cancel.clone();
        let config = self.tls.clone();
        Some(Box::new(move || {
            let cancel = token.clone();
            let tls = tokio_postgres_rustls::MakeRustlsConnect::new(config.clone());
            runtime().spawn(async move {
                let _ = cancel.cancel_query(tls).await;
            });
        }))
    }
}

#[cfg(test)]
#[path = "postgres_tests.rs"]
mod tests;
