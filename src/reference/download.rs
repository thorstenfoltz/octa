//! Downloads for reference data: one plain GET with Octa's version in the
//! User-Agent and the stored validators, so an unchanged file costs nothing.
//! Tests point everything at a stand-in server through [`BASE_ENV`].

use std::time::Duration;

use anyhow::{Context, anyhow};

/// Test-only: replaces scheme and host of every reference address.
pub const BASE_ENV: &str = "OCTA_REFERENCE_BASE_URL";
/// The largest download accepted (the biggest real one is 55 MB).
pub const LIMIT: u64 = 200 * 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Validators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

pub enum Fetched {
    NotModified,
    Body {
        file: tempfile::NamedTempFile,
        validators: Validators,
    },
}

pub fn resolve_with(url: &str, base: Option<&str>) -> String {
    match base.map(str::trim).filter(|b| !b.is_empty()) {
        Some(base) => {
            let path = url.splitn(4, '/').nth(3).unwrap_or("");
            format!("{}/{path}", base.trim_end_matches('/'))
        }
        None => url.to_string(),
    }
}

pub fn resolve(url: &str) -> String {
    resolve_with(url, std::env::var(BASE_ENV).ok().as_deref())
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_global(Some(Duration::from_secs(1800)))
        .build()
        .into()
}

fn user_agent() -> String {
    format!("octa/{}", env!("CARGO_PKG_VERSION"))
}

pub fn fetch(url: &str, known: &Validators) -> anyhow::Result<Fetched> {
    let mut req = agent().get(url).header("User-Agent", user_agent().as_str());
    if let Some(e) = &known.etag {
        req = req.header("If-None-Match", e.as_str());
    }
    if let Some(m) = &known.last_modified {
        req = req.header("If-Modified-Since", m.as_str());
    }
    let mut resp = req.call().map_err(|e| anyhow!("{url}: {e}"))?;
    if resp.status().as_u16() == 304 {
        return Ok(Fetched::NotModified);
    }
    let header = |name: &str| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let validators = Validators {
        etag: header("etag"),
        last_modified: header("last-modified"),
    };
    let mut file = tempfile::NamedTempFile::new().context("creating a temporary file")?;
    let mut body = resp.body_mut().with_config().limit(LIMIT).reader();
    std::io::copy(&mut body, &mut file).with_context(|| format!("reading {url}"))?;
    Ok(Fetched::Body { file, validators })
}

pub fn fetch_text(url: &str) -> anyhow::Result<String> {
    agent()
        .get(url)
        .header("User-Agent", user_agent().as_str())
        .call()
        .map_err(|e| anyhow!("{url}: {e}"))?
        .body_mut()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_string()
        .with_context(|| format!("reading {url}"))
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
