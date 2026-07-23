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

/// Read a response body as JSON, turning a non-2xx status into an error whose
/// message is extracted from the (still-JSON, usually) error body.
fn read_json_checked(resp: &mut ureq::http::Response<ureq::Body>) -> Result<Value> {
    let status = resp.status();
    let text = resp.body_mut().read_to_string()?;
    let json: Value = serde_json::from_str(&text).unwrap_or(Value::String(text.clone()));
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
/// `.error.message`, `.error`, else the value stringified.
pub fn rest_error_message(v: &Value) -> String {
    if let Some(s) = v.get("message").and_then(Value::as_str) {
        return s.to_string();
    }
    if let Some(s) = v.pointer("/error/message").and_then(Value::as_str) {
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
    Err(anyhow!("statement did not finish after {max_tries} polls"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
