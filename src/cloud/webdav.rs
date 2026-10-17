//! WebDAV (Nextcloud, ownCloud, Synology / QNAP boxes, Hetzner Storage Box)
//! as a cloud storage type, on `object_store`'s `HttpStore` (PROPFIND, GET,
//! PUT, DELETE, COPY, MKCOL).
//!
//! The store is rooted at `scheme://host[:port]/` and keys are full server
//! paths, so a connection's `prefix` (the server address's path, e.g.
//! `remote.php/dav/files/anna/`) scopes the tree exactly like an S3 prefix and
//! `davs://host/path` URLs need no translation.

use std::io::{Seek, Write};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result, bail};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use super::provider::walk_recursive;
use super::{CloudProvider, CloudUpload, ObjectEntry, ObjectStoreProvider};

/// Characters left alone in a path segment: RFC 3986 unreserved.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Split a server address into the store root (`https://host[:port]/`), the
/// authority (`host[:port]`, a connection's `bucket`) and the path under it
/// (a connection's `prefix`, ending in `/`, empty for the root). `davs://`
/// and `dav://` are accepted as spellings of `https://` and `http://`.
pub fn split_server_url(url: &str, allow_http: bool) -> Result<(String, String, String)> {
    let url = url.trim();
    let (scheme, rest) = url
        .split_once("://")
        .with_context(|| format!("{url} needs https:// in front"))?;
    let secure = match scheme.to_ascii_lowercase().as_str() {
        "https" | "davs" => true,
        "http" | "dav" if allow_http => false,
        "http" | "dav" => {
            bail!("{url} is plain HTTP; tick \"Allow HTTP\" on this connection to use it anyway")
        }
        other => bail!("{other}:// is not a WebDAV address; use https://"),
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    if authority.is_empty() {
        bail!("{url} has no host name");
    }
    let path = path.trim_matches('/');
    let prefix = if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    };
    let root = format!("{}://{authority}/", if secure { "https" } else { "http" });
    Ok((root, authority.to_string(), prefix))
}

fn basic_auth(user: &str, password: &str) -> String {
    use base64::Engine;
    let raw = format!("{user}:{password}");
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(raw)
    )
}

/// `root` + `key`, each path segment percent-encoded.
fn url_for(root: &str, key: &str) -> String {
    let path: Vec<String> = key
        .split('/')
        .map(|seg| utf8_percent_encode(seg, SEGMENT).to_string())
        .collect();
    format!("{root}{}", path.join("/"))
}

/// A [`CloudProvider`] over one WebDAV server.
pub struct WebDavProvider {
    inner: ObjectStoreProvider,
    root: String,
    auth: Option<String>,
    agent: ureq::Agent,
}

/// Build the provider for a server address. `user` + `password` become HTTP
/// Basic auth on every request; without both the requests go anonymous.
pub fn build_webdav_provider(
    endpoint: &str,
    allow_http: bool,
    user: Option<&str>,
    password: Option<&str>,
) -> Result<WebDavProvider> {
    let (root, _, _) = split_server_url(endpoint, allow_http)?;
    let auth = match (user.filter(|u| !u.is_empty()), password) {
        (Some(u), Some(p)) => Some(basic_auth(u, p)),
        _ => None,
    };
    let mut headers = object_store::HeaderMap::new();
    if let Some(a) = &auth {
        let mut value = object_store::HeaderValue::from_str(a)
            .context("the user name or password holds characters HTTP cannot carry")?;
        // Keeps the credentials out of any debug print of the request.
        value.set_sensitive(true);
        headers.insert("authorization", value);
    }
    let options = object_store::ClientOptions::new()
        .with_allow_http(allow_http)
        .with_default_headers(headers);
    let store = object_store::http::HttpBuilder::new()
        .with_url(root.clone())
        .with_client_options(options)
        .build()
        .with_context(|| format!("building the WebDAV client for {root}"))?;
    Ok(WebDavProvider {
        inner: ObjectStoreProvider::new(Arc::new(store)),
        root,
        auth,
        agent: ureq::Agent::config_builder()
            .http_status_as_error(false)
            // MKCOL is WebDAV's, not HTTP's: ureq refuses it otherwise.
            .allow_non_standard_methods(true)
            .build()
            .into(),
    })
}

/// The provider for a saved WebDAV connection.
pub fn build_webdav_provider_for(
    conn: &super::CloudConnection,
    creds: &super::ProviderCreds,
) -> Result<WebDavProvider> {
    let endpoint = conn
        .endpoint
        .as_deref()
        .filter(|e| !e.trim().is_empty())
        .context("this WebDAV connection has no server address")?;
    let password = match creds {
        super::ProviderCreds::Password(p) => Some(p.as_str()),
        _ => None,
    };
    build_webdav_provider(
        endpoint,
        conn.allow_http,
        conn.username.as_deref(),
        password,
    )
}

