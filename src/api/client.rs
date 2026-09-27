//! Fetching an endpoint, one page at a time.
//!
//! Written against `ureq` directly rather than through [`crate::db::rest`]:
//! that client hardwires bearer auth, discards response headers (so
//! `Link`-header paging is invisible) and carries poll/cancel machinery for
//! async statement APIs that a REST source has no use for. Its two genuinely
//! reusable pieces - the body cap and the error extractor - are imported.
//!
//! The page loop copies the habits every database connector here already has:
//! bound the loop with a constant, stop at the row cap, and check the cancel
//! flag between pages.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::api::records;
use crate::api::{ApiAuth, ApiConnection, ApiPaging, same_origin};
use crate::data::DataTable;
use crate::db::rest::{read_body_capped, rest_error_message};

/// Hard ceiling on pages per fetch. A server that keeps handing back a cursor
/// must not be able to spin a worker thread forever.
pub const MAX_PAGES: usize = 10_000;

/// What to fetch.
#[derive(Debug, Clone, Default)]
pub struct FetchOptions {
    /// Path under the connection's base URL. `None` uses the saved one.
    pub path: Option<String>,
    /// Stop once this many rows are in hand. `None` uses the loader's cap.
    pub max_rows: Option<usize>,
    /// Stop after this many pages. `None` means [`MAX_PAGES`].
    pub max_pages: Option<usize>,
}

/// What came back.
pub struct FetchOutcome {
    pub table: DataTable,
    pub pages: usize,
    /// True when the loop stopped on a limit rather than on the last page, so
    /// the caller can say so instead of implying it read everything.
    pub capped: bool,
}

fn agent(timeout_secs: u32) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        // Redirects are refused, not followed: an endpoint that bounces us
        // somewhere else is a URL the user should fix in Settings, and
        // following it silently would leave the origin they approved.
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(timeout_secs.max(1) as u64)))
        .build()
        .into()
}

/// A list of `name = value` pairs: headers, or query parameters.
type Pairs = Vec<(String, String)>;

/// Headers and query parameters this connection's auth adds.
fn auth_parts(conn: &ApiConnection, secret: Option<&str>) -> (Pairs, Pairs) {
    let mut headers = Vec::new();
    let mut query = Vec::new();
    let s = secret.unwrap_or_default();
    match &conn.auth {
        ApiAuth::None => {}
        ApiAuth::Bearer => {
            headers.push(("Authorization".to_string(), format!("Bearer {s}")));
        }
        ApiAuth::HeaderKey { name } => {
            headers.push((name.clone(), s.to_string()));
        }
        ApiAuth::QueryKey { name } => {
            query.push((name.clone(), s.to_string()));
        }
        ApiAuth::Basic { username } => {
            use base64::Engine;
            let token = base64::engine::general_purpose::STANDARD.encode(format!("{username}:{s}"));
            headers.push(("Authorization".to_string(), format!("Basic {token}")));
        }
    }
    (headers, query)
}

fn with_query(url: &str, params: &[(String, String)]) -> String {
    if params.is_empty() {
        return url.to_string();
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    let q: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect();
    format!("{url}{sep}{}", q.join("&"))
}

/// Percent-encode everything that is not unreserved. Small enough to own; the
/// alternative is a dependency for one function.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// One request. Returns the decoded body plus the `Link` header, which is the
/// only response header any pagination mode here needs.
fn get_json(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(String, String)],
) -> Result<(Value, Option<String>)> {
    let mut req = agent.get(url);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let mut resp = req.call().with_context(|| format!("requesting {url}"))?;
    let status = resp.status().as_u16();
    let link = resp
        .headers()
        .get("link")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if (300..400).contains(&status) {
        let to = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("somewhere else");
        anyhow::bail!(
            "{url} redirected to {to} (HTTP {status}). Redirects are not followed, because the \
             host you saved is the host we talk to. Put the final address in the connection's \
             base URL."
        );
    }

    let body = read_body_capped(&mut resp)?;
    let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        let msg = if value.is_null() {
            body.chars().take(400).collect::<String>()
        } else {
            rest_error_message(&value)
        };
        anyhow::bail!("{url} returned HTTP {status}: {msg}");
    }
    if value.is_null() && !body.trim().is_empty() {
        anyhow::bail!(
            "{url} did not return JSON. The first bytes were: {}",
            body.chars().take(200).collect::<String>()
        );
    }
    Ok((value, link))
}

/// The `rel="next"` target of an RFC 8288 `Link` header, if there is one.
pub fn next_from_link(header: &str) -> Option<String> {
    for part in header.split(',') {
        let part = part.trim();
        let (url_part, params) = part.split_once('>')?;
        let url = url_part.trim_start().strip_prefix('<')?;
        if params.split(';').any(|p| {
            let p = p.trim().replace(' ', "");
            p.eq_ignore_ascii_case("rel=\"next\"") || p.eq_ignore_ascii_case("rel=next")
        }) {
            return Some(url.trim().to_string());
        }
    }
    None
}

