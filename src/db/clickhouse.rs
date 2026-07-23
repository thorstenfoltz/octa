//! ClickHouse connector over the HTTP interface (no native TCP client crate,
//! so this is a small hand-rolled `ureq` client). SQL is POSTed verbatim;
//! SELECTs ask for `FORMAT JSONCompact` and parse the `meta`/`data` envelope.
//!
//! ClickHouse has no classic multi-statement transactions, so `execute`
//! swallows `BEGIN`/`COMMIT`/`ROLLBACK` as no-ops (this lets the shared
//! `write_table_generic` drive INSERTs; writes are therefore non-atomic).

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::{DbConnection, DbConnector, DbEngine, DbWriteMode, DbWriteReport};

/// The in-flight statement's query id, shared between `post` and the cancel
/// closure spawned by `cancel_handle`. A local copy of the shape in
/// `rest.rs`'s `InFlight`: this connector is a hand-rolled `ureq` client, not
/// a `RestClient` user, so it does not pull in that module for one small type.
#[derive(Clone, Default)]
struct InFlight(std::sync::Arc<std::sync::Mutex<Option<String>>>);

impl InFlight {
    fn set(&self, id: &str) {
        *self.0.lock().unwrap() = Some(id.to_string());
    }

    fn clear(&self) {
        *self.0.lock().unwrap() = None;
    }

    fn get(&self) -> Option<String> {
        self.0.lock().unwrap().clone()
    }
}

pub struct ClickHouseConnector {
    agent: ureq::Agent,
    base_url: String,
    user: String,
    key: String,
    database: String,
    conn_label: String,
    in_flight: InFlight,
}

impl ClickHouseConnector {
    /// Build the HTTP client. `stored` is the keyring secret (password); the
    /// URL scheme is HTTPS on the TLS ports (443 / 8443), HTTP otherwise.
    pub fn connect(conn: &DbConnection, stored: Option<&str>) -> Result<Self> {
        let key = super::auth::resolve_password(conn, stored).unwrap_or_default();
        // ponytail: scheme picked from the port; add an explicit TLS toggle to
        // the connection if a non-standard secure port shows up.
        let scheme = if matches!(conn.port, 443 | 8443) {
            "https"
        } else {
            "http"
        };
        let base_url = format!("{scheme}://{}:{}/", conn.host, conn.port);
        Ok(Self {
            agent: ureq::Agent::config_builder()
                .http_status_as_error(false)
                .build()
                .into(),
            base_url,
            user: conn.username.clone(),
            key,
            database: conn.database.clone(),
            conn_label: conn.name.clone(),
            in_flight: InFlight::default(),
        })
    }

    /// POST `sql` to the HTTP endpoint and return the response body text,
    /// erroring on any non-2xx (ClickHouse puts the message in the body).
    /// Tags the request with a fresh query id so `cancel_handle` can `KILL
    /// QUERY` it from another connection while this one is blocked waiting
    /// on the response.
    fn post(&self, sql: &str) -> Result<String> {
        let id = new_query_id();
        self.in_flight.set(&id);
        let url = format!("{}?query_id={id}", self.base_url);
        let result = post_statement(
            &self.agent,
            &url,
            &self.user,
            &self.key,
            &self.database,
            sql,
        )
        .with_context(|| format!("posting to ClickHouse '{}'", self.conn_label));
        self.in_flight.clear();
        result
    }
}

/// POST one SQL statement given an agent and the connection pieces a request
/// needs. A free function (not a connector method) so the cancel closure
/// spawned by `cancel_handle` can call it after cloning what it needs: the
/// running query already holds `&mut self`, so nothing in the closure can
/// borrow the connector.
fn post_statement(
    agent: &ureq::Agent,
    url: &str,
    user: &str,
    key: &str,
    database: &str,
    sql: &str,
) -> Result<String> {
    let mut resp = agent
        .post(url)
        .header("X-ClickHouse-User", user)
        .header("X-ClickHouse-Key", key)
        .header("X-ClickHouse-Database", database)
        .send(sql)
        .context("posting to ClickHouse")?;
    let status = resp.status();
    let body = resp
        .body_mut()
        .read_to_string()
        .context("reading ClickHouse response")?;
    if !status.is_success() {
        bail!("ClickHouse HTTP {}: {}", status.as_u16(), body.trim());
    }
    Ok(body)
}

