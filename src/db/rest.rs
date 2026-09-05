//! Shared REST substrate for the cloud-warehouse connectors (Snowflake,
//! Databricks, BigQuery). A thin `ureq` JSON client with bearer auth, a
//! uniform error-message extractor, and a bounded [`poll`] loop for the async
//! statement APIs.

use std::time::Duration;

use anyhow::{Result, anyhow};
use serde_json::Value;

#[derive(Clone)]
pub struct RestClient {
    agent: ureq::Agent,
    base_url: String,
    /// Headers sent on every request (e.g. a vendor auth-token-type marker).
    default_headers: Vec<(String, String)>,
}

impl RestClient {
    /// A client rooted at `base_url` (trailing slash trimmed).
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Self {
            agent: ureq::Agent::config_builder()
                .http_status_as_error(false)
                .build()
                .into(),
            base_url,
            default_headers: Vec::new(),
        }
    }

    /// Add a header sent on every request from this client (builder-style).
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.default_headers.push((name.into(), value.into()));
        self
    }

    fn url(&self, path: &str) -> String {
        if path.starts_with("http") {
            path.to_string()
        } else {
            format!("{}/{}", self.base_url, path.trim_start_matches('/'))
        }
    }

    /// POST a JSON body with a bearer token; parse the JSON response, mapping a
    /// non-2xx to an error carrying the server's message.
    pub fn post_json(&self, path: &str, bearer: &str, body: &Value) -> Result<Value> {
        let mut req = self
            .agent
            .post(self.url(path))
            .header("Authorization", &format!("Bearer {bearer}"))
            .header("Content-Type", "application/json");
        for (k, v) in &self.default_headers {
            req = req.header(k, v);
        }
        let mut resp = req.send_json(body)?;
        read_json_checked(&mut resp)
    }

    /// GET a JSON response with a bearer token.
    pub fn get_json(&self, path: &str, bearer: &str) -> Result<Value> {
        let mut req = self
            .agent
            .get(self.url(path))
            .header("Authorization", &format!("Bearer {bearer}"));
        for (k, v) in &self.default_headers {
            req = req.header(k, v);
        }
        let mut resp = req.call()?;
        read_json_checked(&mut resp)
    }

    /// DELETE a resource with a bearer token; parse the JSON response.
    pub fn delete_json(&self, path: &str, bearer: &str) -> Result<Value> {
        let mut req = self
            .agent
            .delete(self.url(path))
            .header("Authorization", &format!("Bearer {bearer}"));
        for (k, v) in &self.default_headers {
            req = req.header(k, v);
        }
        let mut resp = req.call()?;
        read_json_checked(&mut resp)
    }
    /// POST a body with caller-supplied headers, no bearer of its own.
    ///
    /// The two engines that need this do not fit `post_json`: Trino's
    /// statement endpoint takes the SQL as a plain-text body with
    /// `X-Trino-*` headers, and Athena's takes a JSON body whose signature
    /// covers the exact bytes, so neither can have headers added for it.
    pub fn post_raw(
        &self,
        path: &str,
        body: &[u8],
        content_type: &str,
        headers: &[(String, String)],
    ) -> Result<Value> {
        let mut req = self
            .agent
            .post(self.url(path))
            .header("Content-Type", content_type);
        for (k, v) in self.default_headers.iter().chain(headers) {
            req = req.header(k, v);
        }
        let mut resp = req.send(body)?;
        read_json_checked(&mut resp)
    }

    /// GET with caller-supplied headers. `path` may be an absolute URL, which
    /// is how Trino hands out the next page of a result.
    pub fn get_with(&self, path: &str, headers: &[(String, String)]) -> Result<Value> {
        let mut req = self.agent.get(self.url(path));
        for (k, v) in self.default_headers.iter().chain(headers) {
            req = req.header(k, v);
        }
        let mut resp = req.call()?;
        read_json_checked(&mut resp)
    }

    /// DELETE with caller-supplied headers, ignoring the body. Trino cancels
    /// a running statement this way.
    pub fn delete_with(&self, path: &str, headers: &[(String, String)]) -> Result<()> {
        let mut req = self.agent.delete(self.url(path));
        for (k, v) in self.default_headers.iter().chain(headers) {
            req = req.header(k, v);
        }
        req.call()?;
        Ok(())
    }
}

