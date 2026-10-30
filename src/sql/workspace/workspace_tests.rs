//! Unit tests for [`workspace`](workspace). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::attach::{AttachStrategy, attach_kind_for, duckdb_attach_sql, mask_password};
use super::*;
use crate::data::{CellValue, ColumnInfo};
use crate::sql::QueryKind;

fn simple_table(name_col: &str, name_val: &str, score: f64) -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: name_col.into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "score".into(),
                data_type: "Float64".into(),
            },
        ],
        rows: vec![vec![
            CellValue::Int(1),
            CellValue::String(name_val.into()),
            CellValue::Float(score),
        ]],
        ..DataTable::empty()
    }
}

#[test]
fn dedupe_appends_suffix() {
    let existing: std::collections::HashSet<String> =
        ["customers".to_string()].into_iter().collect();
    let name = dedupe_sql_name("Customers", |s| existing.contains(s));
    assert_eq!(name, "customers_2");
}

#[test]
fn sanitize_replaces_unsafe_chars() {
    assert_eq!(sanitize_sql_name("My File 2024.csv"), "my_file_2024_csv");
    assert_eq!(sanitize_sql_name("2024 data"), "t_2024_data");
    assert_eq!(sanitize_sql_name("___"), "table");
}

#[test]
fn workspace_round_trips_single_table() {
    let mut ws = SqlWorkspace::new().unwrap();
    let t = simple_table("name", "Alice", 9.5);
    ws.set_active_table(&t).unwrap();
    let out = ws.execute("SELECT id, score FROM data").unwrap();
    assert_eq!(out.kind, QueryKind::Select);
    assert_eq!(out.table.row_count(), 1);
    assert_eq!(out.table.col_count(), 2);
}

#[test]
fn workspace_supports_join_across_two_tables() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&simple_table("name", "Alice", 9.5))
        .unwrap();
    ws.add_table(
        "extra",
        &simple_table("label", "Tier-1", 99.0),
        TableOrigin::TabClone("extra".into()),
    )
    .unwrap();
    let out = ws
        .execute("SELECT d.name, e.label FROM data d JOIN extra e ON d.id = e.id")
        .unwrap();
    assert_eq!(out.table.row_count(), 1);
    assert_eq!(out.table.col_count(), 2);
}

#[test]
fn add_table_replaces_existing_registration() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.add_table(
        "foo",
        &simple_table("name", "v1", 1.0),
        TableOrigin::ActiveTab,
    )
    .unwrap();
    ws.add_table(
        "foo",
        &simple_table("name", "v2", 2.0),
        TableOrigin::ActiveTab,
    )
    .unwrap();
    let out = ws.execute("SELECT name FROM foo").unwrap();
    assert_eq!(out.table.get(0, 0).unwrap().to_string(), "v2");
}

#[test]
fn duckdb_attach_sql_builds_each_dialect() {
    let mut conn = crate::db::DbConnection {
        id: "db-1".into(),
        name: "prod".into(),
        engine: crate::db::DbEngine::Postgres,
        host: "db.example.com".into(),
        port: 5432,
        database: "app".into(),
        username: "octa".into(),
        auth: crate::db::DbAuth::Password,
        allow_writes: false,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: crate::db::DEFAULT_QUERY_TIMEOUT_SECS,
        tls: Default::default(),
        tunnel_port: None,
    };
    assert_eq!(
        duckdb_attach_sql(&conn, "pw", "prod", true).unwrap(),
        "ATTACH 'host=db.example.com port=5432 dbname=app user=octa password=pw \
         sslmode=prefer' AS \"prod\" (TYPE postgres, READ_ONLY)"
    );
    conn.engine = crate::db::DbEngine::MySql;
    conn.port = 3306;
    assert_eq!(
        duckdb_attach_sql(&conn, "pw", "prod", true).unwrap(),
        "ATTACH 'host=db.example.com port=3306 database=app user=octa password=pw \
         ssl_mode=preferred' AS \"prod\" (TYPE mysql, READ_ONLY)"
    );
    // The writable form (the server-to-server copy's target) drops READ_ONLY.
    let writable = duckdb_attach_sql(&conn, "pw", "prod", false).unwrap();
    assert!(writable.ends_with("(TYPE mysql)"), "{writable}");
}

