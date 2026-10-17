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