/// How long [`poll`] waits between attempts. Shared so `poll_tries` and its
/// callers cannot drift apart on the arithmetic.
pub const POLL_DELAY: Duration = Duration::from_millis(500);

/// Poll attempts that fit a `timeout_secs` budget, after subtracting the
/// `server_wait_secs` the server already spent holding the submit request
/// open before it answered.
///
/// The connection's timeout is wall-clock seconds, which is the thing a user
/// can reason about; [`poll`] counts attempts, so this is the conversion.
/// Never returns zero: a poll that never asks would report "did not finish"
/// about a statement it had not looked at.
pub fn poll_tries(timeout_secs: u32, server_wait_secs: u32) -> usize {
    let remaining = u64::from(timeout_secs.saturating_sub(server_wait_secs));
    ((remaining * 1000) / POLL_DELAY.as_millis() as u64).max(1) as usize
}

/// Snowflake SQL API statement cancel.
pub fn snowflake_cancel_path(handle: &str) -> String {
    format!("api/v2/statements/{handle}/cancel")
}

/// Databricks Statement Execution API cancel (an HTTP DELETE on the
/// statement resource).
pub fn databricks_cancel_path(statement_id: &str) -> String {
    format!("api/2.0/sql/statements/{statement_id}")
}

/// BigQuery jobs.cancel.
pub fn bigquery_cancel_path(project: &str, job_id: &str) -> String {
    format!("bigquery/v2/projects/{project}/jobs/{job_id}/cancel")
}

/// The in-flight statement's identity, shared between a connector's `query`
/// and its `cancel_handle` closure. The closure runs on another thread while
/// `query` holds `&mut self`, so it cannot reach a plain field through the
/// connector.
#[derive(Clone, Default)]
pub struct InFlight(std::sync::Arc<std::sync::Mutex<Option<String>>>);

impl InFlight {
    pub fn set(&self, id: &str) {
        *self.0.lock().unwrap() = Some(id.to_string());
    }

    pub fn clear(&self) {
        *self.0.lock().unwrap() = None;
    }

    pub fn get(&self) -> Option<String> {
        self.0.lock().unwrap().clone()
    }
}

/// Largest response body accepted from a warehouse.
///
/// `ureq::Body::read_to_string` defaults to **10 MB**, which is a sensible
/// ceiling for an API that returns settings and far too small for one that
/// returns rows: a Databricks result chunk, a Snowflake partition, a BigQuery
/// page, a Trino page and a whole ClickHouse result set all carry bulk data
/// and sail past it, failing with
/// "the response body is larger than request limit: 10485760" once a table is
/// big enough to be worth paging.
///
/// 256 MB is an order of magnitude past anything these vendors actually send
/// in one response - they chunk results themselves, and Databricks caps an
/// inline result at 25 MiB - so it never rejects a real answer, while still
/// stopping a runaway or hostile response from quietly exhausting memory.
///
/// Deliberately a constant and not a setting. The only engine whose response
/// size a user controls is ClickHouse, which returns the whole result set in
/// one body, and the number that controls it is already exposed: Settings ->
/// Performance -> Live database page size. A byte cap would be a second knob
/// for the same thing and the more dangerous one - raising it past what the
/// machine can hold turns a clean error into an out-of-memory kill, because a
/// JSON body of this size parses into a `Value` tree several times larger.
pub(crate) const MAX_RESPONSE_BYTES: u64 = 256 * 1024 * 1024;

/// Read a response body under [`MAX_RESPONSE_BYTES`].
///
/// `lossy_utf8` is set explicitly because `with_config()` drops what plain
/// `read_to_string()` applies implicitly, so only the ceiling changes.
pub(crate) fn read_body_capped(resp: &mut ureq::http::Response<ureq::Body>) -> Result<String> {
    resp.body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .lossy_utf8(true)
        .read_to_string()
        .map_err(body_read_error)
}

/// ureq's own words for an over-sized body are a byte count and nothing to do
/// about it. Every engine reads through one place, so one arm here replaces
/// that with the setting that actually governs it.
fn body_read_error(e: ureq::Error) -> anyhow::Error {
    match e {
        ureq::Error::BodyExceedsLimit(_) => anyhow!(
            "the server sent more than {} MB in a single response. Lower \
             Settings -> Performance -> Live database page size and open the \
             table again.",
            MAX_RESPONSE_BYTES / (1024 * 1024)
        ),
        other => anyhow::Error::new(other),
    }
}