/// A per-statement id, sent as ClickHouse's `query_id` URL parameter so a
/// later `KILL QUERY` can target exactly this statement. Unique per
/// connection, which is all `KILL QUERY` needs; no new dependency for a
/// value nothing else ever reads.
fn new_query_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("octa-{nanos:x}")
}

/// `KILL QUERY` for one query id. The id is ours (a generated timestamp)
/// rather than user input, but it is still quoted properly: a SQL string
/// built by concatenation should never depend on its input being safe.
fn kill_query_sql(query_id: &str) -> String {
    format!(
        "KILL QUERY WHERE query_id = '{}'",
        query_id.replace('\'', "''")
    )
}

impl DbConnector for ClickHouseConnector {
    fn engine(&self) -> DbEngine {
        DbEngine::ClickHouse
    }

    fn list_schemas(&mut self, _catalog: Option<&str>) -> Result<Vec<String>> {
        let t = self.query("SELECT name FROM system.databases ORDER BY name")?;
        Ok(t.rows
            .iter()
            .filter_map(|r| r.first())
            .map(cell_text)
            .collect())
    }

    fn list_tables(&mut self, _catalog: Option<&str>, schema: &str) -> Result<Vec<String>> {
        let esc = schema.replace('\'', "''");
        let t = self.query(&format!(
            "SELECT name FROM system.tables WHERE database = '{esc}' ORDER BY name"
        ))?;
        Ok(t.rows
            .iter()
            .filter_map(|r| r.first())
            .map(cell_text)
            .collect())
    }

    fn query(&mut self, sql: &str) -> Result<DataTable> {
        // Cap the result at the initial-load row limit by wrapping the query;
        // FORMAT must be the very last clause.
        let cap = crate::formats::initial_load_rows();
        let inner = sql.trim().trim_end_matches(';');
        let wrapped = format!("SELECT * FROM ({inner}) AS _octa_q LIMIT {cap}\nFORMAT JSONCompact");
        let body = self.post(&wrapped)?;
        parse_jsoncompact(&body)
    }

    fn execute(&mut self, sql: &str) -> Result<u64> {
        // ClickHouse HTTP has no real transactions and does not report rows
        // affected; swallow transaction control and return 0.
        let head = sql.trim_start();
        if head.len() >= 5 {
            let kw = head[..head.len().min(8)].to_ascii_uppercase();
            if kw.starts_with("BEGIN") || kw.starts_with("COMMIT") || kw.starts_with("ROLLBACK") {
                return Ok(0);
            }
        }
        self.post(sql)?;
        Ok(0)
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
        // Replace emit generic DDL without a ClickHouse ENGINE clause and will
        // error server-side until a ClickHouse DDL dialect lands.
        super::reject_catalog(self.engine(), catalog)?;
        super::write_table_generic(self, DbEngine::ClickHouse, None, schema, table, mode, data)
    }

    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        let in_flight = self.in_flight.clone();
        let agent = self.agent.clone();
        let base_url = self.base_url.clone();
        let user = self.user.clone();
        let key = self.key.clone();
        let database = self.database.clone();
        Some(Box::new(move || {
            let Some(id) = in_flight.get() else {
                return;
            };
            // Best effort on a fresh request: the connector itself is
            // mutably borrowed by the running query.
            let _ = post_statement(
                &agent,
                &base_url,
                &user,
                &key,
                &database,
                &kill_query_sql(&id),
            );
        }))
    }
}