impl WebDavProvider {
    /// PUT a file straight from disk. A missing parent folder is answered
    /// with 409 (RFC 4918) or, by Apache's mod_dav, 403: create the folders
    /// (MKCOL, root first) and try once more. A real refusal fails again.
    fn put_file(&self, key: &str, file: &mut std::fs::File) -> Result<()> {
        let mut status = self.send_put(key, file)?;
        if status == 409 || status == 403 {
            self.mkcol_parents(key)?;
            status = self.send_put(key, file)?;
        }
        if !(200..300).contains(&status) {
            bail!("the WebDAV server answered {status} to uploading {key}");
        }
        Ok(())
    }

    fn send_put(&self, key: &str, file: &mut std::fs::File) -> Result<u16> {
        file.rewind()
            .context("rewinding the upload's temporary file")?;
        let mut req = self.agent.put(url_for(&self.root, key));
        if let Some(a) = &self.auth {
            req = req.header("Authorization", a);
        }
        // A `&File` body carries its length, so this is a plain
        // Content-Length upload, not a chunked one some servers refuse.
        let resp = req
            .send(&*file)
            .with_context(|| format!("uploading {key}"))?;
        Ok(resp.status().as_u16())
    }

    fn mkcol_parents(&self, key: &str) -> Result<()> {
        let segments: Vec<&str> = key.trim_matches('/').split('/').collect();
        let mut dir = String::new();
        for seg in &segments[..segments.len().saturating_sub(1)] {
            dir.push_str(seg);
            dir.push('/');
            let mut req = ureq::http::Request::builder()
                .method("MKCOL")
                .uri(url_for(&self.root, &dir));
            if let Some(a) = &self.auth {
                req = req.header("Authorization", a.as_str());
            }
            let req = req.body(()).context("building a MKCOL request")?;
            let status = self
                .agent
                .run(req)
                .with_context(|| format!("creating folder {dir}"))?
                .status()
                .as_u16();
            // 405: the folder is already there.
            if !(200..300).contains(&status) && status != 405 {
                bail!("the WebDAV server answered {status} to creating folder {dir}");
            }
        }
        Ok(())
    }
}

impl CloudProvider for WebDavProvider {
    fn list(&self, prefix: &str) -> Result<Vec<ObjectEntry>> {
        self.inner.list(prefix)
    }

    fn list_recursive(&self, prefix: &str, cap: usize) -> Result<(Vec<ObjectEntry>, bool)> {
        self.list_recursive_until(prefix, cap, &AtomicBool::new(false))
    }

    /// Walked one folder at a time: many servers (Nextcloud among them)
    /// refuse or quietly shorten the `Depth: infinity` listing `HttpStore`
    /// would send.
    fn list_recursive_until(
        &self,
        prefix: &str,
        cap: usize,
        stop: &AtomicBool,
    ) -> Result<(Vec<ObjectEntry>, bool)> {
        walk_recursive(&|p| self.list(p), prefix, cap, stop)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>> {
        self.inner.get(key)
    }

    /// Through the same PUT as a streamed upload, not `HttpStore`'s: that
    /// one only creates missing folders on a 409.
    fn put(&self, key: &str, bytes: Vec<u8>) -> Result<()> {
        let mut upload = self.put_streaming(key)?;
        upload.write_chunk(bytes)?;
        upload.finish()
    }

    fn delete(&self, key: &str) -> Result<()> {
        self.inner.delete(key)
    }

    fn copy(&self, from: &str, to: &str) -> Result<()> {
        self.inner.copy(from, to)
    }

    fn get_streaming(
        &self,
        key: &str,
        on_chunk: &mut dyn FnMut(&[u8]) -> Result<()>,
    ) -> Result<()> {
        self.inner.get_streaming(key, on_chunk)
    }

    /// `HttpStore` has no multipart upload, so blocks are spooled to an
    /// unnamed temp file and sent as one PUT from disk at `finish`: a large
    /// copy never has to fit in memory.
    fn put_streaming(&self, key: &str) -> Result<Box<dyn CloudUpload + '_>> {
        let spool = tempfile::tempfile().context("creating a temporary file for the upload")?;
        Ok(Box::new(WebDavUpload {
            provider: self,
            key: key.to_string(),
            spool,
        }))
    }
}

struct WebDavUpload<'a> {
    provider: &'a WebDavProvider,
    key: String,
    spool: std::fs::File,
}

impl CloudUpload for WebDavUpload<'_> {
    fn write_chunk(&mut self, bytes: Vec<u8>) -> Result<()> {
        self.spool
            .write_all(&bytes)
            .context("writing the upload's temporary file")
    }

    fn finish(mut self: Box<Self>) -> Result<()> {
        let key = std::mem::take(&mut self.key);
        self.provider.put_file(&key, &mut self.spool)
    }
}

#[cfg(test)]
#[path = "webdav_tests.rs"]
mod tests;
