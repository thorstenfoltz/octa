//! BigQuery connector over the REST API (`jobs.query` +
//! datasets/tables listing). A small REST client submits a query, polls via
//! `getQueryResults` when the job is not immediately complete, and maps the
//! response into a [`DataTable`]. Bearer auth is a Google access token from
//! Application Default Credentials or a service-account key.
//!
//! The connection's `database` field carries the **GCP project id** (the REST
//! path is scoped by project; datasets are this connector's "schemas").
//! BigQuery has no enforced primary keys, so its tabs open read-only.
//!
//! Live-only: the parser ([`parse_bq_result`]) is unit-tested; the HTTP flow is
//! covered by the env-gated live test.

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::rest::{InFlight, RestClient, bigquery_cancel_path, poll};
use super::{CancelFlag, DbAuth, DbConnection, DbConnector, DbEngine, DbWriteMode, DbWriteReport};

// cloud-platform (not the narrower .../auth/bigquery) so the service-account
// token can also call cloudresourcemanager projects.list for list_catalogs.
const BQ_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

pub struct BigQueryConnector {
    client: RestClient,
    bearer: String,
    project: String,
    conn_label: String,
    cancel: CancelFlag,
    in_flight: InFlight,
}

impl BigQueryConnector {
    pub fn connect(conn: &DbConnection, _secret: Option<&str>) -> Result<Self> {
        let project = conn.database.trim().to_string();
        if project.is_empty() {
            bail!("BigQuery needs a GCP project id; put it in the connection's Database field");
        }
        let bearer = resolve_bearer(conn)?;
        Ok(Self {
            client: RestClient::new("https://bigquery.googleapis.com")
                .with_header("Accept", "application/json"),
            bearer,
            project,
            conn_label: conn.name.clone(),
            cancel: CancelFlag::new(),
            in_flight: InFlight::default(),
        })
    }

    /// Run a query and return the completed response JSON, polling
    /// `getQueryResults` while the job is not done.
    fn run_query(&self, sql: &str, max_results: usize) -> Result<Value> {
        self.in_flight.clear();
        let body = serde_json::json!({
            "query": sql,
            "useLegacySql": false,
            "maxResults": max_results,
            "timeoutMs": 30000,
        });
        let first = self
            .client
            .post_json(
                &format!("bigquery/v2/projects/{}/queries", self.project),
                &self.bearer,
                &body,
            )
            .with_context(|| format!("querying '{}'", self.conn_label))?;
        if first["jobComplete"].as_bool().unwrap_or(false) {
            return Ok(first);
        }
        let job_id = first["jobReference"]["jobId"]
            .as_str()
            .context("BigQuery did not return a job id")?
            .to_string();
        self.in_flight.set(&job_id);
        let path = format!(
            "bigquery/v2/projects/{}/queries/{job_id}?maxResults={max_results}&timeoutMs=30000",
            self.project
        );
        let cancel = self.cancel.clone();
        let result = poll(
            || self.client.get_json(&path, &self.bearer),
            |v| v["jobComplete"].as_bool().unwrap_or(false),
            |_| false, // a real error surfaces as a non-2xx from get_json
            move || cancel.is_cancelled(),
            60,
            std::time::Duration::from_millis(500),
        );
        // Clear on every exit path (success and error alike), so a stale job
        // id is never cancelled later.
        self.in_flight.clear();
        result
    }

    /// GET a listing endpoint and pull a nested id string from each element of
    /// the named array.
    fn list_ids(&self, path: &str, array: &str, id_ptr: &str) -> Result<Vec<String>> {
        let v = self.client.get_json(path, &self.bearer)?;
        Ok(v[array]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|it| it.pointer(id_ptr).and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default())
    }
}

impl DbConnector for BigQueryConnector {
    fn engine(&self) -> DbEngine {
        DbEngine::BigQuery
    }

    fn list_catalogs(&mut self) -> Result<Vec<String>> {
        // Projects live on the Cloud Resource Manager host; RestClient joins
        // paths onto one base, so use a throwaway client for that host.
        let rm = RestClient::new("https://cloudresourcemanager.googleapis.com")
            .with_header("Accept", "application/json");
        let v = rm.get_json("v1/projects", &self.bearer)?;
        let mut ids: Vec<String> = v["projects"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|it| it.pointer("/projectId").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        ids.sort();
        Ok(ids)
    }

    fn list_schemas(&mut self, catalog: Option<&str>) -> Result<Vec<String>> {
        let project = catalog.unwrap_or(&self.project);
        self.list_ids(
            &format!("bigquery/v2/projects/{project}/datasets"),
            "datasets",
            "/datasetReference/datasetId",
        )
    }

    fn list_tables(&mut self, catalog: Option<&str>, schema: &str) -> Result<Vec<String>> {
        let project = catalog.unwrap_or(&self.project);
        self.list_ids(
            &format!("bigquery/v2/projects/{project}/datasets/{schema}/tables"),
            "tables",
            "/tableReference/tableId",
        )
    }

    fn query(&mut self, sql: &str) -> Result<DataTable> {
        self.cancel.reset();
        let cap = crate::formats::initial_load_rows();
        let v = self.run_query(sql, cap)?;
        parse_bq_result(&v)
    }

    fn execute(&mut self, sql: &str) -> Result<u64> {
        self.cancel.reset();
        // Single-statement query jobs auto-commit; swallow transaction control
        // from the shared writer (non-atomic writes).
        let head = sql.trim_start();
        let upper = head[..head.len().min(9)].to_ascii_uppercase();
        if upper.starts_with("BEGIN")
            || upper.starts_with("COMMIT")
            || upper.starts_with("ROLLBACK")
        {
            return Ok(0);
        }
        let v = self.run_query(sql, 0)?;
        Ok(v["numDmlAffectedRows"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0))
    }

    fn write_table(
        &mut self,
        catalog: Option<&str>,
        schema: &str,
        table: &str,
        mode: DbWriteMode,
        data: &DataTable,
    ) -> Result<DbWriteReport> {
        // ponytail: literal-INSERT writer via query jobs; non-atomic. BigQuery
        // standard SQL DDL/DML, so all modes work, but tabs open read-only
        // (no enforced primary keys).
        super::write_table_generic(self, DbEngine::BigQuery, catalog, schema, table, mode, data)
    }

    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        let cancel = self.cancel.clone();
        let in_flight = self.in_flight.clone();
        let client = self.client.clone();
        let bearer = self.bearer.clone();
        let project = self.project.clone();
        Some(Box::new(move || {
            // Stop the client-side wait first: that always works, and the
            // vendor call below is best-effort.
            cancel.cancel();
            let Some(job_id) = in_flight.get() else {
                return;
            };
            let path = bigquery_cancel_path(&project, &job_id);
            // Best effort: the job may already have finished, and a failed
            // cancel must not surface as a query error.
            let _ = client.post_json(&path, &bearer, &serde_json::json!({}));
        }))
    }
}

