//! Databricks connector over the Statement Execution API
//! (`/api/2.0/sql/statements`). Like the Snowflake connector: a small REST
//! client that submits a statement, polls while it runs, and maps the JSON
//! result into a [`DataTable`]. Bearer auth is a PAT, an Azure AD token, or an
//! OAuth M2M token.
//!
//! The connection's `database` field carries the **SQL warehouse id** (the
//! Statement API targets a warehouse, for which the connection model has no
//! dedicated field).
//!
//! Live-only: the parser ([`parse_dbx_result`]) is unit-tested; the HTTP flow
//! is covered by the env-gated live test.

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::rest::{InFlight, RestClient, databricks_cancel_path, poll};
use super::{CancelFlag, DbAuth, DbConnection, DbConnector, DbEngine, DbWriteMode, DbWriteReport};

pub struct DatabricksConnector {
    client: RestClient,
    bearer: String,
    warehouse_id: String,
    conn_label: String,
    cancel: CancelFlag,
    in_flight: InFlight,
}

impl DatabricksConnector {
    pub fn connect(conn: &DbConnection, secret: Option<&str>) -> Result<Self> {
        let host = conn.host.trim().trim_end_matches('/');
        let base = if host.starts_with("http") {
            host.to_string()
        } else {
            format!("https://{host}")
        };
        let warehouse_id = conn.database.trim().to_string();
        if warehouse_id.is_empty() {
            bail!(
                "Databricks needs a SQL warehouse id; put it in the connection's \
                 Database field"
            );
        }
        let bearer = resolve_bearer(conn, secret)?;
        Ok(Self {
            client: RestClient::new(base).with_header("Accept", "application/json"),
            bearer,
            warehouse_id,
            conn_label: conn.name.clone(),
            cancel: CancelFlag::new(),
            in_flight: InFlight::default(),
        })
    }

    /// Submit a statement and return the SUCCEEDED response JSON, polling while
    /// the warehouse runs it.
    fn submit(&self, sql: &str) -> Result<Value> {
        self.in_flight.clear();
        let body = serde_json::json!({
            "statement": sql,
            "warehouse_id": self.warehouse_id,
            "wait_timeout": "30s",
            "on_wait_timeout": "CONTINUE",
            "disposition": "INLINE",
            "format": "JSON_ARRAY",
        });
        let first = self
            .client
            .post_json("api/2.0/sql/statements", &self.bearer, &body)
            .with_context(|| format!("submitting statement on '{}'", self.conn_label))?;
        if dbx_state(&first) == "SUCCEEDED" {
            return Ok(first);
        }
        let id = first["statement_id"]
            .as_str()
            .context("Databricks did not return a statement_id")?
            .to_string();
        self.in_flight.set(&id);
        let path = format!("api/2.0/sql/statements/{id}");
        let cancel = self.cancel.clone();
        let result = poll(
            || self.client.get_json(&path, &self.bearer),
            |v| dbx_state(v) == "SUCCEEDED",
            |v| matches!(dbx_state(v), "FAILED" | "CANCELED" | "CLOSED"),
            move || cancel.is_cancelled(),
            60,
            std::time::Duration::from_millis(500),
        );
        // Clear on every exit path (success and error alike), so a stale
        // statement id is never cancelled later.
        self.in_flight.clear();
        result
    }

    /// Run a `SHOW ...` and pull the values of the first column matching one of
    /// `candidates` (case-insensitive), else the last column.
    fn show_column(&self, sql: &str, candidates: &[&str]) -> Result<Vec<String>> {
        let t = parse_dbx_result(&self.submit(sql)?)?;
        if t.columns.is_empty() {
            return Ok(Vec::new());
        }
        let col = t
            .columns
            .iter()
            .position(|c| candidates.iter().any(|w| c.name.eq_ignore_ascii_case(w)))
            .unwrap_or(t.columns.len() - 1);
        Ok(t.rows
            .iter()
            .filter_map(|r| r.get(col))
            .map(cell_text)
            .collect())
    }
}

impl DbConnector for DatabricksConnector {
    fn engine(&self) -> DbEngine {
        DbEngine::Databricks
    }

    fn list_catalogs(&mut self) -> Result<Vec<String>> {
        self.show_column("SHOW CATALOGS", &["catalog", "catalog_name"])
    }

