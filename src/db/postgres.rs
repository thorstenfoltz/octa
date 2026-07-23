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
    conn_label: String,
    dialect: PgDialect,
}

impl PostgresConnector {
    /// Connect over TLS as plain PostgreSQL. `stored` is the keyring secret
    /// (password auth); IAM auth mints a token via the aws CLI instead.
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
        let mut cfg = tokio_postgres::Config::new();
        cfg.host(&conn.host)
            .port(conn.port)
            .dbname(&conn.database)
            .user(&conn.username)
            .password(&password);
        let tls = tokio_postgres_rustls::MakeRustlsConnect::new(super::rustls_client_config());
        let (client, connection) = runtime().block_on(cfg.connect(tls)).with_context(|| {
            format!(
                "connecting to {}@{}:{}/{}",
                conn.username, conn.host, conn.port, conn.database
            )
        })?;
        let cancel = client.cancel_token();
        // The connection future drives the socket; it ends when the client drops.
        runtime().spawn(async move {
            let _ = connection.await;
        });
        Ok(Self {
            client,
            cancel,
            conn_label: conn.name.clone(),
            dialect,
        })
    }

    fn rows_to_table(rows: &[tokio_postgres::Row]) -> DataTable {
        let mut table = DataTable::empty();
        let Some(first) = rows.first() else {
            return table;
        };
        table.columns = first
            .columns()
            .iter()
            .map(|c| ColumnInfo {
                name: c.name().to_string(),
                data_type: pg_type_to_arrow(c.type_()).to_string(),
            })
            .collect();
        table.rows = rows
            .iter()
            .map(|row| {
                (0..row.columns().len())
                    .map(|i| pg_value_to_cell(row, i))
                    .collect()
            })
            .collect();
        table
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
        _ => "Utf8",
    }
}

/// Extract one cell, trying the tightest Rust type for the column's wire
/// type and degrading to text, then Null.
fn pg_value_to_cell(row: &tokio_postgres::Row, i: usize) -> CellValue {
    let ty = row.columns()[i].type_().clone();
    match ty {
        Type::INT2 => row
            .try_get::<_, Option<i16>>(i)
            .ok()
            .flatten()
            .map(|v| CellValue::Int(v as i64))
            .unwrap_or(CellValue::Null),
        Type::INT4 => row
            .try_get::<_, Option<i32>>(i)
            .ok()
            .flatten()
            .map(|v| CellValue::Int(v as i64))
            .unwrap_or(CellValue::Null),
        Type::INT8 => row
            .try_get::<_, Option<i64>>(i)
            .ok()
            .flatten()
            .map(CellValue::Int)
            .unwrap_or(CellValue::Null),
        Type::FLOAT4 => row
            .try_get::<_, Option<f32>>(i)
            .ok()
            .flatten()
            .map(|v| CellValue::Float(v as f64))
            .unwrap_or(CellValue::Null),
        Type::FLOAT8 => row
            .try_get::<_, Option<f64>>(i)
            .ok()
            .flatten()
            .map(CellValue::Float)
            .unwrap_or(CellValue::Null),
        Type::BOOL => row
            .try_get::<_, Option<bool>>(i)
            .ok()
            .flatten()
            .map(CellValue::Bool)
            .unwrap_or(CellValue::Null),
        Type::DATE => row
            .try_get::<_, Option<chrono::NaiveDate>>(i)
            .ok()
            .flatten()
            .map(|d| CellValue::Date(d.format("%Y-%m-%d").to_string()))
            .unwrap_or(CellValue::Null),
        Type::TIMESTAMP => row
            .try_get::<_, Option<chrono::NaiveDateTime>>(i)
            .ok()
            .flatten()
            .map(|d| CellValue::DateTime(d.format("%Y-%m-%d %H:%M:%S").to_string()))
            .unwrap_or(CellValue::Null),
        Type::TIMESTAMPTZ => row
            .try_get::<_, Option<chrono::DateTime<chrono::Utc>>>(i)
            .ok()
            .flatten()
            .map(|d| CellValue::DateTime(d.format("%Y-%m-%d %H:%M:%S").to_string()))
            .unwrap_or(CellValue::Null),
        _ => row
            .try_get::<_, Option<String>>(i)
            .ok()
            .flatten()
            .map(CellValue::String)
            .unwrap_or(CellValue::Null),
    }
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
        // Stream the result and stop at the initial-load row cap so an
        // unbounded SELECT cannot exhaust memory (dropping the stream
        // discards the remainder; the connection task handles it).
        let cap = crate::formats::initial_load_rows();
        let rows = runtime()
            .block_on(async {
                use futures_util::TryStreamExt;
                let params: &[&(dyn tokio_postgres::types::ToSql + Sync)] = &[];
                let stream = self.client.query_raw(sql, params.iter().copied()).await?;
                tokio::pin!(stream);
                let mut rows = Vec::new();
                while let Some(row) = stream.try_next().await? {
                    rows.push(row);
                    if rows.len() >= cap {
                        break;
                    }
                }
                Ok::<_, tokio_postgres::Error>(rows)
            })
            .with_context(|| format!("querying '{}'", self.conn_label))?;
        Ok(Self::rows_to_table(&rows))
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
        Some(Box::new(move || {
            let cancel = token.clone();
            let tls = tokio_postgres_rustls::MakeRustlsConnect::new(super::rustls_client_config());
            runtime().spawn(async move {
                let _ = cancel.cancel_query(tls).await;
            });
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redshift_uses_svv_catalog() {
        assert!(list_tables_sql(PgDialect::Redshift, "public").contains("svv_redshift_tables"));
        assert!(list_tables_sql(PgDialect::Postgres, "public").contains("information_schema"));
        assert!(list_schemas_sql(PgDialect::Redshift).contains("svv_redshift_schemas"));
    }

    #[test]
    fn table_schema_literal_is_escaped() {
        assert!(list_tables_sql(PgDialect::Postgres, "a'b").contains("'a''b'"));
    }
}