/// Read a response body as JSON, turning a non-2xx status into an error whose
/// message is extracted from the (still-JSON, usually) error body.
fn read_json_checked(resp: &mut ureq::http::Response<ureq::Body>) -> Result<Value> {
    let status = resp.status();
    let text = read_body_capped(resp)?;
    // A match, not `unwrap_or(Value::String(text.clone()))`: `unwrap_or` takes
    // its fallback eagerly, so that spelling cloned the whole body on every
    // response including the ones that parsed. Harmless at 10 MB, wasteful at
    // the ceiling above. The fallback keeps the raw text because a non-JSON
    // body is exactly the case worth showing: cloud storage answers a refused
    // download in XML, a proxy answers in HTML.
    let json: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => Value::String(text),
    };
    if status.is_success() {
        Ok(json)
    } else {
        Err(anyhow!(
            "HTTP {}: {}",
            status.as_u16(),
            rest_error_message(&json)
        ))
    }
}

/// Best-effort human message out of a warehouse error body: `.message`,
/// `.error.message`, `.status.error.message`, `.error`, else the value
/// stringified.
///
/// The `status.error.message` arm exists because a Databricks statement that
/// FAILS answers 200 with the failure nested under `status`, so none of the
/// other arms matched and the whole response was stringified into the status
/// bar. `poll` reports a failed state through this function, so every
/// Databricks failure read as a JSON blob, not just the one that was reported.
pub fn rest_error_message(v: &Value) -> String {
    if let Some(s) = v.get("message").and_then(Value::as_str) {
        return s.to_string();
    }
    if let Some(s) = v.pointer("/error/message").and_then(Value::as_str) {
        return s.to_string();
    }
    if let Some(s) = v.pointer("/status/error/message").and_then(Value::as_str) {
        return s.to_string();
    }
    if let Some(err) = v.get("error") {
        if let Some(s) = err.as_str() {
            return s.to_string();
        }
        return err.to_string();
    }
    v.as_str()
        .map(str::to_string)
        .unwrap_or_else(|| v.to_string())
}

