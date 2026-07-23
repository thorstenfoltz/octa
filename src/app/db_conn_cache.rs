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

type ConnectFn =
    dyn Fn(&DbConnection, Option<&str>) -> anyhow::Result<Box<dyn DbConnector>> + Send + Sync;

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
    ) -> anyhow::Result<(SharedConnector, bool)> {
        if let Some(c) = self
            .inner
            .lock()
            .ok()
            .and_then(|m| m.get(&conn.id).cloned())
        {
            return Ok((c, true));
        }
        let fresh: SharedConnector = Arc::new(Mutex::new((self.connect_fn)(conn, secret)?));
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
        mut f: impl FnMut(&mut dyn DbConnector) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let (shared, was_cached) = self.get_or_connect(conn, secret)?;
        let res = f(lock_connector(&shared).as_mut());
        match res {
            Err(_) if was_cached => {
                self.invalidate(&conn.id);
                let (shared, _) = self.get_or_connect(conn, secret)?;
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
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use octa::data::DataTable;
    use octa::db::{DbAuth, DbEngine, DbWriteMode, DbWriteReport};

    use super::*;

    /// Fake connector: `query` fails `fail_first` times, then succeeds.
    struct FakeConnector {
        fail_first: Arc<AtomicUsize>,
    }

    impl DbConnector for FakeConnector {
        fn engine(&self) -> DbEngine {
            DbEngine::Postgres
        }
        fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn list_tables(&mut self, _: Option<&str>, _schema: &str) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn query(&mut self, _sql: &str) -> anyhow::Result<DataTable> {
            if self
                .fail_first
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
            {
                anyhow::bail!("simulated stale connection");
            }
            Ok(DataTable::empty())
        }
        fn execute(&mut self, _sql: &str) -> anyhow::Result<u64> {
            Ok(0)
        }
        fn write_table(
            &mut self,
            _catalog: Option<&str>,
            _schema: &str,
            _table: &str,
            _mode: DbWriteMode,
            _data: &DataTable,
        ) -> anyhow::Result<DbWriteReport> {
            anyhow::bail!("not used")
        }
    }

    fn test_conn() -> DbConnection {
        DbConnection {
            id: "c1".into(),
            name: "t".into(),
            engine: DbEngine::Postgres,
            host: "h".into(),
            port: 5432,
            database: "d".into(),
            username: "u".into(),
            auth: DbAuth::Password,
            allow_writes: false,
            oauth_client_id: None,
            oauth_tenant: None,
        }
    }

    /// A cache whose factory counts connects and hands out fake connectors
    /// that fail their first `fail_first` queries.
    fn fake_cache(fail_first: usize) -> (DbConnCache, Arc<AtomicUsize>) {
        let connects = Arc::new(AtomicUsize::new(0));
        let counter = connects.clone();
        let fails = Arc::new(AtomicUsize::new(fail_first));
        let cache = DbConnCache {
            inner: Arc::default(),
            connect_fn: Arc::new(move |_conn, _secret| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(FakeConnector {
                    fail_first: fails.clone(),
                }) as Box<dyn DbConnector>)
            }),
        };
        (cache, connects)
    }

    #[test]
    fn second_call_reuses_the_connector() {
        let (cache, connects) = fake_cache(0);
        let conn = test_conn();
        cache
            .with_conn(&conn, None, |c| c.query("SELECT 1"))
            .unwrap();
        cache
            .with_conn(&conn, None, |c| c.query("SELECT 1"))
            .unwrap();
        assert_eq!(connects.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn fresh_connection_failure_does_not_retry() {
        // The connector's very first query fails: with_conn connected fresh,
        // so the error surfaces without a reconnect.
        let (cache, connects) = fake_cache(1);
        let conn = test_conn();
        assert!(cache.with_conn(&conn, None, |c| c.query("x")).is_err());
        assert_eq!(connects.load(Ordering::SeqCst), 1);
    }

    /// A cache sharing one failure counter across ALL connectors it mints,
    /// so a test can make the *cached* connection "die" later.
    fn armable_cache() -> (DbConnCache, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let connects = Arc::new(AtomicUsize::new(0));
        let fails = Arc::new(AtomicUsize::new(0));
        let (counter, shared_fails) = (connects.clone(), fails.clone());
        let cache = DbConnCache {
            inner: Arc::default(),
            connect_fn: Arc::new(move |_c, _s| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(FakeConnector {
                    fail_first: shared_fails.clone(),
                }) as Box<dyn DbConnector>)
            }),
        };
        (cache, connects, fails)
    }

    #[test]
    fn stale_cached_connector_heals() {
        let (cache, connects, fails) = armable_cache();
        let conn = test_conn();
        cache.with_conn(&conn, None, |c| c.query("x")).unwrap();
        assert_eq!(connects.load(Ordering::SeqCst), 1);
        // The cached connection "dies": its next query fails once, so
        // with_conn must reconnect exactly once and retry successfully.
        fails.store(1, Ordering::SeqCst);
        cache.with_conn(&conn, None, |c| c.query("x")).unwrap();
        assert_eq!(connects.load(Ordering::SeqCst), 2, "one reconnect");
    }

    #[test]
    fn invalidate_and_clear_drop_entries() {
        let (cache, connects) = fake_cache(0);
        let conn = test_conn();
        cache.with_conn(&conn, None, |c| c.query("x")).unwrap();
        cache.invalidate(&conn.id);
        cache.with_conn(&conn, None, |c| c.query("x")).unwrap();
        assert_eq!(connects.load(Ordering::SeqCst), 2);
        cache.clear();
        cache.with_conn(&conn, None, |c| c.query("x")).unwrap();
        assert_eq!(connects.load(Ordering::SeqCst), 3);
    }
}