    fn list_schemas(&mut self, catalog: Option<&str>) -> Result<Vec<String>> {
        let sql = match catalog {
            Some(c) => format!("SHOW SCHEMAS IN `{}`", c.replace('`', "``")),
            None => "SHOW SCHEMAS".to_string(),
        };
        self.show_column(&sql, &["databaseName", "schema_name", "namespace"])
    }

    fn list_tables(&mut self, catalog: Option<&str>, schema: &str) -> Result<Vec<String>> {
        let sch = schema.replace('`', "``");
        let sql = match catalog {
            Some(c) => format!("SHOW TABLES IN `{}`.`{sch}`", c.replace('`', "``")),
            None => format!("SHOW TABLES IN `{sch}`"),
        };
        self.show_column(&sql, &["tableName", "table_name", "name"])
    }

    fn query(&mut self, sql: &str) -> Result<DataTable> {
        self.cancel.reset();
        let mut t = parse_dbx_result(&self.submit(sql)?)?;
        let cap = crate::formats::initial_load_rows();
        if t.rows.len() > cap {
            t.rows.truncate(cap);
        }
        Ok(t)
    }

    fn execute(&mut self, sql: &str) -> Result<u64> {
        self.cancel.reset();
        // The Statement API runs one statement per call (no shared transaction),
        // so swallow BEGIN/COMMIT/ROLLBACK from the shared writer (non-atomic).
        let upper = sql.trim_start();
        let head = upper[..upper.len().min(9)].to_ascii_uppercase();
        if head.starts_with("BEGIN") || head.starts_with("COMMIT") || head.starts_with("ROLLBACK") {
            return Ok(0);
        }
        self.submit(sql)?;
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
        // ponytail: literal-INSERT writer; non-atomic on the stateless API.
        // Standard Spark SQL DDL/DML (backtick idents), so all modes work.
        super::write_table_generic(
            self,
            DbEngine::Databricks,
            catalog,
            schema,
            table,
            mode,
            data,
        )
    }

    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        let cancel = self.cancel.clone();
        let in_flight = self.in_flight.clone();
        let client = self.client.clone();
        let bearer = self.bearer.clone();
        Some(Box::new(move || {
            // Stop the client-side wait first: that always works, and the
            // vendor call below is best-effort.
            cancel.cancel();
            let Some(id) = in_flight.get() else {
                return;
            };
            let path = databricks_cancel_path(&id);
            // Best effort: the statement may already have finished, and a
            // failed cancel must not surface as a query error.
            let _ = client.delete_json(&path, &bearer);
        }))
    }
}

/// The statement `status.state` string (`""` when absent).
fn dbx_state(v: &Value) -> &str {
    v["status"]["state"].as_str().unwrap_or("")
}

/// Resolve the bearer token for the connection's auth mode.
fn resolve_bearer(conn: &DbConnection, secret: Option<&str>) -> Result<String> {
    match &conn.auth {
        // Personal access token: the stored secret is the bearer.
        DbAuth::Token => secret
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .context("Databricks personal access token (secret) is not set"),
        // Azure AD token via the az CLI (Databricks resource).
        DbAuth::AzureAd => super::auth::resolve_password(conn, secret),
        DbAuth::OAuthClientCredentials {
            client_id,
            token_url,
        } => {
            let url = token_url
                .as_deref()
                .filter(|u| !u.is_empty())
                .context("Databricks OAuth needs a token_url")?;
            let secret = secret.context("Databricks OAuth needs a client secret")?;
            let tok = super::auth::oauth_client_credentials_token(url, client_id, secret, None)?;
            Ok(tok.access_token)
        }
        // User-to-machine browser OAuth. A token cached by the Settings
        // sign-in flow wins; otherwise open the browser now (this runs on a
        // worker thread) and cache the result.
        DbAuth::OAuthBrowser => {
            if let Some(t) = super::auth::cached_browser_token(&conn.id) {
                return Ok(t.access_token);
            }
            let cfg = super::auth::browser_oauth_config(conn, None)
                .context("Databricks browser sign-in needs a workspace host")?;
            let tok = crate::auth::oauth_browser::acquire_token(
                &cfg,
                crate::auth::oauth_browser::open_url_in_browser,
            )?;
            super::auth::cache_browser_token(&conn.id, tok.clone());
            Ok(tok.access_token)
        }
        other => bail!(
            "Databricks needs a personal access token, Azure AD, or OAuth M2M; \
             got {:?}",
            other.kind()
        ),
    }
}