/// Resolve a Google access token for the connection's auth mode.
fn resolve_bearer(conn: &DbConnection) -> Result<String> {
    match &conn.auth {
        DbAuth::GcpAdc => super::auth::gcp_adc_token(),
        DbAuth::GcpServiceAccount { key_path } => {
            let bytes = std::fs::read(key_path)
                .with_context(|| format!("reading the service-account key at {key_path}"))?;
            let key_json: Value =
                serde_json::from_slice(&bytes).context("parsing the service-account key JSON")?;
            super::auth::gcp_sa_token(&key_json, BQ_SCOPE)
        }
        other => bail!(
            "BigQuery needs Application Default Credentials or a service-account \
             key; got {:?}",
            other.kind()
        ),
    }
}

/// Map a BigQuery field type to an Arrow type-name string.
fn bq_type_to_arrow(ty: &str) -> &'static str {
    match ty.to_ascii_uppercase().as_str() {
        "INTEGER" | "INT64" => "Int64",
        "FLOAT" | "FLOAT64" | "NUMERIC" | "BIGNUMERIC" => "Float64",
        "BOOL" | "BOOLEAN" => "Boolean",
        "DATE" => "Date32",
        "TIMESTAMP" | "DATETIME" => "Timestamp(Microsecond, None)",
        _ => "Utf8", // STRING, BYTES, TIME, GEOGRAPHY, RECORD, JSON, ...
    }
}

/// Parse a BigQuery `jobs.query` response (`schema.fields` + `rows[].f[].v`)
/// into a [`DataTable`]. Scalar cells arrive as JSON strings under `v`.
pub(crate) fn parse_bq_result(v: &Value) -> Result<DataTable> {
    let fields = v["schema"]["fields"]
        .as_array()
        .context("BigQuery response missing schema.fields")?;
    let columns: Vec<ColumnInfo> = fields
        .iter()
        .map(|f| ColumnInfo {
            name: f["name"].as_str().unwrap_or("").to_string(),
            data_type: bq_type_to_arrow(f["type"].as_str().unwrap_or("STRING")).to_string(),
        })
        .collect();
    let mut table = DataTable::empty();
    table.rows = v["rows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    let cells = row["f"].as_array();
                    columns
                        .iter()
                        .enumerate()
                        .map(|(i, col)| {
                            let cell = cells
                                .and_then(|c| c.get(i))
                                .map(|o| &o["v"])
                                .unwrap_or(&Value::Null);
                            bq_cell(cell, &col.data_type)
                        })
                        .collect()
                })
                .collect()
        })
        .unwrap_or_default();
    table.columns = columns;
    Ok(table)
}

/// Convert one BigQuery `v` cell by its Arrow type. Nested RECORD/REPEATED
/// values (JSON objects/arrays) fall back to their JSON text.
fn bq_cell(v: &Value, arrow_type: &str) -> CellValue {
    if v.is_null() {
        return CellValue::Null;
    }
    let Some(s) = v.as_str() else {
        return CellValue::Nested(v.to_string());
    };
    let s = s.to_string();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bigquery_query_response() {
        let v = serde_json::json!({
            "schema": { "fields": [
                {"name":"id","type":"INTEGER"},
                {"name":"name","type":"STRING"} ] },
            "rows": [
                {"f":[{"v":"1"},{"v":"alice"}]},
                {"f":[{"v":"2"},{"v":"bob"}]} ]
        });
        let t = parse_bq_result(&v).unwrap();
        let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["id", "name"]);
        assert_eq!(t.columns[0].data_type, "Int64");
        assert_eq!(t.row_count(), 2);
        assert_eq!(t.rows[0][0], CellValue::Int(1));
        assert_eq!(t.rows[1][1], CellValue::String("bob".into()));
    }

    #[test]
    fn bq_types_and_null_cell() {
        assert_eq!(bq_type_to_arrow("INT64"), "Int64");
        assert_eq!(bq_type_to_arrow("NUMERIC"), "Float64");
        assert_eq!(
            bq_type_to_arrow("TIMESTAMP"),
            "Timestamp(Microsecond, None)"
        );
        assert_eq!(bq_cell(&Value::Null, "Int64"), CellValue::Null);
    }
}
