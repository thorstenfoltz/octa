//! Trino connector over the HTTP statement API (`POST /v1/statement`, then
//! follow `nextUri` until the result is complete). No driver crate exists for
//! Rust; the protocol is small enough that [`super::rest`] covers it.
//!
//! Trino is genuinely three-level (`catalog.schema.table`), so
//! [`DbEngine::has_catalogs`](super::DbEngine::has_catalogs) is true for it and
//! the sidebar grows a catalog level. What a catalog can do depends on the
//! connector behind it: Hive and Iceberg accept writes, many others are
//! read-only, and Trino says so in its own error text when a write is refused.
//!
//! Transport: HTTPS unless the host is written with an explicit `http://`,
//! which is how a local development coordinator (port 8080, no TLS) is
//! reached. Auth is HTTP basic (Trino requires TLS for it) or a bearer token
//! from browser SSO, since Trino commonly fronts an OIDC provider.

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use serde_json::Value;

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::rest::{InFlight, RestClient};
use super::{
    CancelFlag, DbAuth, DbConnection, DbConnector, DbEngine, DbWriteMode, DbWriteReport, auth,
};

pub struct TrinoConnector {
    client: RestClient,
    /// Sent on every request: auth plus the `X-Trino-*` session headers.
    headers: Vec<(String, String)>,
    /// The catalog from the connection, used when the caller names none.
    default_catalog: String,
    conn_label: String,
    /// Seconds of no progress before the `nextUri` walk gives up. Trino has
    /// no separate "are you done?" call: the same chain of links carries both
    /// the waiting and the rows, so the budget is measured from the last page
    /// that actually delivered data. Bounding total wall clock instead would
    /// abort a large result halfway through reading it.
    timeout_secs: u32,
    cancel: CancelFlag,
    /// The running statement's `nextUri`, which is also its cancel URI.
    in_flight: InFlight,
}

impl TrinoConnector {
    pub fn connect(conn: &DbConnection, secret: Option<&str>) -> Result<Self> {
        let host = conn.host.trim().trim_end_matches('/');
        if host.is_empty() {
            bail!(
                "Trino needs a coordinator host for connection '{}'",
                conn.name
            );
        }
        // An explicit scheme wins, so a plaintext development coordinator is
        // reachable; otherwise TLS, which is what basic auth requires anyway.
        let base = if host.starts_with("http://") || host.starts_with("https://") {
            format!("{host}:{}", conn.port)
        } else {
            format!("https://{host}:{}", conn.port)
        };

        let user = if conn.username.trim().is_empty() {
            "octa"
        } else {
            conn.username.trim()
        };
        let mut headers = vec![
            ("X-Trino-User".to_string(), user.to_string()),
            ("X-Trino-Source".to_string(), "octa".to_string()),
        ];
        if !conn.database.trim().is_empty() {
            headers.push((
                "X-Trino-Catalog".to_string(),
                conn.database.trim().to_string(),
            ));
        }
        match conn.auth {
            // Browser SSO and any other token mode: Trino takes the token as a
            // bearer, which is also how its own OAuth2 authentication works.
            DbAuth::OAuthBrowser | DbAuth::Token | DbAuth::OAuthClientCredentials { .. } => {
                let token = auth::resolve_password(conn, secret)?;
                headers.push(("Authorization".to_string(), format!("Bearer {token}")));
            }
            // Password auth is basic auth, but only when there is a password:
            // a coordinator with no authentication configured identifies the
            // caller by `X-Trino-User` alone, and that is the usual shape of a
            // development cluster. A server that does want credentials answers
            // 401 with its own message, which is clearer than Octa guessing.
            _ => {
                if let Some(password) = secret.map(str::trim).filter(|s| !s.is_empty()) {
                    let raw = format!("{user}:{password}");
                    let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
                    headers.push(("Authorization".to_string(), format!("Basic {encoded}")));
                }
            }
        }

        Ok(Self {
            client: RestClient::new(base),
            headers,
            default_catalog: conn.database.trim().to_string(),
            conn_label: conn.name.clone(),
            timeout_secs: conn.query_timeout_secs,
            cancel: CancelFlag::new(),
            in_flight: InFlight::default(),
        })
    }

