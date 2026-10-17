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
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
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
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: octa::db::DEFAULT_QUERY_TIMEOUT_SECS,
        tunnel_port: None,
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
        connect_fn: Arc::new(move |_conn, _secret, _ssh| {
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
        .with_conn(&conn, None, None, |c| c.query("SELECT 1"))
        .unwrap();
    cache
        .with_conn(&conn, None, None, |c| c.query("SELECT 1"))
        .unwrap();
    assert_eq!(connects.load(Ordering::SeqCst), 1);
}

#[test]
fn fresh_connection_failure_does_not_retry() {
    // The connector's very first query fails: with_conn connected fresh,
    // so the error surfaces without a reconnect.
    let (cache, connects) = fake_cache(1);
    let conn = test_conn();
    assert!(
        cache
            .with_conn(&conn, None, None, |c| c.query("x"))
            .is_err()
    );
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
        connect_fn: Arc::new(move |_c, _s, _ssh| {
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
    cache
        .with_conn(&conn, None, None, |c| c.query("x"))
        .unwrap();
    assert_eq!(connects.load(Ordering::SeqCst), 1);
    // The cached connection "dies": its next query fails once, so
    // with_conn must reconnect exactly once and retry successfully.
    fails.store(1, Ordering::SeqCst);
    cache
        .with_conn(&conn, None, None, |c| c.query("x"))
        .unwrap();
    assert_eq!(connects.load(Ordering::SeqCst), 2, "one reconnect");
}

#[test]
fn invalidate_and_clear_drop_entries() {
    let (cache, connects) = fake_cache(0);
    let conn = test_conn();
    cache
        .with_conn(&conn, None, None, |c| c.query("x"))
        .unwrap();
    cache.invalidate(&conn.id);
    cache
        .with_conn(&conn, None, None, |c| c.query("x"))
        .unwrap();
    assert_eq!(connects.load(Ordering::SeqCst), 2);
    cache.clear();
    cache
        .with_conn(&conn, None, None, |c| c.query("x"))
        .unwrap();
    assert_eq!(connects.load(Ordering::SeqCst), 3);
}