#[test]
fn duckdb_attach_sql_carries_the_ssl_mode_or_refuses() {
    use crate::db::tls::{DbTls, TlsMode};
    let mut conn = crate::db::DbConnection {
        id: "db-3".into(),
        name: "k8s".into(),
        engine: crate::db::DbEngine::Postgres,
        host: "db-rw.test.svc".into(),
        port: 55432,
        database: "app".into(),
        username: "u".into(),
        auth: crate::db::DbAuth::Password,
        allow_writes: false,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: crate::db::DEFAULT_QUERY_TIMEOUT_SECS,
        tls: DbTls {
            mode: TlsMode::VerifyFull,
            ca_file: Some("/ca.pem".into()),
            connect_via: Some("127.0.0.1".into()),
        },
        tunnel_port: None,
    };
    // libpq checks the certificate for `host` and dials `hostaddr`.
    let sql = duckdb_attach_sql(&conn, "pw", "a", true).unwrap();
    assert!(
        sql.contains(
            "host=db-rw.test.svc hostaddr=127.0.0.1 port=55432 dbname=app user=u password=pw \
             sslmode=verify-full sslrootcert=/ca.pem"
        ),
        "{sql}"
    );
    // verify-full without a file checks the system's roots.
    conn.tls.ca_file = None;
    let sql = duckdb_attach_sql(&conn, "pw", "a", true).unwrap();
    assert!(
        sql.contains("sslmode=verify-full sslrootcert=system"),
        "{sql}"
    );
    // libpq refuses sslrootcert=system under verify-ca, so Octa says why.
    conn.tls.mode = TlsMode::VerifyCa;
    let err = duckdb_attach_sql(&conn, "pw", "a", true).unwrap_err();
    assert!(err.to_string().contains("CA file"), "{err}");
    // `require` with a root file would be verify-ca in libpq: not sent.
    conn.tls = DbTls {
        mode: TlsMode::Require,
        ca_file: Some("/ca.pem".into()),
        connect_via: None,
    };
    let sql = duckdb_attach_sql(&conn, "pw", "a", true).unwrap();
    assert!(sql.contains("sslmode=require'"), "{sql}");
    // A token sign-in never goes unencrypted, on Attach either.
    conn.tls.mode = TlsMode::Prefer;
    conn.auth = crate::db::DbAuth::AzureAd;
    let sql = duckdb_attach_sql(&conn, "pw", "a", true).unwrap();
    assert!(sql.contains("sslmode=require'"), "{sql}");

    // DuckDB's MySQL extension checks a certificate only against a CA file.
    conn.engine = crate::db::DbEngine::MySql;
    conn.auth = crate::db::DbAuth::Password;
    conn.tls.mode = TlsMode::VerifyFull;
    conn.tls.ca_file = None;
    assert!(duckdb_attach_sql(&conn, "pw", "a", true).is_err());
    conn.tls.ca_file = Some("/ca.pem".into());
    let sql = duckdb_attach_sql(&conn, "pw", "a", true).unwrap();
    assert!(
        sql.contains("ssl_mode=verify_identity ssl_ca=/ca.pem"),
        "{sql}"
    );
}

#[test]
fn duckdb_attach_sql_escapes_awkward_values() {
    let mut conn = crate::db::DbConnection {
        id: "db-2".into(),
        name: "odd".into(),
        engine: crate::db::DbEngine::Postgres,
        host: "h".into(),
        port: 5432,
        database: "d b".into(),
        username: "u".into(),
        auth: crate::db::DbAuth::Password,
        allow_writes: false,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: crate::db::DEFAULT_QUERY_TIMEOUT_SECS,
        tls: Default::default(),
        tunnel_port: None,
    };
    let sql = duckdb_attach_sql(&conn, "p'w \\x", "a", true).unwrap();
    // libpq quoting inside ('' doubled for the outer SQL literal): the space
    // forces quotes, the quote and backslash are backslash-escaped.
    assert_eq!(
        sql,
        "ATTACH 'host=h port=5432 dbname=''d b'' user=u password=''p\\''w \\\\x'' \
         sslmode=prefer' AS \"a\" (TYPE postgres, READ_ONLY)"
    );
    // A failed ATTACH echoes the string as the extension parsed it ('' undone),
    // and a statement error would carry it doubled: neither shows the password.
    let parsed = sql["ATTACH '".len()..sql.find("' AS").unwrap()].replace("''", "'");
    let echo = format!("IO Error: Unable to connect to Postgres at \"{parsed}\": refused");
    for msg in [echo, sql] {
        let masked = mask_password(&msg, crate::db::DbEngine::Postgres, "p'w \\x");
        assert!(masked.contains("password=*** sslmode=prefer"), "{masked}");
        assert!(!masked.contains("\\x"), "{masked}");
    }
    // DuckDB's MySQL extension reads only "..." with \" and \\, and ends a
    // bare value at `=`, so its values are quoted its own way.
    conn.engine = crate::db::DbEngine::MySql;
    let sql = duckdb_attach_sql(&conn, "p\"w \\x=", "a", true).unwrap();
    assert!(
        sql.contains("database=\"d b\" user=u password=\"p\\\"w \\\\x=\" ssl_mode"),
        "{sql}"
    );
    let masked = mask_password(&sql, crate::db::DbEngine::MySql, "p\"w \\x=");
    assert!(masked.contains("password=*** ssl_mode"), "{masked}");
}

