//! Account-level bucket/container enumeration. `object_store` only lists
//! *inside* a bucket, so listing buckets shells out to the provider CLI
//! (the same tools octa already relies on for credentials). When the CLI is
//! absent or denies the call, the caller falls back to manual bucket entry.

use anyhow::{Context, Result, bail};
use std::process::Command;

use super::{CloudConnection, CloudKind};

/// List buckets/containers for an account-level connection.
pub fn list_account_buckets(conn: &CloudConnection) -> Result<Vec<String>> {
    match conn.kind {
        CloudKind::S3 => list_s3(conn),
        CloudKind::AzureBlob => list_azure(conn),
        CloudKind::Gcs => list_gcs(conn),
    }
}

fn run(cmd: &mut Command, what: &str) -> Result<String> {
    let out = cmd
        .output()
        .with_context(|| format!("running {what} (is the CLI installed?)"))?;
    if !out.status.success() {
        bail!(
            "{what} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn list_s3(conn: &CloudConnection) -> Result<Vec<String>> {
    let mut cmd = Command::new("aws");
    cmd.args([
        "s3api",
        "list-buckets",
        "--query",
        "Buckets[].Name",
        "--output",
        "text",
    ]);
    if let Some(p) = &conn.profile {
        cmd.args(["--profile", p]);
    }
    if let Some(e) = &conn.endpoint
        && !e.is_empty()
    {
        cmd.args(["--endpoint-url", e]);
    }
    let out = run(&mut cmd, "aws s3api list-buckets")?;
    Ok(out.split_whitespace().map(|s| s.to_string()).collect())
}

fn list_azure(conn: &CloudConnection) -> Result<Vec<String>> {
    let account = conn
        .account
        .as_deref()
        .context("Azure account-level listing needs the storage account name")?;
    let mut cmd = Command::new("az");
    cmd.args([
        "storage",
        "container",
        "list",
        "--account-name",
        account,
        "--auth-mode",
        "login",
        "--query",
        "[].name",
        "-o",
        "tsv",
    ]);
    let out = run(&mut cmd, "az storage container list")?;
    Ok(out
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

fn list_gcs(conn: &CloudConnection) -> Result<Vec<String>> {
    let mut cmd = Command::new("gcloud");
    cmd.args(["storage", "buckets", "list", "--format=value(name)"]);
    // Buckets live under a project; without `--project` gcloud only lists the
    // active project. `account` (reused as the gcloud identity for GCS) picks
    // which logged-in account to list as.
    if let Some(p) = &conn.project {
        cmd.args(["--project", p]);
    }
    if let Some(a) = &conn.account {
        cmd.args(["--account", a]);
    }
    let out = run(&mut cmd, "gcloud storage buckets list")?;
    Ok(out
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

// ponytail: account-level bucket listing shells out to the provider CLI; if
// throughput/no-CLI ever matters, swap to the cloud SDK ListBuckets per provider.
