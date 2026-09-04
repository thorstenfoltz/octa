//! PostgreSQL connector (tokio-postgres + rustls). SQL goes to the server
//! verbatim, so the user writes real Postgres dialect and the server's own
//! permissions apply.

use anyhow::{Context, Result};
use tokio_postgres::types::{FromSql, Type};

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
        // Through a jump host the socket goes to the loopback forward while
        // `host` stays the database's own name. tokio-postgres validates the
        // certificate against `host` and dials `hostaddr`, so a tunnelled
        // connection verifies TLS exactly as a direct one does.
        if conn.is_tunnelled() {
            let (dial_host, dial_port) = conn.dial_target();
            let addr: std::net::IpAddr = dial_host
                .parse()
                .context("the SSH tunnel's local address is not an IP")?;
            cfg.hostaddr(addr).port(dial_port);
        }
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

/// Decode Postgres' binary `numeric` wire format into an exact decimal string.
///
/// tokio-postgres has no `FromSql` for `NUMERIC` unless a decimal crate is
/// compiled in, and the generic text fallback does not apply either: asking
/// for a `String` returns `Err`, which used to be swallowed and turned the
/// cell into NULL. Since a database tab writes back full rows, that silently
/// replaced real values on the server with NULL. Hence an exact decoder here
/// rather than a lossy hop through `f64`.
///
/// Layout (`src/backend/utils/adt/numeric.c`): `i16 ndigits`, `i16 weight`,
/// `u16 sign`, `u16 dscale`, then `ndigits` base-10000 groups.
fn decode_pg_numeric(raw: &[u8]) -> Option<String> {
    if raw.len() < 8 {
        return None;
    }
    let be16 = |o: usize| i16::from_be_bytes([raw[o], raw[o + 1]]);
    let ndigits = be16(0);
    let weight = be16(2) as i32;
    let sign = be16(4) as u16;
    let dscale = be16(6) as usize;
    if ndigits < 0 || raw.len() < 8 + ndigits as usize * 2 {
        return None;
    }
    match sign {
        0xC000 => return Some("NaN".to_string()),
        0xD000 => return Some("Infinity".to_string()),
        0xF000 => return Some("-Infinity".to_string()),
        _ => {}
    }
    let digits: Vec<i16> = (0..ndigits as usize).map(|i| be16(8 + i * 2)).collect();

    let mut out = String::new();
    if sign == 0x4000 {
        out.push('-');
    }
    // Integer part: groups 0..=weight, the first written bare so 1 does not
    // become 0001.
    if weight < 0 {
        out.push('0');
    } else {
        for i in 0..=weight {
            let d = digits.get(i as usize).copied().unwrap_or(0);
            if i == 0 {
                out.push_str(&d.to_string());
            } else {
                out.push_str(&format!("{d:04}"));
            }
        }
    }
    // Fractional part: keep exactly `dscale` digits, padding with the zero
    // groups that a negative weight implies.
    if dscale > 0 {
        out.push('.');
        let mut frac = String::new();
        let mut i = weight + 1;
        while frac.len() < dscale {
            let d = if i < 0 {
                0
            } else {
                digits.get(i as usize).copied().unwrap_or(0)
            };
            frac.push_str(&format!("{d:04}"));
            i += 1;
        }
        frac.truncate(dscale);
        out.push_str(&frac);
    }
    Some(out)
}

/// `NUMERIC` as its exact decimal text. See [`decode_pg_numeric`].
#[derive(Debug)]
struct PgNumeric(String);

impl<'a> FromSql<'a> for PgNumeric {
    fn from_sql(
        _ty: &Type,
        raw: &'a [u8],
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        decode_pg_numeric(raw)
            .map(PgNumeric)
            .ok_or_else(|| "malformed numeric wire value".into())
    }

    fn accepts(ty: &Type) -> bool {
        matches!(*ty, Type::NUMERIC)
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
        Type::NUMERIC => row
            .try_get::<_, Option<PgNumeric>>(i)
            .ok()
            .flatten()
            .map(|n| CellValue::String(n.0))
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

#[cfg(test)]
mod numeric_tests {
    use super::decode_pg_numeric;

    /// Build a binary `numeric` payload the way Postgres does.
    fn enc(weight: i16, sign: u16, dscale: u16, digits: &[i16]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&(digits.len() as i16).to_be_bytes());
        v.extend_from_slice(&weight.to_be_bytes());
        v.extend_from_slice(&sign.to_be_bytes());
        v.extend_from_slice(&dscale.to_be_bytes());
        for d in digits {
            v.extend_from_slice(&d.to_be_bytes());
        }
        v
    }

    #[test]
    fn decodes_a_money_like_value() {
        // 99.50 -> groups [99, 5000], weight 0, scale 2
        assert_eq!(
            decode_pg_numeric(&enc(0, 0, 2, &[99, 5000])).unwrap(),
            "99.50"
        );
    }

    #[test]
    fn keeps_trailing_zeros_of_the_declared_scale() {
        // 120.50 must not come back as 120.5: numeric(10,2) means two digits.
        assert_eq!(
            decode_pg_numeric(&enc(0, 0, 2, &[120, 5000])).unwrap(),
            "120.50"
        );
    }

    #[test]
    fn decodes_values_below_one() {
        // 0.05 -> weight -1 (no integer group at all)
        assert_eq!(decode_pg_numeric(&enc(-1, 0, 2, &[500])).unwrap(), "0.05");
        // 0.000005 = 500 * 10000^-2, so a whole zero group precedes the digits
        assert_eq!(
            decode_pg_numeric(&enc(-2, 0, 6, &[500])).unwrap(),
            "0.000005"
        );
    }

    #[test]
    fn decodes_negative_and_multi_group_values() {
        assert_eq!(
            decode_pg_numeric(&enc(0, 0x4000, 4, &[1234, 5678])).unwrap(),
            "-1234.5678"
        );
        // 10000 needs the second group padded to 0000, not written as 0.
        assert_eq!(decode_pg_numeric(&enc(1, 0, 0, &[1])).unwrap(), "10000");
    }

    #[test]
    fn decodes_the_special_signs() {
        assert_eq!(decode_pg_numeric(&enc(0, 0xC000, 0, &[])).unwrap(), "NaN");
        assert_eq!(
            decode_pg_numeric(&enc(0, 0xD000, 0, &[])).unwrap(),
            "Infinity"
        );
    }

    #[test]
    fn refuses_a_truncated_payload() {
        assert!(decode_pg_numeric(&[0, 1, 0, 0]).is_none());
        // header promises one group, body has none
        assert!(decode_pg_numeric(&[0, 1, 0, 0, 0, 0, 0, 0]).is_none());
    }
}