#[test]
fn attached_table_listing_serves_from_cache_until_invalidated() {
    // White-box: seed the cache directly (a real remote attachment would need
    // a live server). Cache keys are a subset of live attachments by
    // construction (detach removes its entry), so the cache is consulted
    // before the attachment lookup.
    let ws = SqlWorkspace::new().unwrap();
    ws.attached_tables_cache.borrow_mut().insert(
        "wh".to_string(),
        vec![AttachedTable {
            schema: "public".into(),
            table: "cached_tbl".into(),
            row_count: None,
        }],
    );
    let listed = ws.list_attached_tables("wh").unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].table, "cached_tbl");

    // Invalidation drops the entry; with no real attachment behind it the
    // next listing errors instead of re-serving stale data.
    ws.invalidate_attached_cache();
    assert!(ws.list_attached_tables("wh").is_err());
}

#[test]
fn attach_kind_native_vs_import() {
    use crate::db::DbEngine;
    assert!(matches!(
        attach_kind_for(DbEngine::Redshift),
        AttachStrategy::Native {
            ext: "postgres",
            ..
        }
    ));
    assert!(matches!(
        attach_kind_for(DbEngine::MySql),
        AttachStrategy::Native { ext: "mysql", .. }
    ));
    // SQL Server and the warehouse engines have no DuckDB extension.
    for e in [
        DbEngine::Mssql,
        DbEngine::ClickHouse,
        DbEngine::Exasol,
        DbEngine::Snowflake,
        DbEngine::Databricks,
        DbEngine::BigQuery,
    ] {
        assert!(
            matches!(attach_kind_for(e), AttachStrategy::Import),
            "{e:?}"
        );
    }
    for e in [DbEngine::DuckDb, DbEngine::Sqlite] {
        assert!(matches!(attach_kind_for(e), AttachStrategy::File), "{e:?}");
    }
}

#[test]
fn create_table_hands_over_the_new_table_and_drops_it() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&simple_table("name", "a", 1.0))
        .unwrap();
    let qo = ws
        .execute("CREATE TABLE people (id INTEGER, name VARCHAR, born DATE)")
        .unwrap();
    assert_eq!(qo.kind, QueryKind::Mutation);
    assert_eq!(qo.created.as_deref(), Some("people"));
    assert_eq!(qo.table.row_count(), 0);
    let types: Vec<(&str, &str)> = qo
        .table
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c.data_type.as_str()))
        .collect();
    assert_eq!(
        types,
        vec![("id", "Int64"), ("name", "Utf8"), ("born", "Date32")]
    );
    // The workspace no longer holds it: the caller owns the table now.
    assert!(ws.execute("SELECT * FROM people").is_err());
    // `data` is untouched.
    assert_eq!(
        ws.execute("SELECT * FROM data").unwrap().table.row_count(),
        1
    );
}

#[test]
fn create_table_as_select_and_view_carry_rows() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&simple_table("name", "a", 1.0))
        .unwrap();
    let qo = ws
        .execute("CREATE TEMP TABLE \"my copy\" AS SELECT id, score * 2 AS dbl FROM data")
        .unwrap();
    assert_eq!(qo.created.as_deref(), Some("my copy"));
    assert_eq!(qo.table.row_count(), 1);
    assert_eq!(qo.table.get(0, 1), Some(&CellValue::Float(2.0)));

    let qo = ws
        .execute("CREATE VIEW v AS SELECT name FROM data")
        .unwrap();
    assert_eq!(qo.created.as_deref(), Some("v"));
    assert_eq!(qo.table.get(0, 0), Some(&CellValue::String("a".into())));
    assert!(ws.execute("SELECT * FROM v").is_err());
}