    /// Run `sql` to completion, following `nextUri` and accumulating rows up
    /// to `cap`. Reaching the cap cancels the rest with a DELETE, which is how
    /// Trino is told a client has stopped reading.
    fn run(&self, sql: &str, cap: usize) -> Result<DataTable> {
        self.cancel.reset();
        self.in_flight.clear();
        let mut page = self
            .client
            .post_raw("v1/statement", sql.as_bytes(), "text/plain", &self.headers)
            .with_context(|| format!("submitting to '{}'", self.conn_label))?;

        let mut columns: Vec<ColumnInfo> = Vec::new();
        let mut rows: Vec<Vec<CellValue>> = Vec::new();
        let mut capped = false;
        let budget = std::time::Duration::from_secs(u64::from(self.timeout_secs));
        let mut last_progress = std::time::Instant::now();
        loop {
            if let Some(message) = trino_error(&page) {
                bail!("{message}");
            }
            if columns.is_empty()
                && let Some(cols) = page["columns"].as_array()
            {
                columns = cols
                    .iter()
                    .map(|c| ColumnInfo {
                        name: c["name"].as_str().unwrap_or("").to_string(),
                        data_type: trino_type_to_arrow(c["type"].as_str().unwrap_or("varchar"))
                            .to_string(),
                    })
                    .collect();
            }
            if !capped && let Some(data) = page["data"].as_array() {
                if !data.is_empty() {
                    last_progress = std::time::Instant::now();
                }
                for row in data {
                    if rows.len() >= cap {
                        capped = true;
                        break;
                    }
                    rows.push(
                        columns
                            .iter()
                            .enumerate()
                            .map(|(i, col)| {
                                trino_cell(row.get(i).unwrap_or(&Value::Null), &col.data_type)
                            })
                            .collect(),
                    );
                }
            }
            let Some(next) = page["nextUri"].as_str().map(str::to_string) else {
                break;
            };
            self.in_flight.set(&next);
            if capped || self.cancel.is_cancelled() {
                // Trino keeps the query running until the client either reads
                // it out or says it has stopped; DELETE is how it says so.
                let _ = self.client.delete_with(&next, &self.headers);
                if self.cancel.is_cancelled() {
                    bail!("statement cancelled");
                }
                break;
            }
            if last_progress.elapsed() > budget {
                // Say so before walking away, or the coordinator keeps the
                // query running for a client that has stopped reading.
                let _ = self.client.delete_with(&next, &self.headers);
                self.in_flight.clear();
                bail!(
                    "statement returned no rows for {}s; raise the connection's \
                     query timeout in Settings -> Databases if it needs longer",
                    self.timeout_secs
                );
            }
            page = self
                .client
                .get_with(&next, &self.headers)
                .with_context(|| format!("reading results from '{}'", self.conn_label))?;
        }
        self.in_flight.clear();
        let mut table = DataTable::empty();
        table.columns = columns;
        table.rows = rows;
        Ok(table)
    }

    /// The first column of every row of `sql`, for the SHOW listings.
    fn names(&self, sql: &str) -> Result<Vec<String>> {
        let table = self.run(sql, crate::formats::initial_load_rows())?;
        Ok(table
            .rows
            .iter()
            .filter_map(|r| r.first().map(CellValue::to_string))
            .filter(|s| !s.is_empty())
            .collect())
    }

    /// `catalog` when given, else the connection's own. Trino cannot list a
    /// schema without one.
    fn catalog_or_default(&self, catalog: Option<&str>) -> Result<String> {
        let name = catalog
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .unwrap_or(self.default_catalog.as_str());
        if name.is_empty() {
            bail!(
                "Trino needs a catalog: pick one in the sidebar, or put a default \
                 in the Database field of connection '{}'",
                self.conn_label
            );
        }
        Ok(DbEngine::Trino.quote_ident(name))
    }
}

/// Trino reports a failed statement in the result body rather than by HTTP
/// status, so the message has to be dug out of it.
fn trino_error(v: &Value) -> Option<String> {
    let err = v.get("error")?;
    let message = err["message"].as_str().unwrap_or("query failed");
    match err["errorName"].as_str() {
        Some(name) => Some(format!("{name}: {message}")),
        None => Some(message.to_string()),
    }
}

/// Map a Trino type name to the Arrow-name strings Octa uses. Parameterised
/// types arrive spelled out (`varchar(10)`, `decimal(38,2)`, `timestamp(6)
/// with time zone`), so the match is on the prefix.
pub(crate) fn trino_type_to_arrow(ty: &str) -> &'static str {
    let t = ty.trim().to_ascii_lowercase();
    match t.as_str() {
        "tinyint" | "smallint" | "integer" | "int" | "bigint" => "Int64",
        "real" | "double" => "Float64",
        "boolean" => "Boolean",
        "date" => "Date32",
        _ if t.starts_with("decimal") => "Float64",
        // `time` sorts with the timestamps here only for the prefix test; it
        // has no date part, so it stays text.
        _ if t.starts_with("timestamp") => "Timestamp(Microsecond, None)",
        // varchar, char, varbinary, json, uuid, ipaddress, array(...),
        // map(...), row(...), and the time types.
        _ => "Utf8",
    }
}