/// Map a Databricks (Spark SQL) type name to an Arrow type-name string.
fn dbx_type_to_arrow(ty: &str) -> &'static str {
    // type_name is the base type (e.g. "DECIMAL"); parameters live elsewhere.
    match ty.to_ascii_uppercase().as_str() {
        "INT" | "INTEGER" | "BIGINT" | "LONG" | "SMALLINT" | "SHORT" | "TINYINT" | "BYTE" => {
            "Int64"
        }
        "DOUBLE" | "FLOAT" | "REAL" | "DECIMAL" => "Float64",
        "BOOLEAN" => "Boolean",
        "DATE" => "Date32",
        t if t.starts_with("TIMESTAMP") => "Timestamp(Microsecond, None)",
        _ => "Utf8", // STRING, BINARY, ARRAY, MAP, STRUCT, VARIANT, ...
    }
}

/// Parse a Databricks Statement result (`manifest.schema.columns` +
/// `result.data_array`) into a [`DataTable`]. Cells arrive as JSON strings.
pub(crate) fn parse_dbx_result(v: &Value) -> Result<DataTable> {
    let cols = v["manifest"]["schema"]["columns"]
        .as_array()
        .context("Databricks result missing manifest.schema.columns")?;
    let columns: Vec<ColumnInfo> = cols
        .iter()
        .map(|c| ColumnInfo {
            name: c["name"].as_str().unwrap_or("").to_string(),
            data_type: dbx_type_to_arrow(c["type_name"].as_str().unwrap_or("STRING")).to_string(),
        })
        .collect();
    let mut table = DataTable::empty();
    table.rows = v["result"]["data_array"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    columns
                        .iter()
                        .enumerate()
                        .map(|(i, col)| {
                            dbx_cell(row.get(i).unwrap_or(&Value::Null), &col.data_type)
                        })
                        .collect()
                })
                .collect()
        })
        .unwrap_or_default();
    table.columns = columns;
    Ok(table)
}

/// Convert one Databricks cell (JSON string or null) by its Arrow type.
fn dbx_cell(v: &Value, arrow_type: &str) -> CellValue {
    if v.is_null() {
        return CellValue::Null;
    }
    let s = v.as_str().unwrap_or("").to_string();
    match arrow_type {
        "Int64" => s
            .parse::<i64>()
            .map(CellValue::Int)
            .unwrap_or(CellValue::String(s)),
        "Float64" => s
            .parse::<f64>()
            .map(CellValue::Float)
            .unwrap_or(CellValue::String(s)),
        "Boolean" => CellValue::Bool(matches!(s.as_str(), "true" | "1" | "TRUE")),
        "Date32" => CellValue::Date(s),
        "Timestamp(Microsecond, None)" => CellValue::DateTime(s),
        _ => CellValue::String(s),
    }
}

/// Plain text of a cell (for `SHOW` name extraction).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_databricks_result() {
        let v = serde_json::json!({
            "manifest": { "schema": { "columns": [
                {"name":"id","type_name":"INT"},
                {"name":"name","type_name":"STRING"} ] } },
            "result": { "data_array": [ ["1","alice"], ["2","bob"] ] }
        });
        let t = parse_dbx_result(&v).unwrap();
        let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["id", "name"]);
        assert_eq!(t.columns[0].data_type, "Int64");
        assert_eq!(t.row_count(), 2);
        assert_eq!(t.rows[0][0], CellValue::Int(1));
    }

    #[test]
    fn dbx_types_map() {
        assert_eq!(dbx_type_to_arrow("BIGINT"), "Int64");
        assert_eq!(dbx_type_to_arrow("decimal"), "Float64");
        assert_eq!(dbx_type_to_arrow("BOOLEAN"), "Boolean");
        assert_eq!(
            dbx_type_to_arrow("TIMESTAMP_NTZ"),
            "Timestamp(Microsecond, None)"
        );
        assert_eq!(dbx_type_to_arrow("STRUCT"), "Utf8");
    }
}