#[test]
fn create_that_adds_nothing_new_takes_the_mutation_path() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&simple_table("name", "a", 1.0))
        .unwrap();
    // Replacing `data` itself is a mutation of `data`, not a new table.
    let qo = ws
        .execute("CREATE OR REPLACE TEMP TABLE data AS SELECT 7 AS id")
        .unwrap();
    assert_eq!(qo.created, None);
    assert_eq!(qo.table.get(0, 0), Some(&CellValue::Int(7)));
}

#[test]
fn create_inside_an_attached_database_is_refused_not_handed_over() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("side.duckdb");
    duckdb::Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);")
        .unwrap();
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&simple_table("name", "a", 1.0))
        .unwrap();
    ws.attach(&path, "side", AttachKind::DuckDb).unwrap();
    // Attachments are read-only, so DuckDB refuses; the error reaches the
    // caller and the catalog diff never runs.
    let err = ws
        .execute("CREATE TABLE side.u AS SELECT x + 1 AS y FROM side.t")
        .unwrap_err();
    assert!(format!("{err:#}").contains("read-only"), "{err:#}");
    assert!(ws.execute("SELECT * FROM side.u").is_err());
    assert_eq!(
        ws.execute("SELECT * FROM data").unwrap().table.row_count(),
        1
    );
}

#[test]
fn explain_names_the_table_it_reads() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&simple_table("name", "a", 1.5))
        .unwrap();
    let plan = ws.explain("SELECT * FROM data WHERE score > 1").unwrap();
    assert!(
        plan.steps
            .iter()
            .any(|s| s.kind == crate::sql::explain::StepKind::ReadTable
                && s.table.as_deref() == Some("data")),
        "{plan:?}"
    );
    assert!(
        plan.server_text.contains("SEQ_SCAN"),
        "{}",
        plan.server_text
    );
    assert!(!plan.server_text.contains('\u{2500}'));
}

#[test]
fn explaining_a_delete_deletes_nothing() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&simple_table("name", "a", 1.5))
        .unwrap();
    ws.explain("DELETE FROM data").unwrap();
    let left = ws.execute("SELECT count(*) AS n FROM data").unwrap();
    assert_eq!(left.table.rows[0][0].to_string(), "1");
}

/// A large-file tab's `data` is a view over the whole file, listed with the
/// file's row count, and `data` swaps between that view and a copied table
/// either way round: DuckDB refuses to `DROP TABLE` a view.
#[test]
fn active_scan_reads_the_whole_file_and_swaps_with_a_copied_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.csv");
    let mut csv = String::from("id,name\n");
    for i in 0..5000 {
        csv.push_str(&format!("{i},row {i}\n"));
    }
    std::fs::write(&path, csv).unwrap();
    let count = |ws: &mut SqlWorkspace| {
        ws.execute("SELECT count(*) AS n FROM data")
            .unwrap()
            .table
            .rows[0][0]
            .to_string()
    };

    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_scan(&path, 5000).unwrap();
    assert_eq!(count(&mut ws), "5000");
    let listed = ws.list_tables();
    assert!(
        listed
            .iter()
            .any(|t| t.sql_name == "data" && t.row_count == 5000)
    );
    // The inspector takes the count from the registration, not a scan.
    assert_eq!(
        ws.inspect_registered_table("data", 3).unwrap().row_count,
        Some(5000)
    );

    ws.set_active_table(&simple_table("name", "a", 1.5))
        .unwrap();
    assert_eq!(count(&mut ws), "1");
    ws.set_active_scan(&path, 5000).unwrap();
    assert_eq!(count(&mut ws), "5000");
}