/// One Trino cell. Values arrive as real JSON types, not strings, so a
/// number is already a number; the string fallbacks cover the types Trino
/// renders as text (decimals, for instance, to keep their precision).
fn trino_cell(v: &Value, arrow_type: &str) -> CellValue {
    if v.is_null() {
        return CellValue::Null;
    }
    match arrow_type {
        "Int64" => v
            .as_i64()
            .map(CellValue::Int)
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()).map(CellValue::Int))
            .unwrap_or_else(|| CellValue::String(v.to_string())),
        "Float64" => v
            .as_f64()
            .map(CellValue::Float)
            .or_else(|| {
                v.as_str()
                    .and_then(|s| s.parse().ok())
                    .map(CellValue::Float)
            })
            .unwrap_or_else(|| CellValue::String(v.to_string())),
        "Boolean" => v
            .as_bool()
            .map(CellValue::Bool)
            .unwrap_or_else(|| CellValue::String(v.to_string())),
        "Date32" => CellValue::Date(v.as_str().unwrap_or_default().to_string()),
        "Timestamp(Microsecond, None)" => {
            CellValue::DateTime(v.as_str().unwrap_or_default().to_string())
        }
        _ => match v.as_str() {
            Some(s) => CellValue::String(s.to_string()),
            // array(...), map(...) and row(...) arrive as JSON structures.
            None => CellValue::Nested(v.to_string()),
        },
    }
}

impl DbConnector for TrinoConnector {
    fn engine(&self) -> DbEngine {
        DbEngine::Trino
    }

    fn list_catalogs(&mut self) -> Result<Vec<String>> {
        self.names("SHOW CATALOGS")
    }

    fn list_schemas(&mut self, catalog: Option<&str>) -> Result<Vec<String>> {
        let catalog = self.catalog_or_default(catalog)?;
        self.names(&format!("SHOW SCHEMAS FROM {catalog}"))
    }

    fn list_tables(&mut self, catalog: Option<&str>, schema: &str) -> Result<Vec<String>> {
        let catalog = self.catalog_or_default(catalog)?;
        let schema = DbEngine::Trino.quote_ident(schema);
        self.names(&format!("SHOW TABLES FROM {catalog}.{schema}"))
    }

    fn query(&mut self, sql: &str) -> Result<DataTable> {
        self.run(sql, crate::formats::initial_load_rows())
    }

    fn execute(&mut self, sql: &str) -> Result<u64> {
        // Trino has no transactions to open per statement here: START
        // TRANSACTION binds to a session, and the statement API gives each
        // request its own unless one is negotiated. The shared write skeleton's
        // BEGIN / COMMIT are therefore no-ops, as on the warehouse engines.
        let head = sql.trim_start().to_ascii_uppercase();
        if head.starts_with("BEGIN") || head.starts_with("COMMIT") || head.starts_with("ROLLBACK") {
            return Ok(0);
        }
        let table = self.run(sql, 1)?;
        Ok(table.row_count() as u64)
    }

    fn write_table(
        &mut self,
        catalog: Option<&str>,
        schema: &str,
        table: &str,
        mode: DbWriteMode,
        data: &DataTable,
    ) -> Result<DbWriteReport> {
        let catalog = catalog
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_string)
            .or_else(|| Some(self.default_catalog.clone()).filter(|c| !c.is_empty()));
        super::write_table_generic(
            self,
            DbEngine::Trino,
            catalog.as_deref(),
            schema,
            table,
            mode,
            data,
        )
    }

    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        let flag = self.cancel.clone();
        Some(Box::new(move || flag.cancel()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_map_including_parameterised_ones() {
        assert_eq!(trino_type_to_arrow("bigint"), "Int64");
        assert_eq!(trino_type_to_arrow("varchar(10)"), "Utf8");
        assert_eq!(trino_type_to_arrow("decimal(38,2)"), "Float64");
        assert_eq!(
            trino_type_to_arrow("timestamp(6) with time zone"),
            "Timestamp(Microsecond, None)"
        );
        // A time has no date part, so it is not a timestamp.
        assert_eq!(trino_type_to_arrow("time(3)"), "Utf8");
        assert_eq!(trino_type_to_arrow("array(varchar)"), "Utf8");
    }

    /// Trino sends real JSON types, and a decimal as text so it keeps its
    /// precision on the wire: both have to land in the same bucket.
    #[test]
    fn cells_take_json_types_and_text_numbers() {
        assert_eq!(trino_cell(&Value::Null, "Int64"), CellValue::Null);
        assert_eq!(
            trino_cell(&serde_json::json!(42), "Int64"),
            CellValue::Int(42)
        );
        assert_eq!(
            trino_cell(&serde_json::json!("1.50"), "Float64"),
            CellValue::Float(1.5)
        );
        assert_eq!(
            trino_cell(&serde_json::json!(["a", "b"]), "Utf8"),
            CellValue::Nested("[\"a\",\"b\"]".into())
        );
    }

    /// A failed Trino statement comes back as HTTP 200 with an `error` object,
    /// so the body is the only place the failure shows.
    #[test]
    fn statement_error_is_read_from_the_body() {
        let v = serde_json::json!({
            "error": {"message": "line 1:8: Column 'nope' cannot be resolved",
                      "errorName": "COLUMN_NOT_FOUND"}
        });
        assert_eq!(
            trino_error(&v).unwrap(),
            "COLUMN_NOT_FOUND: line 1:8: Column 'nope' cannot be resolved"
        );
        assert!(trino_error(&serde_json::json!({"stats": {"state": "RUNNING"}})).is_none());
    }
}
