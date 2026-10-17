//! Unit tests for [`bigquery`](super). No BigQuery project was available: the
//! fixture follows Google's documented `jobs.query` dry-run response.

use super::*;

#[test]
fn a_dry_run_reports_the_bytes_it_would_process() {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/bigquery_dry_run.json")).unwrap();
    assert_eq!(bytes_from_dry_run(&v).unwrap(), 3_435_973_837);
}

#[test]
fn a_response_without_a_byte_count_is_an_error() {
    assert!(bytes_from_dry_run(&serde_json::json!({"jobComplete": true})).is_err());
}