/// A job copies `data` a slice at a time on the runner connection, swaps it
/// in whole, and runs the query after it; the window's connection sees the
/// result. A copy whose feed is dropped leaves the old `data` in place.
#[test]
fn a_job_copies_data_then_runs_the_query_off_the_window_connection() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&simple_table("name", "old", 1.0))
        .unwrap();
    let t = simple_table("name", "a", 1.0);

    let (tx, rx) = std::sync::mpsc::sync_channel(4);
    for i in 0..3 {
        let mut slice = t.clone();
        slice.rows[0][0] = CellValue::Int(i);
        tx.send(Some(slice)).unwrap();
    }
    tx.send(None).unwrap();
    let feed = DataFeed {
        columns: t.columns.clone(),
        rows: rx,
    };
    let query = JobQuery {
        sql: "SELECT id FROM data ORDER BY id".to_string(),
        page_rows: 2,
        slot: 9,
    };
    let (job, _) = ws.run_job(Some(feed), Some(query)).unwrap();
    let done = std::thread::spawn(job).join().unwrap();
    assert_eq!(done.copied, Some(Ok(3)));
    let paged = done.query.unwrap().unwrap();
    assert_eq!(paged.total_rows, Some(3));
    assert_eq!(paged.outcome.table.row_count(), 2);
    // The window's own connection pages the same result and sees `data`.
    assert_eq!(ws.result_page(9, 2, 2).unwrap().row_count(), 1);
    ws.note_active_table(3);
    assert_eq!(
        ws.execute("SELECT count(*) FROM data")
            .unwrap()
            .table
            .get(0, 0),
        Some(&CellValue::Int(3))
    );

    // Stopped half way: the old `data` stays.
    let (tx, rx) = std::sync::mpsc::sync_channel(4);
    tx.send(Some(t.clone())).unwrap();
    drop(tx);
    let feed = DataFeed {
        columns: t.columns.clone(),
        rows: rx,
    };
    let (job, _) = ws.run_job(Some(feed), None).unwrap();
    let done = std::thread::spawn(job).join().unwrap();
    assert!(matches!(done.copied, Some(Err(_))));
    assert!(done.query.is_none());
    assert_eq!(
        ws.execute("SELECT count(*) FROM data")
            .unwrap()
            .table
            .get(0, 0),
        Some(&CellValue::Int(3))
    );
}

/// A SQL Server with `n` tables of one row each; `broken` fails to read.
struct ImportFake {
    tables: Vec<String>,
    broken: Option<String>,
}

impl crate::db::DbConnector for ImportFake {
    fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
        Ok(vec!["dbo".into()])
    }
    fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
        Ok(self.tables.clone())
    }
    fn query(&mut self, sql: &str) -> anyhow::Result<DataTable> {
        if let Some(b) = &self.broken
            && sql.contains(b.as_str())
        {
            anyhow::bail!("permission denied for {b}");
        }
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        }];
        t.rows = vec![vec![CellValue::Int(1)]];
        Ok(t)
    }
    fn engine(&self) -> crate::db::DbEngine {
        crate::db::DbEngine::Mssql
    }
    fn execute(&mut self, _: &str) -> anyhow::Result<u64> {
        Ok(0)
    }
    fn write_table(
        &mut self,
        _: Option<&str>,
        _: &str,
        _: &str,
        _: crate::db::DbWriteMode,
        _: &DataTable,
    ) -> anyhow::Result<crate::db::DbWriteReport> {
        anyhow::bail!("read only")
    }
}

fn mssql_conn() -> crate::db::DbConnection {
    crate::db::DbConnection {
        id: "db-mssql".into(),
        name: "erp".into(),
        engine: crate::db::DbEngine::Mssql,
        host: "db.example.com".into(),
        port: 1433,
        database: "erp".into(),
        username: "octa".into(),
        auth: crate::db::DbAuth::Password,
        allow_writes: false,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: crate::db::DEFAULT_QUERY_TIMEOUT_SECS,
        tls: Default::default(),
        tunnel_port: None,
    }
}

/// The old 60-table cap is gone: a schema of 100 tables imports whole, every
/// table queryable from the workspace, with progress reported per table.
#[test]
fn an_import_takes_every_table_with_no_cap() {
    let mut ws = SqlWorkspace::new().unwrap();
    let mut fake = ImportFake {
        tables: (0..100).map(|i| format!("t{i}")).collect(),
        broken: None,
    };
    let mut sink = ws.import_sink();
    let stop = std::sync::atomic::AtomicBool::new(false);
    let mut last = (0, 0);
    let imported = import_scope(
        &mut fake,
        &mssql_conn(),
        &AttachScope::default(),
        &mut sink,
        &stop,
        |d, t| last = (d, t),
    )
    .unwrap();
    assert_eq!(last, (100, 100));
    let names = ws.note_imported(imported);
    assert_eq!(names.len(), 100);
    let out = ws.execute("SELECT count(*) FROM t99").unwrap();
    assert_eq!(out.table.get(0, 0).unwrap().to_string(), "1");
}

