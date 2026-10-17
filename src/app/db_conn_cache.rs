//! Process-lifetime cache of live database connectors, keyed by connection
//! id. Before this cache every sidebar expansion, table open and server
//! query paid a full TCP + TLS + auth handshake (`db::connect` per call),
//! which made browsing two servers in parallel feel sluggish. One connector
//! per connection id, one operation at a time per connection (the inner
//! Mutex); different connections run in parallel. Workers clone the cache
//! (cheap, `Arc`-backed); the Settings dialog clears it on Apply so edited
//! or deleted connections never reuse a stale connector.
//!
//! ponytail: one connector per connection id, so two operations on the SAME
//! connection serialise; upgrade to a small per-id pool if that ever hurts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use octa::db::{DbConnection, DbConnector};

pub(crate) type SharedConnector = Arc<Mutex<Box<dyn DbConnector>>>;

type ConnectFn = dyn Fn(&DbConnection, Option<&str>, Option<&str>) -> anyhow::Result<Box<dyn DbConnector>>
    + Send
    + Sync;

#[derive(Clone)]
pub(crate) struct DbConnCache {
    inner: Arc<Mutex<HashMap<String, SharedConnector>>>,
    /// Connection factory; `db::connect` in production, injectable in tests.
    connect_fn: Arc<ConnectFn>,
}

impl Default for DbConnCache {
    fn default() -> Self {
        Self {
            inner: Arc::default(),
            connect_fn: Arc::new(octa::db::connect),
        }
    }
}

impl DbConnCache {
    /// The cached connector for `conn.id`, connecting when absent. The bool
    /// says whether it came from the cache, i.e. whether a failure may just
    /// be a stale TCP connection worth one reconnect.
    pub(crate) fn get_or_connect(
        &self,
        conn: &DbConnection,
        secret: Option<&str>,
        ssh_secret: Option<&str>,
    ) -> anyhow::Result<(SharedConnector, bool)> {
        if let Some(c) = self
            .inner
            .lock()
            .ok()
            .and_then(|m| m.get(&conn.id).cloned())
        {
            return Ok((c, true));
        }
        let fresh: SharedConnector =
            Arc::new(Mutex::new((self.connect_fn)(conn, secret, ssh_secret)?));
        if let Ok(mut m) = self.inner.lock() {
            m.insert(conn.id.clone(), fresh.clone());
        }
        Ok((fresh, false))
    }

    /// Run `f` on the (possibly cached) connector. When a CACHED connector
    /// fails, drop it, reconnect once and retry `f` once - this heals dead
    /// TCP connections (server restart, idle timeout) transparently. A
    /// failure on a freshly connected connector returns as-is.
    ///
    /// ponytail: the retry re-sends the statement; safe because a failed
    /// statement did not apply, and the write-back path is one transaction.
    pub(crate) fn with_conn<T>(
        &self,
        conn: &DbConnection,
        secret: Option<&str>,
        ssh_secret: Option<&str>,
        mut f: impl FnMut(&mut dyn DbConnector) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let (shared, was_cached) = self.get_or_connect(conn, secret, ssh_secret)?;
        let res = f(lock_connector(&shared).as_mut());
        match res {
            Err(_) if was_cached => {
                self.invalidate(&conn.id);
                let (shared, _) = self.get_or_connect(conn, secret, ssh_secret)?;
                f(lock_connector(&shared).as_mut())
            }
            other => other,
        }
    }

    /// Drop one connection's cached connector (e.g. after an error).
    pub(crate) fn invalidate(&self, conn_id: &str) {
        if let Ok(mut m) = self.inner.lock() {
            m.remove(conn_id);
        }
    }

    /// Drop every cached connector (Settings apply: connections may have
    /// been edited, deleted, or had their secret changed).
    pub(crate) fn clear(&self) {
        if let Ok(mut m) = self.inner.lock() {
            m.clear();
        }
    }
}

/// Lock a shared connector, shrugging off poisoning: a panic on another
/// worker mid-operation leaves the connector suspect, but every caller
/// either retries through `with_conn` or surfaces the error anyway.
pub(crate) fn lock_connector(
    shared: &SharedConnector,
) -> std::sync::MutexGuard<'_, Box<dyn DbConnector>> {
    shared.lock().unwrap_or_else(|p| p.into_inner())
}

#[cfg(test)]
#[path = "db_conn_cache_tests.rs"]
mod tests;
