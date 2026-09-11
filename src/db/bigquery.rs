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

use super::rest::{InFlight, POLL_DELAY, RestClient, bigquery_cancel_path, poll, poll_tries};
use super::{CancelFlag, DbAuth, DbConnection, DbConnector, DbEngine, DbWriteMode, DbWriteReport};

// cloud-platform (not the narrower .../auth/bigquery) so the service-account
// token can also call cloudresourcemanager projects.list for list_catalogs.
const BQ_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

/// Ceiling on `getQueryResults` page follows, so a misbehaving server cannot
/// spin the loader thread. See [`BigQueryConnector::read_pages`].
const MAX_RESULT_PAGES: usize = 100_000;

/// How long `jobs.query` blocks before answering with an incomplete job.
const BQ_WAIT_SECS: u32 = 30;

pub struct BigQueryConnector {
    client: RestClient,
    bearer: String,
    project: String,
    conn_label: String,
    /// Seconds to keep asking BigQuery whether the job is done.
    timeout_secs: u32,
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
            timeout_secs: conn.query_timeout_secs,
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
            // A uint32 page size, and the caller may pass the "Unlimited"
            // sentinel (`usize::MAX`); anything past u32 the API refuses.
            // Paging is handled by `read_pages`, so clamping only bounds the
            // first page.
            "maxResults": max_results.min(u32::MAX as usize),
            "timeoutMs": BQ_WAIT_SECS * 1000,
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
        let wait_ms = BQ_WAIT_SECS * 1000;
        let path = format!(
            "bigquery/v2/projects/{}/queries/{job_id}?maxResults={max_results}&timeoutMs={wait_ms}",
            self.project
        );
        let cancel = self.cancel.clone();
        let result = poll(
            || self.client.get_json(&path, &self.bearer),
            |v| v["jobComplete"].as_bool().unwrap_or(false),
            |_| false, // a real error surfaces as a non-2xx from get_json
            move || cancel.is_cancelled(),
            // `timeoutMs` already spent BQ_WAIT_SECS of the budget waiting.
            poll_tries(self.timeout_secs, BQ_WAIT_SECS),
            POLL_DELAY,
        );
        // Clear on every exit path (success and error alike), so a stale job
        // id is never cancelled later.
        self.in_flight.clear();
        result
    }

    /// Append the result pages after the first, stopping at `cap` rows.
    ///
    /// BigQuery caps one response at roughly 10 MB whatever `maxResults` asks
    /// for, and hands back a `pageToken` for the rest. Without following it a
    /// large table arrived short with nothing said about the missing rows.
    fn read_pages(&self, first: &Value, table: &mut DataTable, cap: usize) -> Result<()> {
        let Some(job_id) = first.pointer("/jobReference/jobId").and_then(Value::as_str) else {
            return Ok(());
        };
        let columns = table.columns.clone();
        // Bounded rather than `while there is a token`: a server that handed
        // back a token it already used, or empty pages with fresh tokens,
        // would otherwise spin this worker forever. The bound is far past any
        // result the row cap allows, so it never cuts a real page walk short.
        let mut token = bq_page_token(first);
        for _ in 0..MAX_RESULT_PAGES {
            let Some(t) = token else {
                return Ok(());
            };
            if table.rows.len() >= cap {
                return Ok(());
            }
            if self.cancel.is_cancelled() {
                bail!("query cancelled");
            }
            let v = self
                .client
                .get_json(&bq_page_path(&self.project, job_id, cap, &t), &self.bearer)
                .with_context(|| {
                    format!("fetching the next result page on '{}'", self.conn_label)
                })?;
            append_bq_rows(&v["rows"], &columns, &mut table.rows);
            token = bq_page_token(&v);
        }
        Ok(())
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
        let mut t = parse_bq_result(&v)?;
        self.read_pages(&v, &mut t, cap)?;
        if t.rows.len() > cap {
            t.rows.truncate(cap);
        }
        Ok(t)
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
    append_bq_rows(&v["rows"], &columns, &mut table.rows);
    table.columns = columns;
    Ok(table)
}

/// Decode one page's `rows` block onto `out`. The first page arrives inside
/// the query response, the rest as their own `getQueryResults` GETs, and both
/// are this same `{"f":[{"v":...}]}` shape.
fn append_bq_rows(v: &Value, columns: &[ColumnInfo], out: &mut Vec<Vec<CellValue>>) {
    let Some(rows) = v.as_array() else {
        return;
    };
    out.reserve(rows.len());
    for row in rows {
        let cells = row["f"].as_array();
        out.push(
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
                .collect(),
        );
    }
}

/// The `pageToken` for the page after this one, if there is one.
fn bq_page_token(v: &Value) -> Option<String> {
    v["pageToken"]
        .as_str()
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// The `getQueryResults` path for one further page of a finished job.
fn bq_page_path(project: &str, job_id: &str, max_results: usize, token: &str) -> String {
    format!(
        "bigquery/v2/projects/{project}/queries/{job_id}\
         ?maxResults={max_results}&pageToken={}",
        query_escape(token)
    )
}

/// Percent-encode a query-string value. A page token is opaque, so everything
/// outside the unreserved set is escaped rather than trusted: a `+` in a
/// token would otherwise be read back as a space and the paging would stall
/// or repeat, which is exactly the failure this walk exists to prevent.
fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
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

    /// The token is what tells the walk there is more; an absent or empty one
    /// is the end of the result, not a page to fetch.
    #[test]
    fn page_token_marks_the_end_of_a_result() {
        assert_eq!(
            bq_page_token(&serde_json::json!({"pageToken": "abc"})),
            Some("abc".to_string())
        );
        assert_eq!(bq_page_token(&serde_json::json!({"pageToken": ""})), None);
        assert_eq!(bq_page_token(&serde_json::json!({})), None);
    }

    #[test]
    fn page_path_carries_an_escaped_token() {
        assert_eq!(
            bq_page_path("proj", "job_1", 100, "a+b/c=="),
            "bigquery/v2/projects/proj/queries/job_1?maxResults=100&pageToken=a%2Bb%2Fc%3D%3D"
        );
    }

    #[test]
    fn a_page_body_appends_to_the_rows_in_hand() {
        let columns = vec![ColumnInfo {
            name: "id".to_string(),
            data_type: "Int64".to_string(),
        }];
        let mut rows = vec![vec![CellValue::Int(1)]];
        append_bq_rows(
            &serde_json::json!([{"f":[{"v":"2"}]}, {"f":[{"v":null}]}]),
            &columns,
            &mut rows,
        );
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1][0], CellValue::Int(2));
        assert_eq!(rows[2][0], CellValue::Null);
    }

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