/// All or nothing: a failure halfway, or a stop, takes back the tables
/// already made, so no attachment is ever partly there.
#[test]
fn a_failed_or_stopped_import_leaves_nothing_behind() {
    let mut ws = SqlWorkspace::new().unwrap();
    let mut fake = ImportFake {
        tables: vec!["a".into(), "b".into(), "broken".into()],
        broken: Some("broken".into()),
    };
    let stop = std::sync::atomic::AtomicBool::new(false);
    let mut sink = ws.import_sink();
    let err = import_scope(
        &mut fake,
        &mssql_conn(),
        &AttachScope::default(),
        &mut sink,
        &stop,
        |_, _| {},
    )
    .err()
    .expect("the broken table fails the import");
    assert!(format!("{err:#}").contains("permission denied"));
    assert!(ws.execute("SELECT * FROM a").is_err(), "a was taken back");

    fake.broken = None;
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut sink = ws.import_sink();
    assert!(
        import_scope(
            &mut fake,
            &mssql_conn(),
            &AttachScope::default(),
            &mut sink,
            &stop,
            |_, _| {},
        )
        .is_err()
    );
    assert!(ws.list_tables().iter().all(|t| t.sql_name != "a"));
}

/// DuckDB keeps ATTACHed databases per database instance, not per
/// connection: one attached on the runner is queryable from the workspace's
/// own connection. The background ATTACH relies on it.
#[test]
fn an_attach_on_the_runner_is_seen_by_the_window_connection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.duckdb");
    {
        let c = duckdb::Connection::open(&path).unwrap();
        c.execute_batch("CREATE TABLE t AS SELECT 42 AS v").unwrap();
    }
    let mut ws = SqlWorkspace::new().unwrap();
    ws.runner
        .lock()
        .unwrap()
        .execute_batch(&format!("ATTACH '{}' AS other (READ_ONLY)", path.display()))
        .unwrap();
    let out = ws.execute("SELECT v FROM other.t").unwrap();
    assert_eq!(out.table.get(0, 0).unwrap().to_string(), "42");
}

/// Other open tabs are copied by a worker on the runner, one table per
/// feed; a feed that is dropped (its tab changed) fails alone and leaves
/// nothing behind, the rest still land.
#[test]
fn other_tabs_copy_on_the_runner_and_one_failure_stays_alone() {
    let mut ws = SqlWorkspace::new().unwrap();
    let cols = vec![ColumnInfo {
        name: "v".into(),
        data_type: "Int64".into(),
    }];
    let (tx_ok, rx_ok) = std::sync::mpsc::sync_channel(4);
    let (tx_gone, rx_gone) = std::sync::mpsc::sync_channel::<Option<DataTable>>(4);
    let mut slice = DataTable::empty();
    slice.columns = cols.clone();
    slice.rows = vec![vec![CellValue::Int(7)], vec![CellValue::Int(8)]];
    tx_ok.send(Some(slice)).unwrap();
    tx_ok.send(None).unwrap();
    drop(tx_gone);
    let job = ws.copy_tables_job(vec![
        (
            "sales".into(),
            DataFeed {
                columns: cols.clone(),
                rows: rx_ok,
            },
        ),
        (
            "gone".into(),
            DataFeed {
                columns: cols,
                rows: rx_gone,
            },
        ),
    ]);
    let done = std::thread::spawn(job).join().unwrap();
    assert_eq!(done[0].1.as_ref().copied(), Ok(2));
    assert!(done[1].1.is_err());
    ws.note_table("sales", TableOrigin::TabClone("sales.csv".into()), 2);
    let out = ws.execute("SELECT sum(v) FROM sales").unwrap();
    assert_eq!(out.table.get(0, 0).unwrap().to_string(), "15");
    assert!(ws.execute("SELECT * FROM gone").is_err());
    assert!(ws.execute("SELECT * FROM __octa_next_gone").is_err());
}