/// Fetch the first page only, undecoded, for the dialog's preview and the
/// Test button. Nothing is turned into a table: the caller wants the envelope
/// so it can offer the records-array picker.
pub fn fetch_first_page(
    conn: &ApiConnection,
    secret: Option<&str>,
    path: Option<&str>,
) -> Result<Value> {
    let (headers, query) = auth_parts(conn, secret);
    let mut all_headers = conn.headers.clone();
    all_headers.extend(headers);
    let url = with_query(&conn.url_for(path), &query);
    let (value, _) = get_json(&agent(conn.timeout_secs), &url, &all_headers)?;
    Ok(value)
}

/// Fetch every page the pagination rule leads to, and build one table.
pub fn fetch_table(
    conn: &ApiConnection,
    secret: Option<&str>,
    opts: &FetchOptions,
    cancel: &Arc<AtomicBool>,
) -> Result<FetchOutcome> {
    let (auth_headers, auth_query) = auth_parts(conn, secret);
    let mut headers = conn.headers.clone();
    headers.extend(auth_headers);

    let agent = agent(conn.timeout_secs);
    let base = conn.url_for(opts.path.as_deref());
    let max_pages = opts.max_pages.unwrap_or(MAX_PAGES).clamp(1, MAX_PAGES);
    let max_rows = opts
        .max_rows
        .unwrap_or_else(crate::formats::initial_load_rows);

    let mut rows: Vec<Value> = Vec::new();
    let mut pages = 0usize;
    let mut capped = false;
    // `None` until the first page decides it, for the modes that follow a URL
    // the server handed back.
    let mut next_url: Option<String> = None;
    let mut index: u32 = 0;

    while pages < max_pages {
        if cancel.load(Ordering::Relaxed) {
            capped = true;
            break;
        }

        let url = match (&conn.paging, &next_url) {
            (_, Some(u)) => u.clone(),
            (ApiPaging::None, None) => with_query(&base, &auth_query),
            (ApiPaging::PageNumber { param, start }, None) => {
                let mut q = auth_query.clone();
                q.push((param.clone(), (start + index).to_string()));
                if let Some(n) = conn.page_size {
                    q.push(("per_page".to_string(), n.to_string()));
                }
                with_query(&base, &q)
            }
            (
                ApiPaging::OffsetLimit {
                    offset_param,
                    limit_param,
                },
                None,
            ) => {
                let limit = conn.page_size.unwrap_or(100);
                let mut q = auth_query.clone();
                q.push((offset_param.clone(), (index * limit).to_string()));
                q.push((limit_param.clone(), limit.to_string()));
                with_query(&base, &q)
            }
            (ApiPaging::Cursor { .. }, None) | (ApiPaging::LinkHeader, None) => {
                with_query(&base, &auth_query)
            }
        };

        // Every URL after the first came from the server. It may not leave the
        // origin the user saved.
        if pages > 0 && !same_origin(&base, &url) {
            anyhow::bail!(
                "the endpoint's next-page link points at {url}, which is not the host this \
                 connection was saved for. Stopping rather than following it."
            );
        }

        let (body, link) = get_json(&agent, &url, &headers)?;
        pages += 1;

        let page = records::records_at(&body, &conn.records_pointer).ok_or_else(|| {
            anyhow::anyhow!(
                "no value at \"{}\" in the response from {url}. \
                 Leave the records path empty to let Octa find the rows itself.",
                conn.records_pointer
            )
        })?;
        let got = records::row_count(&page);
        records::extend(&mut rows, page);

        if rows.len() >= max_rows {
            rows.truncate(max_rows);
            capped = true;
            break;
        }

        // Where the next page is, or that there is none.
        next_url = None;
        index += 1;
        match &conn.paging {
            ApiPaging::None => break,
            ApiPaging::PageNumber { .. } => {
                if got == 0 {
                    break;
                }
            }
            ApiPaging::OffsetLimit { .. } => {
                let limit = conn.page_size.unwrap_or(100) as usize;
                // A short page is the last page; a full one might not be.
                if got < limit {
                    break;
                }
            }
            ApiPaging::Cursor { pointer, param } => {
                let cursor = records::records_at(&body, pointer).and_then(|v| match v {
                    Value::String(s) if !s.trim().is_empty() => Some(s),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                });
                match cursor {
                    Some(c) => {
                        let mut q = auth_query.clone();
                        q.push((param.clone(), c));
                        next_url = Some(with_query(&base, &q));
                    }
                    None => break,
                }
            }
            ApiPaging::LinkHeader => match link.as_deref().and_then(next_from_link) {
                Some(u) => next_url = Some(u),
                None => break,
            },
        }
    }

    if pages >= max_pages {
        capped = true;
    }

    let label = conn.tab_label(opts.path.as_deref());
    let table = crate::formats::json_reader::json_to_table(
        Value::Array(rows),
        std::path::Path::new(&label),
        "JSON API",
    )?;
    Ok(FetchOutcome {
        table,
        pages,
        capped,
    })
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