/// Read one cell as plain text (for the single-column catalogue queries).
fn cell_text(c: &CellValue) -> String {
    match c {
        CellValue::String(s)
        | CellValue::Date(s)
        | CellValue::DateTime(s)
        | CellValue::Nested(s) => s.clone(),
        CellValue::Int(i) => i.to_string(),
        CellValue::Float(f) => f.to_string(),
        CellValue::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// Map a ClickHouse column type to the Arrow type-name strings the rest of
/// Octa uses. `Nullable(...)`/`LowCardinality(...)` wrappers are stripped.
pub(crate) fn ch_type_to_arrow(ty: &str) -> &'static str {
    let mut t = ty.trim();
    while let Some(inner) = t
        .strip_prefix("Nullable(")
        .or_else(|| t.strip_prefix("LowCardinality("))
    {
        t = inner.trim_end_matches(')').trim();
    }
    if t.starts_with("UInt") || t.starts_with("Int") {
        "Int64"
    } else if t.starts_with("Float") || t.starts_with("Decimal") {
        "Float64"
    } else if t == "Bool" {
        "Boolean"
    } else if t.starts_with("DateTime") {
        "Timestamp(Microsecond, None)"
    } else if t.starts_with("Date") {
        "Date32"
    } else {
        "Utf8"
    }
}

/// Parse a ClickHouse `FORMAT JSONCompact` envelope (`meta` + `data`) into a
/// [`DataTable`].
pub(crate) fn parse_jsoncompact(body: &str) -> Result<DataTable> {
    let v: Value = serde_json::from_str(body).context("parsing ClickHouse JSONCompact")?;
    let meta = v["meta"]
        .as_array()
        .context("ClickHouse response missing `meta`")?;
    let columns: Vec<ColumnInfo> = meta
        .iter()
        .map(|m| ColumnInfo {
            name: m["name"].as_str().unwrap_or("").to_string(),
            data_type: ch_type_to_arrow(m["type"].as_str().unwrap_or("")).to_string(),
        })
        .collect();
    let mut table = DataTable::empty();
    table.rows = v["data"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    columns
                        .iter()
                        .enumerate()
                        .map(|(i, col)| {
                            json_to_cell(row.get(i).unwrap_or(&Value::Null), &col.data_type)
                        })
                        .collect()
                })
                .collect()
        })
        .unwrap_or_default();
    table.columns = columns;
    Ok(table)
}

/// Convert one JSON value to a [`CellValue`] guided by the column's Arrow type.
/// ClickHouse emits 64-bit integers as JSON strings (JS precision), so both
/// number and string forms are accepted.
fn json_to_cell(v: &Value, arrow_type: &str) -> CellValue {
    if v.is_null() {
        return CellValue::Null;
    }
    match arrow_type {
        "Int64" => v
            .as_i64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            .map(CellValue::Int)
            .unwrap_or_else(|| CellValue::String(value_text(v))),
        "Float64" => v
            .as_f64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            .map(CellValue::Float)
            .unwrap_or_else(|| CellValue::String(value_text(v))),
        "Boolean" => match v.as_bool() {
            Some(b) => CellValue::Bool(b),
            None => CellValue::String(value_text(v)),
        },
        "Date32" => CellValue::Date(value_text(v)),
        "Timestamp(Microsecond, None)" => CellValue::DateTime(value_text(v)),
        _ => CellValue::String(value_text(v)),
    }
}

/// Text form of a JSON scalar (bare string for strings, JSON for the rest).
fn value_text(v: &Value) -> String {
    v.as_str()
        .map(str::to_string)
        .unwrap_or_else(|| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_jsoncompact_names_types_rows() {
        let body = r#"{
          "meta":[{"name":"id","type":"UInt32"},{"name":"name","type":"String"}],
          "data":[[1,"a"],[2,"b"]],
          "rows":2
        }"#;
        let t = parse_jsoncompact(body).unwrap();
        let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["id", "name"]);
        assert_eq!(t.columns[0].data_type, "Int64"); // UInt32 -> Int64
        assert_eq!(t.row_count(), 2);
        assert_eq!(t.rows[0][0], CellValue::Int(1));
        assert_eq!(t.rows[1][1], CellValue::String("b".into()));
    }

    #[test]
    fn ch_types_map_and_strip_wrappers() {
        assert_eq!(ch_type_to_arrow("Nullable(Int64)"), "Int64");
        assert_eq!(ch_type_to_arrow("LowCardinality(String)"), "Utf8");
        assert_eq!(ch_type_to_arrow("Float32"), "Float64");
        assert_eq!(
            ch_type_to_arrow("DateTime64(3)"),
            "Timestamp(Microsecond, None)"
        );
        assert_eq!(ch_type_to_arrow("Date"), "Date32");
    }

    #[test]
    fn int64_as_string_is_parsed() {
        // ClickHouse serialises 64-bit ints as strings.
        let body = r#"{"meta":[{"name":"big","type":"UInt64"}],"data":[["9223372036854775807"]],"rows":1}"#;
        let t = parse_jsoncompact(body).unwrap();
        assert_eq!(t.rows[0][0], CellValue::Int(i64::MAX));
    }

    #[test]
    fn kill_query_sql_escapes_the_id() {
        assert_eq!(
            kill_query_sql("abc-123"),
            "KILL QUERY WHERE query_id = 'abc-123'"
        );
        // A quote in the id must not break out of the literal.
        assert_eq!(kill_query_sql("a'b"), "KILL QUERY WHERE query_id = 'a''b'");
    }
}
