//! Exasol connector via `sqlx-exasol` (async; driven through the shared blocking
//! [`runtime`] like the MySQL connector). Rows come back as `serde_json`-backed
//! `ExaValue`s; we decode each column generically with `try_get_unchecked`
//! (which skips sqlx's type-compat gate and just runs `Decode` on the inner
//! JSON), trying i64/f64/bool/String guided by the column's declared type. This
//! deliberately avoids sqlx's `chrono`/`rust_decimal` features, whose macro
//! crate drags in `sqlx-sqlite` and collides with octa's bundled libsqlite3.

use anyhow::{Context, Result};
use sqlx_exasol::{
    AssertSqlSafe, Column, ConnectOptions, ExaConnectOptions, ExaConnection, ExaRow, Row, TypeInfo,
    query,
};

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::{DbConnection, DbConnector, DbEngine, DbWriteMode, DbWriteReport, runtime};

pub struct ExasolConnector {
    conn: ExaConnection,
    conn_label: String,
    /// The server-side session this connector owns, used to kill its work
    /// from a second connection.
    session_id: Option<String>,
    /// A clone of the connection settings plus its secret, so the cancel
    /// closure can open a throwaway connection. Both are already in memory
    /// for the lifetime of the connector.
    reconnect: (DbConnection, Option<String>),
}

impl ExasolConnector {
    /// Open a WebSocket connection (password auth). Installs the ring crypto
    /// provider once so sqlx's internal rustls config can build.
    pub fn connect(conn: &DbConnection, stored: Option<&str>) -> Result<Self> {
        use std::sync::Once;
        static CRYPTO: Once = Once::new();
        CRYPTO.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });

        let password = super::auth::resolve_password(conn, stored)?;
        let opts = ExaConnectOptions::builder()
            .host(conn.host.clone())
            .port(conn.port)
            .username(conn.username.clone())
            .password(password)
            .schema(conn.database.clone())
            .build()
            .context("building Exasol connection options")?;
        let c = runtime().block_on(opts.connect()).with_context(|| {
            format!(
                "connecting to {}@{}:{}",
                conn.username, conn.host, conn.port
            )
        })?;
        let mut me = Self {
            conn: c,
            conn_label: conn.name.clone(),
            session_id: None,
            reconnect: (conn.clone(), stored.map(str::to_string)),
        };
        // Failing to read the session id just means no cancel handle, which
        // is the pre-existing behaviour.
        let session_id = me
            .catalogue("SELECT CURRENT_SESSION")
            .ok()
            .and_then(|ids| ids.into_iter().next());
        me.session_id = session_id;
        Ok(me)
    }

    /// Run a SELECT and materialise it, capped at the initial-load row limit.
    fn run_select(&mut self, sql: &str) -> Result<DataTable> {
        let cap = crate::formats::initial_load_rows();
        let inner = sql.trim().trim_end_matches(';');
        let wrapped = format!("SELECT * FROM ({inner}) AS OCTA_Q LIMIT {cap}");
        let rows: Vec<ExaRow> = runtime()
            .block_on(
                query::<sqlx_exasol::Exasol>(AssertSqlSafe(wrapped)).fetch_all(&mut self.conn),
            )
            .with_context(|| format!("querying '{}'", self.conn_label))?;
        Ok(rows_to_table(&rows))
    }

    /// The single-column text results of a catalogue query.
    fn catalogue(&mut self, sql: &str) -> Result<Vec<String>> {
        let t = self.run_select(sql)?;
        Ok(t.rows
            .into_iter()
            .filter_map(|mut r| r.drain(..).next())
            .map(|c| match c {
                CellValue::String(s) => s,
                other => cell_to_text(&other),
            })
            .collect())
    }
}

impl DbConnector for ExasolConnector {
    fn engine(&self) -> DbEngine {
        DbEngine::Exasol
    }

    fn list_schemas(&mut self, _catalog: Option<&str>) -> Result<Vec<String>> {
        self.catalogue("SELECT SCHEMA_NAME FROM EXA_SCHEMAS ORDER BY SCHEMA_NAME")
    }

