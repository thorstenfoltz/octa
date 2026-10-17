//! BigQuery has no plan before a query runs. Its dry run answers one thing,
//! the bytes the query would read, which is what decides its cost; Octa shows
//! the bytes and never a price (rates differ by contract and region).

use anyhow::{Context, Result};
use serde_json::Value;

/// `totalBytesProcessed` of a `jobs.query` dry-run response. The API sends
/// the int64 as a string.
pub fn bytes_from_dry_run(v: &Value) -> Result<u64> {
    let raw = &v["totalBytesProcessed"];
    raw.as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| raw.as_u64())
        .context("BigQuery's dry run did not say how many bytes it would read")
}

#[cfg(test)]
#[path = "bigquery_tests.rs"]
mod tests;