/// Poll `fetch` until `is_done` (return the value), `is_error` (fail with the
/// body's message), `is_aborted` (fail as cancelled), or `max_tries` is
/// exhausted. Sleeps `delay` between tries.
pub fn poll<F, D, E, A>(
    mut fetch: F,
    is_done: D,
    is_error: E,
    is_aborted: A,
    max_tries: usize,
    delay: Duration,
) -> Result<Value>
where
    F: FnMut() -> Result<Value>,
    D: Fn(&Value) -> bool,
    E: Fn(&Value) -> bool,
    A: Fn() -> bool,
{
    for attempt in 0..max_tries {
        if is_aborted() {
            return Err(anyhow!("statement cancelled"));
        }
        let v = fetch()?;
        if is_error(&v) {
            return Err(anyhow!(rest_error_message(&v)));
        }
        if is_done(&v) {
            return Ok(v);
        }
        if attempt + 1 < max_tries && !delay.is_zero() {
            std::thread::sleep(delay);
            if is_aborted() {
                return Err(anyhow!("statement cancelled"));
            }
        }
    }
    // Reported in seconds, not attempts: seconds are what the connection's
    // query timeout is set in, so the message names the number to raise.
    let budget = max_tries as f64 * delay.as_secs_f64();
    Err(anyhow!(
        "statement did not finish within {budget:.0}s; raise the connection's \
         query timeout in Settings -> Databases if it needs longer"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The over-sized-body error is the one a user can act on, so it must name
    /// the setting rather than repeat ureq's byte count. Everything else keeps
    /// ureq's own words.
    #[test]
    fn oversized_body_error_names_the_page_size_setting() {
        let msg = body_read_error(ureq::Error::BodyExceedsLimit(10 * 1024 * 1024)).to_string();
        assert!(msg.contains("256 MB"), "{msg}");
        assert!(msg.contains("Live database page size"), "{msg}");

        let other = body_read_error(ureq::Error::HostNotFound).to_string();
        assert_eq!(other, "host not found");
    }

    #[test]
    fn extract_error_message_from_body() {
        let v = serde_json::json!({"message":"bad token","code":"390144"});
        assert_eq!(rest_error_message(&v), "bad token");
    }

    #[test]
    fn extract_nested_error_message() {
        let v = serde_json::json!({"error":{"message":"nope","status":401}});
        assert_eq!(rest_error_message(&v), "nope");
    }

    /// A Databricks statement failure, exactly as the API returns it. Without
    /// the `status.error.message` arm this stringified the entire response
    /// into the status bar.
    #[test]
    fn extract_databricks_statement_error_message() {
        let v = serde_json::json!({
            "statement_id": "01f1a783-5e31-18b9-9daa-c4aa79442a40",
            "status": {
                "state": "FAILED",
                "error": {
                    "error_code": "BAD_REQUEST",
                    "message": "Inline byte limit exceeded."
                }
            }
        });
        assert_eq!(rest_error_message(&v), "Inline byte limit exceeded.");
    }

    #[test]
    fn poll_stops_on_success() {
        let states = std::cell::Cell::new(0);
        let out = poll(
            || {
                let s = states.get();
                states.set(s + 1);
                Ok(serde_json::json!({"status": if s < 2 {"RUNNING"} else {"SUCCEEDED"}}))
            },
            |v| v["status"] == "SUCCEEDED",
            |v| v["status"] == "FAILED",
            || false,
            3,
            Duration::from_millis(0),
        )
        .unwrap();
        assert_eq!(out["status"], "SUCCEEDED");
    }

    #[test]
    fn poll_fails_on_error_state() {
        let out = poll(
            || Ok(serde_json::json!({"status":"FAILED","message":"boom"})),
            |v| v["status"] == "SUCCEEDED",
            |v| v["status"] == "FAILED",
            || false,
            3,
            Duration::from_millis(0),
        );
        assert_eq!(out.unwrap_err().to_string(), "boom");
    }

    /// The budget is stated in seconds because that is the unit the
    /// connection's setting uses, and the message has to name the knob.
    #[test]
    fn poll_tries_convert_a_second_budget_into_attempts() {
        // 60s total, 30s of it already spent server-side: 30s of polling at
        // 500ms is 60 attempts, which is what the connectors did before the
        // timeout was configurable.
        assert_eq!(poll_tries(60, 30), 60);
        assert_eq!(poll_tries(60, 0), 120);
        // A budget entirely eaten by the server wait still asks once.
        assert_eq!(poll_tries(30, 30), 1);
        assert_eq!(poll_tries(1, 99), 1);
    }

    #[test]
    fn poll_exhausts() {
        let out = poll(
            || Ok(serde_json::json!({"status":"RUNNING"})),
            |v| v["status"] == "SUCCEEDED",
            |_| false,
            || false,
            2,
            Duration::from_millis(0),
        );
        assert!(out.unwrap_err().to_string().contains("did not finish"));
    }

    #[test]
    fn cancel_paths_match_each_vendor_api() {
        assert_eq!(
            snowflake_cancel_path("01b2-c3d4"),
            "api/v2/statements/01b2-c3d4/cancel"
        );
        assert_eq!(
            databricks_cancel_path("01ef-9a"),
            "api/2.0/sql/statements/01ef-9a"
        );
        assert_eq!(
            bigquery_cancel_path("my-proj", "job_123"),
            "bigquery/v2/projects/my-proj/jobs/job_123/cancel"
        );
    }

    #[test]
    fn poll_stops_when_aborted() {
        let calls = std::cell::Cell::new(0);
        let out = poll(
            || {
                calls.set(calls.get() + 1);
                Ok(serde_json::json!({ "done": false }))
            },
            |v| v["done"].as_bool().unwrap_or(false),
            |_| false,
            || calls.get() >= 2,
            100,
            Duration::ZERO,
        );
        let err = out.expect_err("an aborted poll must be an error");
        assert!(
            err.to_string().contains("cancelled"),
            "message should name the cancellation, got: {err}"
        );
        assert_eq!(
            calls.get(),
            2,
            "poll must stop as soon as abort reports true"
        );
    }
}