    fn list_tables(&mut self, _catalog: Option<&str>, schema: &str) -> Result<Vec<String>> {
        let esc = schema.replace('\'', "''");
        self.catalogue(&format!(
            "SELECT TABLE_NAME FROM EXA_ALL_TABLES \
             WHERE TABLE_SCHEMA = '{esc}' ORDER BY TABLE_NAME"
        ))
    }

    fn query(&mut self, sql: &str) -> Result<DataTable> {
        self.run_select(sql)
    }

    fn execute(&mut self, sql: &str) -> Result<u64> {
        // Exasol has no `BEGIN` statement (transactions are implicit); swallow
        // it so the shared write_table_generic driver works. COMMIT/ROLLBACK
        // are real and pass through.
        if sql.trim_start().len() >= 5 && sql.trim_start()[..5].eq_ignore_ascii_case("BEGIN") {
            return Ok(0);
        }
        let res = runtime()
            .block_on(
                query::<sqlx_exasol::Exasol>(AssertSqlSafe(sql.to_string()))
                    .execute(&mut self.conn),
            )
            .with_context(|| format!("executing on '{}'", self.conn_label))?;
        Ok(res.rows_affected())
    }

    fn write_table(
        &mut self,
        catalog: Option<&str>,
        schema: &str,
        table: &str,
        mode: DbWriteMode,
        data: &DataTable,
    ) -> Result<DbWriteReport> {
        // ponytail: reuses the literal-INSERT writer. Append works; Create /
        // Replace emit generic (Postgres-flavoured) DDL that may not match
        // Exasol types (no TEXT) until an Exasol DDL dialect lands. Writes are
        // not guaranteed atomic (implicit transactions).
        super::reject_catalog(self.engine(), catalog)?;
        super::write_table_generic(self, DbEngine::Exasol, None, schema, table, mode, data)
    }

    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        let session_id = self.session_id.clone()?;
        let (conn, secret) = self.reconnect.clone();
        Some(Box::new(move || {
            super::kill_via_new_connection(conn.clone(), secret.clone(), session_id.clone());
        }))
    }
}

/// Map sqlx `ExaRow`s to a [`DataTable`], decoding each column by its type.
fn rows_to_table(rows: &[ExaRow]) -> DataTable {
    let mut table = DataTable::empty();
    let Some(first) = rows.first() else {
        return table;
    };
    let cats: Vec<(ColumnInfo, Cat)> = first
        .columns()
        .iter()
        .map(|c| {
            let cat = categorise(c.type_info().name());
            (
                ColumnInfo {
                    name: c.name().to_string(),
                    data_type: cat.arrow().to_string(),
                },
                cat,
            )
        })
        .collect();
    table.rows = rows
        .iter()
        .map(|row| {
            cats.iter()
                .enumerate()
                .map(|(i, (_, cat))| decode_cell(row, i, *cat))
                .collect()
        })
        .collect();
    table.columns = cats.into_iter().map(|(ci, _)| ci).collect();
    table
}

/// Broad category of an Exasol column, driving both the Arrow type name and
/// which Rust type we decode into.
#[derive(Debug, Clone, Copy)]
enum Cat {
    Int,
    Decimal,
    Double,
    Bool,
    Date,
    Timestamp,
    Text,
}

impl Cat {
    fn arrow(self) -> &'static str {
        match self {
            Cat::Int => "Int64",
            Cat::Decimal | Cat::Double => "Float64",
            Cat::Bool => "Boolean",
            Cat::Date => "Date32",
            Cat::Timestamp => "Timestamp(Microsecond, None)",
            Cat::Text => "Utf8",
        }
    }
}

