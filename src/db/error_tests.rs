use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::data::DataTable;
use crate::db::{DbConnection, DbEngine, DbWriteMode, DbWriteReport};

/// Fails every call with what `fail` makes, and records whether its own
/// `fetch_batches` (not the trait's default) ran.
struct Fake {
    fail: fn() -> anyhow::Error,
    own_fetch: Arc<AtomicBool>,
    sink_fails: bool,
}

impl DbConnector for Fake {
    fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
        Err((self.fail)())
    }
    fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
        Err((self.fail)())
    }
    fn query(&mut self, _: &str) -> anyhow::Result<DataTable> {
        Err((self.fail)())
    }
    fn engine(&self) -> DbEngine {
        DbEngine::Databricks
    }
    fn fetch_batches(
        &mut self,
        _: &str,
        _: usize,
        sink: &mut dyn FnMut(DataTable) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        self.own_fetch.store(true, Ordering::SeqCst);
        if self.sink_fails {
            return sink(DataTable::empty());
        }
        Err((self.fail)())
    }
    fn execute(&mut self, _: &str) -> anyhow::Result<u64> {
        Err((self.fail)())
    }
    fn write_table(
        &mut self,
        _: Option<&str>,
        _: &str,
        _: &str,
        _: DbWriteMode,
        _: &DataTable,
    ) -> anyhow::Result<DbWriteReport> {
        Err((self.fail)())
    }
    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        Some(Box::new(|| {}))
    }
}

fn wrapped(fail: fn() -> anyhow::Error) -> (Box<dyn DbConnector>, Arc<AtomicBool>) {
    let own_fetch = Arc::new(AtomicBool::new(false));
    let fake = Fake {
        fail,
        own_fetch: own_fetch.clone(),
        sink_fails: false,
    };
    (labelled(Box::new(fake), "Databricks Imec"), own_fetch)
}

#[test]
fn an_error_the_server_sent_names_the_engine() {
    let (mut c, _) = wrapped(|| {
        anyhow::Error::new(ServerError("[TABLE_OR_VIEW_NOT_FOUND] no table x".into()))
            .context("running the statement")
    });
    let err = format!("{:#}", c.query("SELECT * FROM x").unwrap_err());
    assert_eq!(
        err,
        "Databricks returned an error.\nrunning the statement: [TABLE_OR_VIEW_NOT_FOUND] no table x"
    );
}

#[test]
fn any_other_failure_says_the_conversation_broke_off() {
    let (mut c, _) = wrapped(|| anyhow::anyhow!("no result after 300 s"));
    for err in [
        c.query("SELECT 1").map(|_| ()).unwrap_err(),
        c.execute("DELETE FROM x").map(|_| ()).unwrap_err(),
        c.list_schemas(None).map(|_| ()).unwrap_err(),
        c.list_tables(None, "s").map(|_| ()).unwrap_err(),
        c.write_table(None, "s", "t", DbWriteMode::Create, &DataTable::empty())
            .map(|_| ())
            .unwrap_err(),
    ] {
        assert_eq!(
            format!("{err:#}"),
            "Octa could not finish talking to 'Databricks Imec'.\nno result after 300 s"
        );
    }
}

#[test]
fn the_wrapper_keeps_what_the_connector_does_its_own_way() {
    let (mut c, own_fetch) = wrapped(|| anyhow::anyhow!("x"));
    assert_eq!(c.engine(), DbEngine::Databricks);
    assert!(c.cancel_handle().is_some());
    let _ = c.fetch_batches("SELECT 1", 10, &mut |_| Ok(()));
    assert!(
        own_fetch.load(Ordering::SeqCst),
        "the trait's default fetch_batches ran instead of the connector's"
    );
}

#[test]
fn a_failure_of_the_receiving_side_is_not_blamed_on_the_connection() {
    // An export writes each batch to a file; a full disk is not the
    // database's doing, nor the connection's.
    let fake = Fake {
        fail: || anyhow::anyhow!("x"),
        own_fetch: Arc::new(AtomicBool::new(false)),
        sink_fails: true,
    };
    let mut c = labelled(Box::new(fake), "Databricks Imec");
    let err = c
        .fetch_batches("SELECT 1", 10, &mut |_| {
            anyhow::bail!("No space left on device")
        })
        .unwrap_err();
    assert_eq!(format!("{err:#}"), "No space left on device");
}

fn file_conn(dir: &std::path::Path, engine: DbEngine) -> DbConnection {
    let path = dir.join(match engine {
        DbEngine::Sqlite => "shop.sqlite",
        _ => "shop.duckdb",
    });
    match engine {
        DbEngine::Sqlite => drop(rusqlite::Connection::open(&path).unwrap()),
        _ => drop(duckdb::Connection::open(&path).unwrap()),
    }
    DbConnection::file("id".into(), "shop", engine, &path.to_string_lossy(), false)
}

#[test]
fn a_real_engine_error_is_recognised_as_the_engines() {
    let dir = tempfile::tempdir().unwrap();
    for (engine, label) in [(DbEngine::Sqlite, "SQLite"), (DbEngine::DuckDb, "DuckDB")] {
        let mut c = crate::db::connect(&file_conn(dir.path(), engine), None, None).unwrap();
        let err = format!("{:#}", c.query("SELEC 1").unwrap_err());
        assert!(
            err.starts_with(&format!("{label} returned an error.\n")),
            "{engine:?}: {err}"
        );
    }
}

#[test]
fn a_failed_connect_says_octa_could_not_connect() {
    let conn = DbConnection::file(
        "id".into(),
        "shop",
        DbEngine::Sqlite,
        "/nonexistent/octa/shop.sqlite",
        false,
    );
    let err = format!("{:#}", crate::db::connect(&conn, None, None).err().unwrap());
    assert!(
        err.starts_with("Octa could not connect to 'shop'.\n"),
        "{err}"
    );
}