/// Classify from the Exasol type name string (e.g. "DECIMAL(18, 0)",
/// "VARCHAR(100) UTF8", "DOUBLE", "TIMESTAMP").
fn categorise(name: &str) -> Cat {
    let n = name.trim();
    if n.starts_with("BOOLEAN") {
        Cat::Bool
    } else if n.starts_with("DOUBLE") {
        Cat::Double
    } else if n.starts_with("DECIMAL") {
        // Scale 0 -> integer, otherwise fractional.
        if decimal_scale_is_zero(n) {
            Cat::Int
        } else {
            Cat::Decimal
        }
    } else if n.starts_with("DATE") {
        Cat::Date
    } else if n.starts_with("TIMESTAMP") {
        Cat::Timestamp
    } else {
        Cat::Text
    }
}

/// True for a `DECIMAL(p, 0)` (or `DECIMAL(*, 0)`) type name.
fn decimal_scale_is_zero(name: &str) -> bool {
    name.rsplit_once(',')
        .map(|(_, scale)| scale.trim().trim_end_matches(')').trim() == "0")
        .unwrap_or(false)
}

/// Decode one cell generically via `try_get_unchecked`, preferring the Rust
/// type suggested by the column category but falling through the number/string
/// forms the JSON value can actually take (a `DECIMAL` wider than i64 arrives as
/// a JSON string; a whole `DOUBLE` may arrive as a JSON integer). NULL and any
/// undecodable value become [`CellValue::Null`].
fn decode_cell(row: &ExaRow, i: usize, cat: Cat) -> CellValue {
    let as_i64 = || {
        row.try_get_unchecked::<Option<i64>, _>(i)
            .ok()
            .flatten()
            .map(CellValue::Int)
    };
    let as_f64 = || {
        row.try_get_unchecked::<Option<f64>, _>(i)
            .ok()
            .flatten()
            .map(CellValue::Float)
    };
    let as_bool = || {
        row.try_get_unchecked::<Option<bool>, _>(i)
            .ok()
            .flatten()
            .map(CellValue::Bool)
    };
    let as_text = || {
        row.try_get_unchecked::<Option<String>, _>(i)
            .ok()
            .flatten()
            .map(|s| match cat {
                Cat::Date => CellValue::Date(s),
                Cat::Timestamp => CellValue::DateTime(s),
                _ => CellValue::String(s),
            })
    };
    let ordered: [&dyn Fn() -> Option<CellValue>; 4] = match cat {
        Cat::Int => [&as_i64, &as_f64, &as_text, &as_bool],
        Cat::Decimal | Cat::Double => [&as_f64, &as_i64, &as_text, &as_bool],
        Cat::Bool => [&as_bool, &as_text, &as_i64, &as_f64],
        Cat::Date | Cat::Timestamp | Cat::Text => [&as_text, &as_i64, &as_f64, &as_bool],
    };
    ordered
        .into_iter()
        .find_map(|f| f())
        .unwrap_or(CellValue::Null)
}

/// Fallback textual form of a non-string cell (catalogue queries only).
fn cell_to_text(c: &CellValue) -> String {
    match c {
        CellValue::Int(i) => i.to_string(),
        CellValue::Float(f) => f.to_string(),
        CellValue::Bool(b) => b.to_string(),
        CellValue::Date(s) | CellValue::DateTime(s) | CellValue::Nested(s) => s.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorise_decimal_scale() {
        assert!(matches!(categorise("DECIMAL(18, 0)"), Cat::Int));
        assert!(matches!(categorise("DECIMAL(10, 2)"), Cat::Decimal));
        assert!(matches!(categorise("DECIMAL(*, 0)"), Cat::Int));
        assert!(matches!(categorise("DOUBLE"), Cat::Double));
        assert!(matches!(categorise("BOOLEAN"), Cat::Bool));
        assert!(matches!(categorise("VARCHAR(100) UTF8"), Cat::Text));
        assert!(matches!(categorise("TIMESTAMP"), Cat::Timestamp));
        assert!(matches!(categorise("DATE"), Cat::Date));
    }

    #[test]
    fn cats_map_to_arrow() {
        assert_eq!(Cat::Int.arrow(), "Int64");
        assert_eq!(Cat::Decimal.arrow(), "Float64");
        assert_eq!(Cat::Timestamp.arrow(), "Timestamp(Microsecond, None)");
    }
}
