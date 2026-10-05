//! Live-server integration tests for the DB connectors. Each engine's tests
//! run only when its env var is set (see the plan's docker rig):
//!
//! ```bash
//! export OCTA_TEST_POSTGRES_URL='host=127.0.0.1;port=5432;db=postgres;user=postgres;pass=pw'
//! export OCTA_TEST_MYSQL_URL='host=127.0.0.1;port=3306;db=mysql;user=root;pass=pw'
//! export OCTA_TEST_MSSQL_URL='host=127.0.0.1;port=1433;db=master;user=sa;pass=Str0ng!Pw'
//! export OCTA_TEST_ORACLE_URL='host=127.0.0.1;port=1521;db=FREEPDB1;user=system;pass=pw'
//! export OCTA_TEST_TRINO_URL='host=http://127.0.0.1;port=8080;db=tpch;user=octa'
//! ```
//!
//! Without the env var a test prints "skipped" and passes, so CI stays green
//! with no database available.

use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::db::{DbAuth, DbConnection, DbEngine, DbWriteMode, connect, ensure_write_allowed};

/// Parse `host=..;port=..;db=..;user=..;pass=..` into a connection.
fn conn_from_env(var: &str, engine: DbEngine) -> Option<(DbConnection, String)> {
    let raw = std::env::var(var).ok()?;
    let mut host = String::new();
    let mut port = engine.default_port();
    let mut db = String::new();
    let mut user = String::new();
    let mut pass = String::new();
    for part in raw.split(';') {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        match k.trim() {
            "host" => host = v.trim().to_string(),
            "port" => port = v.trim().parse().unwrap_or(port),
            "db" => db = v.trim().to_string(),
            "user" => user = v.trim().to_string(),
            "pass" => pass = v.trim().to_string(),
            _ => {}
        }
    }
    Some((
        DbConnection {
            id: format!("test-{var}"),
            name: format!("test {}", engine.label()),
            engine,
            host,
            port,
            database: db,
            username: user,
            auth: DbAuth::Password,
            allow_writes: true,
            oauth_client_id: None,
            oauth_tenant: None,
            athena_workgroup: None,
            athena_output_location: None,
            query_timeout_secs: octa::db::DEFAULT_QUERY_TIMEOUT_SECS,
            ssh: None,
            tunnel_port: None,
        },
        pass,
    ))
}

fn sample_table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::Int(1), CellValue::String("ada".into())],
        vec![CellValue::Int(2), CellValue::String("o'hara".into())],
    ];
    t
}

/// The shared engine exercise: connect, SELECT 1, list schemas/tables,
/// write-back round-trip (create + append), read-only gate.
fn exercise(engine: DbEngine, env_var: &str, expect_schema: &str, write_schema: &str) {
    let Some((mut conn, pass)) = conn_from_env(env_var, engine) else {
        eprintln!("skipped: {env_var} not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");

    let one = c.query("SELECT 1 AS one").expect("select 1");
    assert_eq!(one.row_count(), 1);

    let schemas = c.list_schemas(None).expect("list schemas");
    assert!(
        schemas.iter().any(|s| s == expect_schema),
        "{expect_schema} missing from {schemas:?}"
    );
    // Every listed schema must enumerate without error.
    let tables = c.list_tables(None, expect_schema).expect("list tables");
    let _ = tables;

    // Write-back round trip into a throwaway table.
    let table_name = format!(
        "octa_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let data = sample_table();
    let report = c
        .write_table(None, write_schema, &table_name, DbWriteMode::Create, &data)
        .expect("create + write");
    assert!(report.created);
    assert_eq!(report.rows_written, 2);
    let report2 = c
        .write_table(None, write_schema, &table_name, DbWriteMode::Append, &data)
        .expect("append");
    assert!(!report2.created);
    let qt = format!(
        "SELECT COUNT(*) AS n FROM {}.{}",
        engine.quote_ident(write_schema),
        engine.quote_ident(&table_name)
    );
    let count = c.query(&qt).expect("count");
    assert_eq!(count.row_count(), 1);
    assert_eq!(count.rows[0][0], CellValue::Int(4));
    // Quoted value survived (the o'hara row).
    let names = c
        .query(&format!(
            "SELECT name FROM {}.{} WHERE id = 2",
            engine.quote_ident(write_schema),
            engine.quote_ident(&table_name)
        ))
        .expect("select names");
    assert_eq!(names.rows[0][0], CellValue::String("o'hara".into()));
    c.execute(&format!(
        "DROP TABLE {}.{}",
        engine.quote_ident(write_schema),
        engine.quote_ident(&table_name)
    ))
    .expect("drop");

    // The read-only gate refuses mutations without touching the server.
    conn.allow_writes = false;
    assert!(ensure_write_allowed(&conn, Some("DELETE FROM x")).is_err());
    assert!(ensure_write_allowed(&conn, Some("SELECT 1")).is_ok());
}

/// Live round-trip of the diff-based write-back: create a PK table, load it
/// as an editable tab does (rows + `DbRowMeta` baseline + PK discovery),
/// edit / insert / delete, build + apply the plan in one transaction,
/// re-query and assert the server matches; then force a failure (duplicate
/// PK insert) and assert the rollback left the table untouched.
fn exercise_write_back(engine: DbEngine, env_var: &str, schema: &str) {
    use octa::db::write_back::{apply_write_back, build_write_back_plan};

    let Some((conn, pass)) = conn_from_env(env_var, engine) else {
        eprintln!("skipped: {env_var} not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    let table_name = format!(
        "octa_wb_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let target = format!(
        "{}.{}",
        engine.quote_ident(schema),
        engine.quote_ident(&table_name)
    );
    let text_type = match engine {
        DbEngine::Mssql => "NVARCHAR(100)",
        _ => "VARCHAR(100)",
    };
    c.execute(&format!(
        "CREATE TABLE {target} (id BIGINT PRIMARY KEY, name {text_type})"
    ))
    .expect("create pk table");
    c.execute(&format!(
        "INSERT INTO {target} (id, name) VALUES (1, 'a'), (2, 'b'), (3, 'c')"
    ))
    .expect("seed rows");

    // Row-key discovery via the shared information_schema query: a primary
    // key when there is one, else a NOT NULL unique constraint.
    let keys = c
        .query(&octa::db::row_key_sql(engine, None, schema, &table_name))
        .expect("row key query");
    let candidates: Vec<octa::db::RowKeyCandidate> = keys
        .rows
        .iter()
        .filter_map(|r| {
            Some(octa::db::RowKeyCandidate {
                constraint_type: r.first()?.to_string(),
                constraint_name: r.get(1)?.to_string(),
                column_name: r.get(2)?.to_string(),
                nullable: r
                    .get(3)
                    .map(|v| v.to_string().eq_ignore_ascii_case("YES"))
                    .unwrap_or(true),
            })
        })
        .collect();
    let pk_cols = octa::db::choose_row_key(&candidates);
    assert_eq!(pk_cols, vec!["id".to_string()]);
    let identity = octa::db::write_back::RowIdentity::Key(pk_cols);

    // Load + baseline, exactly as the sidebar open worker builds it.
    let mut t = c
        .query(&octa::db::select_sample_sql(
            engine,
            None,
            schema,
            &table_name,
            1000,
        ))
        .expect("load");
    let original: std::collections::HashMap<i64, Vec<CellValue>> = t
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| (i as i64, r.clone()))
        .collect();
    t.db_meta = Some(octa::data::DbRowMeta {
        table_name: table_name.clone(),
        schema: Some(schema.to_string()),
        row_tags: (0..t.rows.len()).map(|i| Some(i as i64)).collect(),
        original,
        original_columns: t.columns.iter().map(|c| c.name.clone()).collect(),
    });

    // Edit row id=2's name, delete row id=3, insert id=9.
    let name_col = t.columns.iter().position(|c| c.name == "name").unwrap();
    let row2 = t
        .rows
        .iter()
        .position(|r| r[0] == CellValue::Int(2))
        .unwrap();
    t.rows[row2][name_col] = CellValue::String("B".into());
    let row3 = t
        .rows
        .iter()
        .position(|r| r[0] == CellValue::Int(3))
        .unwrap();
    t.rows.remove(row3);
    t.db_meta.as_mut().unwrap().row_tags.remove(row3);
    t.rows
        .push(vec![CellValue::Int(9), CellValue::String("z".into())]);
    t.db_meta.as_mut().unwrap().row_tags.push(None);

    let plan = build_write_back_plan(&t, &identity).expect("plan");
    assert_eq!(plan.change_count(), 3);
    let report = apply_write_back(
        c.as_mut(),
        engine,
        schema,
        &table_name,
        &t.columns,
        &identity,
        &plan,
    )
    .expect("apply");
    assert_eq!((report.deleted, report.updated, report.inserted), (1, 1, 1));

    let back = c
        .query(&format!("SELECT id, name FROM {target} ORDER BY id"))
        .expect("re-query");
    assert_eq!(back.row_count(), 3);
    assert_eq!(back.rows[0][0], CellValue::Int(1));
    assert_eq!(back.rows[1][1], CellValue::String("B".into()));
    assert_eq!(back.rows[2][0], CellValue::Int(9));

    // Rollback: a plan whose insert collides with an existing PK must leave
    // the table unchanged, including the update in the same transaction.
    let mut bad = octa::db::write_back::DbWriteBackPlan::default();
    bad.updates.push((
        vec![CellValue::Int(1)],
        vec![CellValue::Int(1), CellValue::String("MUTATED".into())],
    ));
    bad.inserts
        .push(vec![CellValue::Int(9), CellValue::String("dup".into())]);
    apply_write_back(
        c.as_mut(),
        engine,
        schema,
        &table_name,
        &t.columns,
        &identity,
        &bad,
    )
    .expect_err("duplicate PK insert must fail");
    let after = c
        .query(&format!("SELECT name FROM {target} WHERE id = 1"))
        .expect("post-rollback query");
    assert_eq!(
        after.rows[0][0],
        CellValue::String("a".into()),
        "rollback must undo the update"
    );

    c.execute(&format!("DROP TABLE {target}")).expect("drop");
}

/// Held while `query_row_cap_live` lowers the process-wide row cap to 5,
/// which would otherwise truncate a parallel test's results.
static ROW_CAP: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn row_cap_lock() -> std::sync::MutexGuard<'static, ()> {
    ROW_CAP.lock().unwrap_or_else(|e| e.into_inner())
}

/// A huge SELECT must stop collecting at the initial-load row cap instead
/// of materialising every row (used to OOM-crash the app), and the
/// connection must stay usable afterwards (MySQL/MSSQL drain the remaining
/// wire packets after the early stop). One test fn for all three engines so
/// the process-wide guard is held once. The cap is process-wide, so tests
/// that read more than 5 rows in one query take [`ROW_CAP`] too.
#[test]
fn query_row_cap_live() {
    let _cap = row_cap_lock();
    let cases = [
        (
            DbEngine::Postgres,
            "OCTA_TEST_POSTGRES_URL",
            "SELECT * FROM generate_series(1, 100000)",
        ),
        (
            DbEngine::MySql,
            "OCTA_TEST_MYSQL_URL",
            // A cross join, not a recursive CTE: stock MySQL 8 defaults
            // cte_max_recursion_depth to 1000, which would abort a 10000-deep CTE
            // before the row cap is ever exercised.
            "SELECT a.table_name FROM information_schema.columns a \
             CROSS JOIN information_schema.columns b LIMIT 10000",
        ),
        (
            DbEngine::Mssql,
            "OCTA_TEST_MSSQL_URL",
            "SELECT TOP 10000 a.object_id FROM sys.objects a CROSS JOIN sys.objects b \
             CROSS JOIN sys.objects c",
        ),
    ];
    let _guard = octa::formats::InitialLoadRowsGuard::new(5);
    for (engine, env_var, big_sql) in cases {
        let Some((conn, pass)) = conn_from_env(env_var, engine) else {
            eprintln!("skipped: {env_var} not set");
            continue;
        };
        let mut c = connect(&conn, Some(&pass), None).expect("connect");
        let capped = c.query(big_sql).expect("capped query");
        assert_eq!(capped.row_count(), 5, "{engine:?} result capped");
        let again = c
            .query("SELECT 1 AS one")
            .expect("connection reusable after cap");
        assert_eq!(again.row_count(), 1, "{engine:?} second query works");
    }
}

/// A `cancel_handle()` from a second connection stops a long statement at the
/// vendor. Each engine runs a 30s no-op; the handle fires after 2s and the
/// query must return well before its natural end. Exercises `kill_sql` +
/// `kill_via_new_connection` (MySQL `KILL QUERY`, MSSQL `KILL`), the branch's
/// otherwise-untested cancellation path.
#[test]
fn cancel_running_query_live() {
    let cases = [
        (DbEngine::MySql, "OCTA_TEST_MYSQL_URL", "SELECT SLEEP(30)"),
        (
            DbEngine::Mssql,
            "OCTA_TEST_MSSQL_URL",
            "WAITFOR DELAY '00:00:30'",
        ),
    ];
    for (engine, env_var, slow_sql) in cases {
        let Some((conn, pass)) = conn_from_env(env_var, engine) else {
            eprintln!("skipped: {env_var} not set");
            continue;
        };
        let mut c = connect(&conn, Some(&pass), None).expect("connect");
        let cancel = c
            .cancel_handle()
            .unwrap_or_else(|| panic!("{engine:?} has a cancel handle"));
        let start = std::time::Instant::now();
        let runner = std::thread::spawn(move || {
            // Returns Ok or Err once the vendor kills the statement; we only
            // care that it stops promptly, not how it reports.
            let _ = c.query(slow_sql);
        });
        std::thread::sleep(std::time::Duration::from_secs(2));
        cancel();
        runner.join().expect("query thread");
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(20),
            "{engine:?} statement should be cancelled well before 30s, took {elapsed:?}"
        );
    }
}

#[test]
fn postgres_write_back_live() {
    exercise_write_back(DbEngine::Postgres, "OCTA_TEST_POSTGRES_URL", "public");
}

#[test]
fn mysql_write_back_live() {
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_MYSQL_URL", DbEngine::MySql) else {
        eprintln!("skipped: OCTA_TEST_MYSQL_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    c.execute("CREATE DATABASE IF NOT EXISTS octa_wb_db")
        .expect("create db");
    drop(c);
    exercise_write_back(DbEngine::MySql, "OCTA_TEST_MYSQL_URL", "octa_wb_db");
    let mut c = connect(&conn, Some(&pass), None).expect("reconnect");
    c.execute("DROP DATABASE octa_wb_db").expect("drop db");
}

#[test]
fn mssql_write_back_live() {
    exercise_write_back(DbEngine::Mssql, "OCTA_TEST_MSSQL_URL", "dbo");
}

#[test]
fn postgres_live() {
    exercise(
        DbEngine::Postgres,
        "OCTA_TEST_POSTGRES_URL",
        "public",
        "public",
    );
}

#[test]
fn mysql_live() {
    // MySQL "schemas" are databases, and a bare server has only the system
    // ones (which list_schemas hides on purpose) - create a real one first.
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_MYSQL_URL", DbEngine::MySql) else {
        eprintln!("skipped: OCTA_TEST_MYSQL_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    c.execute("CREATE DATABASE IF NOT EXISTS octa_test_db")
        .expect("create db");
    drop(c);
    exercise(
        DbEngine::MySql,
        "OCTA_TEST_MYSQL_URL",
        "octa_test_db",
        "octa_test_db",
    );
    let mut c = connect(&conn, Some(&pass), None).expect("reconnect");
    c.execute("DROP DATABASE octa_test_db").expect("drop db");
}

#[test]
fn mssql_live() {
    exercise(DbEngine::Mssql, "OCTA_TEST_MSSQL_URL", "dbo", "dbo");
}

/// Asks SQL Server itself whether the connection is encrypted.
///
/// The whole TDS session is, not merely the login packet: tiberius'
/// `Config::default()` sets `EncryptionLevel::Required` whenever a TLS feature
/// is compiled in, and `MssqlConnector::connect` never lowers it. The
/// `trust_cert()` call on the password branch disables certificate
/// *validation*, not encryption.
///
/// Worth having permanently, and not covered by the other MSSQL tests: a TLS
/// stack that silently degraded to plaintext would still connect, still return
/// rows, and still pass every one of them. This is the assertion that fails
/// instead. It guards the `[patch.crates-io]` tiberius fork in Cargo.toml,
/// whose whole purpose is replacing the TLS implementation underneath.
#[test]
fn mssql_session_encryption_live() {
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_MSSQL_URL", DbEngine::Mssql) else {
        eprintln!("skipped: OCTA_TEST_MSSQL_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");

    let t = c
        .query("SELECT encrypt_option FROM sys.dm_exec_connections WHERE session_id = @@SPID")
        .expect("query dm_exec_connections");
    assert_eq!(t.row_count(), 1, "no row for the current session");

    let got = t.rows[0][0].to_string();
    assert_eq!(got, "TRUE", "MSSQL session is not encrypted");
}

#[test]
fn redshift_live() {
    // Redshift rides the Postgres connector with the Redshift catalogue
    // dialect; env-gated on a real Redshift cluster URL.
    exercise(
        DbEngine::Redshift,
        "OCTA_TEST_REDSHIFT_URL",
        "public",
        "public",
    );
}

#[test]
fn clickhouse_read_roundtrip() {
    // Read-only: ClickHouse CREATE needs an ENGINE clause the generic DDL
    // doesn't emit, so skip the write-back exercise and check the read path.
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_CLICKHOUSE_URL", DbEngine::ClickHouse) else {
        eprintln!("skipped: OCTA_TEST_CLICKHOUSE_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    let one = c.query("SELECT 1 AS one").expect("select 1");
    assert_eq!(one.row_count(), 1);
    assert_eq!(one.rows[0][0], CellValue::Int(1));

    let schemas = c.list_schemas(None).expect("list schemas");
    assert!(
        schemas.iter().any(|s| s == "system"),
        "system db missing from {schemas:?}"
    );
    let tables = c.list_tables(None, "system").expect("list tables");
    assert!(tables.iter().any(|t| t == "databases"));
}

#[test]
fn exasol_read_roundtrip() {
    // The sqlx driver owns the wire protocol, so this is a read smoke test:
    // connect + SELECT 1. Env-gated on a real Exasol instance URL.
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_EXASOL_URL", DbEngine::Exasol) else {
        eprintln!("skipped: OCTA_TEST_EXASOL_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    let one = c.query("SELECT 1 AS ONE").expect("select 1");
    assert_eq!(one.row_count(), 1);
    assert_eq!(one.rows[0][0], CellValue::Int(1));
}

/// Oracle: read plus a full write round trip, because the DDL mapping (NUMBER
/// precisions, VARCHAR2 widths) and the ANSI date literals are new here and a
/// SELECT alone would exercise neither. Runs in the connecting user's own
/// schema, which is the one Oracle grants CREATE TABLE on by default.
#[test]
fn oracle_read_write_roundtrip() {
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_ORACLE_URL", DbEngine::Oracle) else {
        eprintln!("skipped: OCTA_TEST_ORACLE_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    // Pre-23c Oracle has no FROM-less SELECT.
    let one = c.query("SELECT 1 AS one FROM dual").expect("select 1");
    assert_eq!(one.row_count(), 1);
    assert_eq!(one.rows[0][0], CellValue::Int(1));

    // The catalog folds an unquoted user name to upper case.
    let schema = conn.username.to_uppercase();
    let _ = c.list_tables(None, &schema).expect("list tables");

    let table_name = format!(
        "OCTA_TEST_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let report = c
        .write_table(
            None,
            &schema,
            &table_name,
            DbWriteMode::Create,
            &sample_table(),
        )
        .expect("create + write");
    assert!(report.created);
    assert_eq!(report.rows_written, 2);
    let target = format!(
        "{}.{}",
        DbEngine::Oracle.quote_ident(&schema),
        DbEngine::Oracle.quote_ident(&table_name)
    );
    // Octa's DDL quotes every identifier, so the columns it created are
    // lower case on a server that would otherwise fold them to upper: the
    // read back has to quote them too.
    let names = c
        .query(&format!(
            "SELECT {} FROM {target} WHERE {} = 2",
            DbEngine::Oracle.quote_ident("name"),
            DbEngine::Oracle.quote_ident("id")
        ))
        .expect("select names");
    assert_eq!(names.rows.len(), 1);
    assert_eq!(names.rows[0][0], CellValue::String("o'hara".into()));
    let count = c
        .query(&format!("SELECT COUNT(*) AS n FROM {target}"))
        .expect("count");
    assert_eq!(count.rows[0][0], CellValue::Int(2));
    c.execute(&format!("DROP TABLE {target}")).expect("drop");
}

/// The Oracle-only SQL Octa generates, against a real server: the FETCH FIRST
/// sample, the OFFSET/FETCH page, the ALL_* catalogue queries that stand in
/// for `information_schema`, the PL/SQL drop behind Replace mode, and the
/// ANSI date literals. Each of these is a dialect arm no other engine
/// exercises, so a unit test can only check the text, never that Oracle
/// accepts it.
#[test]
fn oracle_dialect_paths_live() {
    use octa::db::{choose_row_key, select_sample_sql};

    let Some((conn, pass)) = conn_from_env("OCTA_TEST_ORACLE_URL", DbEngine::Oracle) else {
        eprintln!("skipped: OCTA_TEST_ORACLE_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    let schema = conn.username.to_uppercase();
    let q = |s: &str| DbEngine::Oracle.quote_ident(s);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let parent = format!("OCTA_P_{stamp}");
    let child = format!("OCTA_C_{stamp}");

    // Unquoted DDL, i.e. the upper-case names an Oracle schema really has.
    c.execute(&format!(
        "CREATE TABLE {parent} (id NUMBER(10) PRIMARY KEY, born DATE, seen TIMESTAMP)"
    ))
    .expect("create parent");
    c.execute(&format!(
        "CREATE TABLE {child} (id NUMBER(10) PRIMARY KEY, parent_id NUMBER(10)          CONSTRAINT {child}_FK REFERENCES {parent} (id))"
    ))
    .expect("create child");
    for i in 1..=7 {
        c.execute(&format!(
            "INSERT INTO {parent} (id, born, seen) VALUES              ({i}, DATE '2024-01-0{i}', TIMESTAMP '2024-01-0{i} 10:00:00')"
        ))
        .expect("seed parent");
    }
    c.execute("COMMIT").expect("commit");

    // FETCH FIRST, not LIMIT: Oracle has no LIMIT clause.
    let sample = c
        .query(&select_sample_sql(
            DbEngine::Oracle,
            None,
            &schema,
            &parent,
            3,
        ))
        .expect("sample");
    assert_eq!(sample.row_count(), 3);
    // Oracle DATE carries a time, so it reads as a timestamp, and the ANSI
    // literals above survived the round trip.
    let born = sample
        .columns
        .iter()
        .position(|c| c.name == "BORN")
        .unwrap();
    assert_eq!(
        sample.rows[0][born],
        CellValue::DateTime("2024-01-01 00:00:00".into())
    );
    let seen = sample
        .columns
        .iter()
        .position(|c| c.name == "SEEN")
        .unwrap();
    assert_eq!(
        sample.rows[0][seen],
        CellValue::DateTime("2024-01-01 10:00:00".into())
    );

    // OFFSET/FETCH paging, through the copy lane's own entry point: 7 rows
    // in pages of 3. Oracle rejects the `AS` alias every other engine takes,
    // so this is the arm that would fail with the shared text.
    let mut pages = Vec::new();
    let mut first_of_last = CellValue::Null;
    c.fetch_batches(
        &format!("SELECT id FROM {parent} ORDER BY id"),
        3,
        &mut |t| {
            pages.push(t.row_count());
            first_of_last = t.rows[0][0].clone();
            Ok(())
        },
    )
    .expect("fetch_batches");
    assert_eq!(pages, vec![3, 3, 1]);
    assert_eq!(first_of_last, CellValue::Int(7));

    // Row-key discovery out of ALL_CONSTRAINTS instead of information_schema.
    let keys = c
        .query(&octa::db::row_key_sql(
            DbEngine::Oracle,
            None,
            &schema,
            &parent,
        ))
        .expect("row key query");
    let candidates: Vec<octa::db::RowKeyCandidate> = keys
        .rows
        .iter()
        .filter_map(|r| {
            Some(octa::db::RowKeyCandidate {
                constraint_type: r.first()?.to_string(),
                constraint_name: r.get(1)?.to_string(),
                column_name: r.get(2)?.to_string(),
                nullable: r
                    .get(3)
                    .map(|v| v.to_string().eq_ignore_ascii_case("YES"))
                    .unwrap_or(true),
            })
        })
        .collect();
    assert_eq!(choose_row_key(&candidates), vec!["ID".to_string()]);

    // Column metadata for the "Show metadata..." tab.
    let meta = c
        .query(&octa::db::table_metadata_sql(
            DbEngine::Oracle,
            None,
            &schema,
            &parent,
        ))
        .expect("metadata");
    assert_eq!(meta.row_count(), 3);

    // The declared foreign key, read from ALL_CONSTRAINTS + ALL_CONS_COLUMNS.
    let (columns, fks) =
        octa::db::relationships::scan(c.as_mut(), None, std::slice::from_ref(&schema))
            .expect("scan");
    assert!(
        columns
            .iter()
            .any(|(_, t, col)| t == &child && col == "PARENT_ID")
    );
    let fk = fks
        .iter()
        .find(|f| f.child_table == child)
        .expect("the declared foreign key");
    assert_eq!(fk.parent_table, parent);
    assert_eq!(fk.child_column, "PARENT_ID");
    assert_eq!(fk.parent_column, "ID");

    // Replace mode drops through the PL/SQL wrapper: once with the table
    // there, once without, and only ORA-00942 may be swallowed.
    let data = sample_table();
    let repl = format!("OCTA_R_{stamp}");
    for _ in 0..2 {
        let report = c
            .write_table(None, &schema, &repl, DbWriteMode::Replace, &data)
            .expect("replace");
        assert!(report.created);
        assert_eq!(report.rows_written, 2);
    }

    // Every cell kind Octa can write, through the Oracle DDL mapping and the
    // Oracle literal forms, read back as what it went in as. This is the
    // round trip the docs promise: NUMBER(1) booleans, ANSI date literals,
    // and decimals as NUMBER rather than the BINARY_DOUBLE this driver
    // cannot read back.
    let mut mixed = DataTable::empty();
    mixed.columns = ["n", "x", "flag", "d", "ts", "txt"]
        .iter()
        .zip([
            "Int64",
            "Float64",
            "Boolean",
            "Date32",
            "Timestamp(Microsecond, None)",
            "Utf8",
        ])
        .map(|(name, ty)| ColumnInfo {
            name: (*name).into(),
            data_type: ty.into(),
        })
        .collect();
    mixed.rows = vec![vec![
        CellValue::Int(-7),
        CellValue::Float(1.5),
        CellValue::Bool(true),
        CellValue::Date("2024-01-31".into()),
        CellValue::DateTime("2024-01-31 10:00:00".into()),
        CellValue::String("o'hara".into()),
    ]];
    let types = format!("OCTA_T_{stamp}");
    c.write_table(None, &schema, &types, DbWriteMode::Create, &mixed)
        .expect("write every cell kind");
    let back = c
        .query(&format!(
            "SELECT {} FROM {}.{}",
            mixed
                .columns
                .iter()
                .map(|c| q(&c.name))
                .collect::<Vec<_>>()
                .join(", "),
            q(&schema),
            q(&types)
        ))
        .expect("read back");
    assert_eq!(back.rows[0][0], CellValue::Int(-7));
    assert_eq!(back.rows[0][1], CellValue::Float(1.5));
    // A boolean is NUMBER(1) on a server older than 23c, so it comes back as
    // the 1 that was written, not as true.
    assert_eq!(back.rows[0][2], CellValue::Int(1));
    assert_eq!(
        back.rows[0][3],
        CellValue::DateTime("2024-01-31 00:00:00".into())
    );
    assert_eq!(
        back.rows[0][4],
        CellValue::DateTime("2024-01-31 10:00:00".into())
    );
    assert_eq!(back.rows[0][5], CellValue::String("o'hara".into()));

    for t in [&types, &repl, &child, &parent] {
        c.execute(&format!("DROP TABLE {}.{}", q(&schema), q(t)))
            .expect("drop");
    }
}

/// Trino: the statement API end to end. A local coordinator has the `tpch`
/// catalog built in, so the catalogue listings and a real query both have
/// something to find without any setup.
///
/// `OCTA_TEST_TRINO_URL='host=http://127.0.0.1,port=8080,db=tpch,user=octa'`
/// against `docker run -p 8080:8080 trinodb/trino`; the `http://` prefix is
/// what selects plaintext, and a coordinator with no authentication takes any
/// password.
#[test]
fn trino_read_live() {
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_TRINO_URL", DbEngine::Trino) else {
        eprintln!("skipped: OCTA_TEST_TRINO_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");

    let one = c.query("SELECT 1 AS one").expect("select 1");
    assert_eq!(one.row_count(), 1);
    assert_eq!(one.rows[0][0], CellValue::Int(1));

    // Three-level: catalogs, then schemas within one, then tables.
    let catalogs = c.list_catalogs().expect("list catalogs");
    assert!(catalogs.iter().any(|c| c == "tpch"), "{catalogs:?}");
    let schemas = c.list_schemas(Some("tpch")).expect("list schemas");
    assert!(schemas.iter().any(|s| s == "tiny"), "{schemas:?}");
    let tables = c.list_tables(Some("tpch"), "tiny").expect("list tables");
    assert!(tables.iter().any(|t| t == "nation"), "{tables:?}");

    // Types: tpch.tiny.nation is (bigint, varchar, bigint, varchar).
    let nation = c
        .query("SELECT nationkey, name FROM tpch.tiny.nation ORDER BY nationkey LIMIT 3")
        .expect("query nation");
    assert_eq!(nation.row_count(), 3);
    assert_eq!(nation.columns[0].data_type, "Int64");
    assert_eq!(nation.columns[1].data_type, "Utf8");
    assert_eq!(nation.rows[0][0], CellValue::Int(0));
    assert_eq!(nation.rows[0][1], CellValue::String("ALGERIA".into()));

    // Paging through the copy lane: 25 nations in pages of 10. Trino takes
    // the standard LIMIT/OFFSET form, which is what `paged_sql` emits.
    let mut pages = Vec::new();
    c.fetch_batches(
        "SELECT nationkey FROM tpch.tiny.nation ORDER BY nationkey",
        10,
        &mut |t| {
            pages.push(t.row_count());
            Ok(())
        },
    )
    .expect("fetch_batches");
    assert_eq!(pages, vec![10, 10, 5]);

    // A failed statement must report Trino's own message, which arrives in
    // the result body rather than as an HTTP status.
    let err = c
        .query("SELECT nope FROM tpch.tiny.nation")
        .expect_err("a bad column must fail");
    let text = format!("{err:#}");
    assert!(text.contains("COLUMN_NOT_FOUND"), "{text}");
    // And the connection is still usable afterwards.
    assert_eq!(c.query("SELECT 2 AS two").expect("reuse").row_count(), 1);
}

/// Athena, against a real AWS account. Unlike the other engines there is no
/// container to run it in, so this one only ever runs where someone points it
/// at an account:
/// `OCTA_TEST_ATHENA_URL='host=athena.eu-central-1.amazonaws.com;db=default'`
/// with credentials in the environment or the aws CLI, and a workgroup that
/// sets its own result location (or `athena_output_location` on the saved
/// connection).
#[test]
fn athena_read_live() {
    let Some((mut conn, _)) = conn_from_env("OCTA_TEST_ATHENA_URL", DbEngine::Athena) else {
        eprintln!("skipped: OCTA_TEST_ATHENA_URL not set");
        return;
    };
    // Athena signs every request; there is no password to resolve, and the
    // region rides on the auth mode.
    conn.auth = DbAuth::AwsIam {
        region: std::env::var("AWS_REGION").ok(),
        sso_start_url: None,
        sso_region: None,
        sso_account_id: None,
        sso_role: None,
    };
    conn.athena_output_location = std::env::var("OCTA_TEST_ATHENA_OUTPUT").ok();
    let mut c = connect(&conn, None, None).expect("connect");

    let one = c.query("SELECT 1 AS one").expect("select 1");
    assert_eq!(one.row_count(), 1);
    assert_eq!(one.rows[0][0], CellValue::Int(1));

    // The Glue catalogue answers the sidebar's two levels.
    let schemas = c.list_schemas(None).expect("list databases");
    assert!(!schemas.is_empty(), "no Glue databases visible");
    let _ = c.list_tables(None, &schemas[0]).expect("list tables");
}

#[test]
fn snowflake_read_roundtrip() {
    // Read-only smoke test over the SQL API v2 (auth via the connection's
    // configured mode). Env-gated on a real Snowflake account URL.
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_SNOWFLAKE_URL", DbEngine::Snowflake) else {
        eprintln!("skipped: OCTA_TEST_SNOWFLAKE_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    let one = c.query("SELECT 1 AS ONE").expect("select 1");
    assert_eq!(one.row_count(), 1);
    assert_eq!(one.rows[0][0], CellValue::Int(1));
}

#[test]
fn databricks_read_roundtrip() {
    // Read-only smoke test over the Statement Execution API. The `db` field of
    // the URL is the SQL warehouse id. Env-gated on a real workspace URL.
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_DATABRICKS_URL", DbEngine::Databricks) else {
        eprintln!("skipped: OCTA_TEST_DATABRICKS_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    let one = c.query("SELECT 1 AS one").expect("select 1");
    assert_eq!(one.row_count(), 1);
    assert_eq!(one.rows[0][0], CellValue::Int(1));
}

#[test]
fn bigquery_read_roundtrip() {
    // Read-only smoke test over the REST API. The `db` field of the URL is the
    // GCP project id; auth via the connection's ADC / service-account mode.
    // Env-gated on OCTA_TEST_BIGQUERY_URL.
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_BIGQUERY_URL", DbEngine::BigQuery) else {
        eprintln!("skipped: OCTA_TEST_BIGQUERY_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    let one = c.query("SELECT 1 AS one").expect("select 1");
    assert_eq!(one.row_count(), 1);
    assert_eq!(one.rows[0][0], CellValue::Int(1));
}

/// Live server-to-server copy MySQL -> Postgres through DuckDB (Create,
/// Append, Replace), asserting row counts and values on the Postgres side.
/// Needs BOTH env vars; installs the DuckDB postgres+mysql extensions over
/// the network on first run.
#[test]
fn mysql_to_postgres_copy_live() {
    use octa::db::copy::{DbCopyEnd, copy_table};

    let Some((my_conn, my_pass)) = conn_from_env("OCTA_TEST_MYSQL_URL", DbEngine::MySql) else {
        eprintln!("skipped: OCTA_TEST_MYSQL_URL not set");
        return;
    };
    let Some((mut pg_conn, pg_pass)) = conn_from_env("OCTA_TEST_POSTGRES_URL", DbEngine::Postgres)
    else {
        eprintln!("skipped: OCTA_TEST_POSTGRES_URL not set");
        return;
    };

    // Seed a source table in MySQL.
    let mut my = connect(&my_conn, Some(&my_pass), None).expect("connect mysql");
    my.execute("CREATE DATABASE IF NOT EXISTS octa_copy_db")
        .expect("create db");
    my.execute("DROP TABLE IF EXISTS octa_copy_db.people")
        .expect("pre-clean");
    my.execute("CREATE TABLE octa_copy_db.people (id BIGINT PRIMARY KEY, name VARCHAR(50))")
        .expect("create source");
    my.execute("INSERT INTO octa_copy_db.people VALUES (1, 'ada'), (2, 'o''hara'), (3, 'zoe')")
        .expect("seed");

    let source = DbCopyEnd {
        ssh_secret: None,
        conn: my_conn.clone(),
        catalog: None,
        schema: "octa_copy_db".into(),
        table: "people".into(),
    };
    let target = DbCopyEnd {
        ssh_secret: None,
        conn: pg_conn.clone(),
        catalog: None,
        schema: "public".into(),
        table: "octa_copied_people".into(),
    };

    // Create: table appears on Postgres with all rows.
    let report = copy_table(
        &source,
        Some(&my_pass),
        &target,
        Some(&pg_pass),
        DbWriteMode::Create,
        &|_| {},
    )
    .expect("create copy");
    assert_eq!(report.rows_copied, 3);
    assert!(report.created);

    let mut pg = connect(&pg_conn, Some(&pg_pass), None).expect("connect pg");
    let back = pg
        .query("SELECT id, name FROM public.octa_copied_people ORDER BY id")
        .expect("read back");
    assert_eq!(back.row_count(), 3);
    assert_eq!(back.rows[1][1], CellValue::String("o'hara".into()));

    // Append doubles the rows; Replace brings it back to the source count.
    let report = copy_table(
        &source,
        Some(&my_pass),
        &target,
        Some(&pg_pass),
        DbWriteMode::Append,
        &|_| {},
    )
    .expect("append copy");
    assert_eq!(report.rows_copied, 3);
    let n = pg
        .query("SELECT COUNT(*) FROM public.octa_copied_people")
        .expect("count");
    assert_eq!(n.rows[0][0], CellValue::Int(6));

    copy_table(
        &source,
        Some(&my_pass),
        &target,
        Some(&pg_pass),
        DbWriteMode::Replace,
        &|_| {},
    )
    .expect("replace copy");
    let n = pg
        .query("SELECT COUNT(*) FROM public.octa_copied_people")
        .expect("recount");
    assert_eq!(n.rows[0][0], CellValue::Int(3));

    // The target's write gate is enforced.
    pg_conn.allow_writes = false;
    let gated = DbCopyEnd {
        ssh_secret: None,
        conn: pg_conn,
        ..target.clone()
    };
    let err = copy_table(
        &source,
        Some(&my_pass),
        &gated,
        Some(&pg_pass),
        DbWriteMode::Replace,
        &|_| {},
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("Allow writes"), "{err}");

    pg.execute("DROP TABLE public.octa_copied_people")
        .expect("drop pg");
    my.execute("DROP DATABASE octa_copy_db")
        .expect("drop mysql");
}

/// A file compared against a live Postgres table, through the same
/// `compare_join` engine the file-vs-file diff uses. Seeds three rows, then
/// builds a "file" side with one changed row, one dropped and one added, and
/// asserts the diff reports exactly that.
#[test]
fn file_vs_postgres_table_diff_live() {
    let Some((conn, secret)) = conn_from_env("OCTA_TEST_POSTGRES_URL", DbEngine::Postgres) else {
        println!("skipped: OCTA_TEST_POSTGRES_URL");
        return;
    };
    let mut c = connect(&conn, Some(&secret), None).expect("connect");
    c.execute("DROP TABLE IF EXISTS octa_diff_live").unwrap();
    c.execute("CREATE TABLE octa_diff_live (id INT PRIMARY KEY, name TEXT)")
        .unwrap();
    c.execute("INSERT INTO octa_diff_live VALUES (1,'a'),(2,'b'),(3,'c')")
        .unwrap();

    let db_side = octa::db::fetch_table::fetch_table(
        &conn,
        Some(&secret),
        None,
        None,
        "public",
        "octa_diff_live",
    )
    .expect("fetch_table");
    assert_eq!(db_side.row_count(), 3, "seeded rows must come back");

    // The "file" side: row 2 changed, row 3 gone, row 4 new.
    let mut file_side = db_side.clone();
    file_side.rows[1][1] = CellValue::String("B".into());
    file_side.rows.retain(|r| r[0] != CellValue::Int(3));
    file_side
        .rows
        .push(vec![CellValue::Int(4), CellValue::String("d".into())]);

    let result = octa::data::compare::compare_join(&file_side, &db_side, &["id".to_string()])
        .expect("compare_join");
    assert_eq!(result.changed.len(), 1, "one changed row");
    assert_eq!(result.only_in_a.len(), 1, "id 4 is file-only");
    assert_eq!(result.only_in_b.len(), 1, "id 3 is database-only");

    c.execute("DROP TABLE octa_diff_live").unwrap();
}

/// An unqualified table name falls back to the connection's own database
/// rather than guessing a schema.
#[test]
fn fetch_table_defaults_the_schema_live() {
    let Some((conn, secret)) = conn_from_env("OCTA_TEST_MYSQL_URL", DbEngine::MySql) else {
        println!("skipped: OCTA_TEST_MYSQL_URL");
        return;
    };
    let mut c = connect(&conn, Some(&secret), None).expect("connect");
    c.execute("DROP TABLE IF EXISTS octa_fetch_live").unwrap();
    c.execute("CREATE TABLE octa_fetch_live (id INT PRIMARY KEY)")
        .unwrap();
    c.execute("INSERT INTO octa_fetch_live VALUES (1),(2)")
        .unwrap();

    let t =
        octa::db::fetch_table::fetch_table(&conn, Some(&secret), None, None, "", "octa_fetch_live")
            .expect("fetch_table with an empty schema");
    assert_eq!(t.row_count(), 2);

    c.execute("DROP TABLE octa_fetch_live").unwrap();
}

/// Postgres `numeric` must survive the round trip exactly.
///
/// Regression test for a silent data-loss bug: tokio-postgres has no
/// `FromSql<String>` for NUMERIC, the generic fallback swallowed the error,
/// and every decimal column read as NULL. Because a database tab writes back
/// full rows, saving any edit then replaced real values on the server with
/// NULL. Money columns are the common case, so this asserts exact text, not
/// an approximation: `99.50` must not come back as `99.5`.
#[test]
fn postgres_numeric_round_trip_live() {
    let env_var = "OCTA_TEST_POSTGRES_URL";
    let Some((conn, secret)) = conn_from_env(env_var, DbEngine::Postgres) else {
        eprintln!("skipped: {env_var} not set");
        return;
    };
    let mut c = connect(&conn, Some(&secret), None).expect("connect");

    c.execute("DROP TABLE IF EXISTS octa_numeric_probe").ok();
    c.execute(
        "CREATE TABLE octa_numeric_probe (\
             id int PRIMARY KEY, money numeric(12,2), tiny numeric, \
             big numeric, neg numeric(10,4), nul numeric)",
    )
    .expect("create probe table");
    c.execute(
        "INSERT INTO octa_numeric_probe VALUES \
         (1, 99.50, 0.05, 10000, -1234.5678, NULL), \
         (2, 120.50, 0.000005, 123456789, -0.0001, NULL), \
         (3, 0.00, 12345678.87654321, 1, 0.0000, NULL)",
    )
    .expect("insert probe rows");

    let got = c
        .query("SELECT id, money, tiny, big, neg, nul FROM octa_numeric_probe ORDER BY id")
        .expect("select");
    c.execute("DROP TABLE octa_numeric_probe").ok();

    let cell = |row: usize, col: usize| -> String {
        got.get(row, col).map(|v| v.to_string()).unwrap_or_default()
    };

    assert_eq!(got.row_count(), 3, "expected three probe rows");
    // Trailing zeros are part of the declared scale and must not be trimmed.
    assert_eq!(cell(0, 1), "99.50");
    assert_eq!(cell(1, 1), "120.50");
    assert_eq!(cell(2, 1), "0.00");
    // Values below one exercise a negative weight in the wire format.
    assert_eq!(cell(0, 2), "0.05");
    assert_eq!(cell(1, 2), "0.000005");
    // More than one base-10000 group, integer and fractional.
    assert_eq!(cell(2, 2), "12345678.87654321");
    assert_eq!(cell(0, 3), "10000");
    assert_eq!(cell(1, 3), "123456789");
    // Negatives keep their sign and scale.
    assert_eq!(cell(0, 4), "-1234.5678");
    assert_eq!(cell(1, 4), "-0.0001");
    assert_eq!(cell(2, 4), "0.0000");
    // A real NULL must still read as NULL, not as an empty decimal.
    assert!(
        matches!(got.get(0, 5), Some(CellValue::Null) | None),
        "NULL numeric should stay NULL, got {:?}",
        got.get(0, 5)
    );
}

/// A statement that keeps the server silent for longer than 30 s still
/// returns. tiberius 0.13 added a 30 s `command_timeout` by default; the
/// connector turns it off, because a big sort or aggregate routinely goes
/// that long without a token and the user already has Cancel.
#[test]
fn mssql_long_query_no_timeout_live() {
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_MSSQL_URL", DbEngine::Mssql) else {
        eprintln!("skipped: OCTA_TEST_MSSQL_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    let t = c
        .query("WAITFOR DELAY '00:00:35'; SELECT 1 AS one")
        .expect("a 35 s statement must not time out");
    assert_eq!(t.row_count(), 1);
}

/// 30 rows covering what the server path special-cases: an id, a float, a
/// Boolean (correlated as 1/0), a nullable integer with one far outlier, and
/// nullable lowercase text (no collation differences, one clear mode).
fn pushdown_table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = [
        ("id", "Int64"),
        ("price", "Float64"),
        ("flag", "Boolean"),
        ("qty", "Int64"),
        ("name", "Utf8"),
    ]
    .iter()
    .map(|(n, d)| ColumnInfo {
        name: (*n).into(),
        data_type: (*d).into(),
    })
    .collect();
    let names = ["ada", "bob", "cy", "dee", "ada", "eve"];
    t.rows = (0..30i64)
        .map(|i| {
            vec![
                CellValue::Int(i),
                CellValue::Float((i * 37 % 23) as f64 * 1.25 + 0.5),
                CellValue::Bool(i % 3 != 0),
                match i {
                    _ if i % 7 == 2 => CellValue::Null,
                    29 => CellValue::Int(10_000),
                    _ => CellValue::Int(i % 5 + i / 4),
                },
                if i % 9 == 4 {
                    CellValue::Null
                } else {
                    CellValue::String(names[(i % 6) as usize].into())
                },
            ]
        })
        .collect();
    t
}

fn assert_cells_match(what: &str, a: &CellValue, b: &CellValue) {
    let f = |v: &CellValue| v.to_string().trim().parse::<f64>().ok();
    match (f(a), f(b)) {
        (Some(x), Some(y)) => assert!(
            (x - y).abs() <= 1e-9 * x.abs().max(1.0),
            "{what}: {x} vs {y}"
        ),
        _ => assert_eq!(a.to_string(), b.to_string(), "{what}"),
    }
}

/// Server Summary, Data quality, Value frequency and Correlation against
/// the in-memory engines over the same seeded rows. Runs in the db-live CI
/// job for Postgres and MySQL; a no-op without the env var.
fn pushdown_parity(engine: DbEngine, env_var: &str, schema: &str) {
    use octa::data::summary::SummaryStat;
    use octa::db::pushdown as p;
    let _cap = row_cap_lock();

    let Some((conn, secret)) = conn_from_env(env_var, engine) else {
        eprintln!("skipped: {env_var} not set");
        return;
    };
    let mut c = connect(&conn, Some(&secret), None).expect("connect");
    let data = pushdown_table();
    let target = format!(
        "{}.{}",
        engine.quote_ident(schema),
        engine.quote_ident("octa_pushdown")
    );
    c.execute(&format!("DROP TABLE IF EXISTS {target}")).ok();
    c.write_table(None, schema, "octa_pushdown", DbWriteMode::Create, &data)
        .expect("seed");
    let src = p::ServerSource {
        conn: conn.clone(),
        catalog: None,
        schema: schema.into(),
        table: "octa_pushdown".into(),
        filter: None,
        derived: Vec::new(),
    };
    let stop = std::sync::atomic::AtomicBool::new(false);
    let total = p::count_rows(c.as_mut(), &src, &stop).unwrap();
    assert_eq!(total, data.row_count());
    let flag = 2;

    // Summary: every statistic both paths compute exactly. The Boolean's
    // value statistics are text on the server ("true" on Postgres, "1" on
    // MySQL), so only its counts are compared.
    let exact = [
        SummaryStat::Min,
        SummaryStat::Max,
        SummaryStat::Sum,
        SummaryStat::Mean,
        SummaryStat::Std,
        SummaryStat::Median,
        SummaryStat::Q25,
        SummaryStat::Q75,
        SummaryStat::Mode,
        SummaryStat::ModeCount,
        SummaryStat::NotNullCount,
        SummaryStat::NullCount,
        SummaryStat::NullPercent,
        SummaryStat::UniqueCount,
        SummaryStat::DistinctRatio,
        SummaryStat::TextLenMin,
        SummaryStat::TextLenMax,
        SummaryStat::TotalRows,
    ];
    let counts_only = [
        "not_null",
        "null_count",
        "null_percent",
        "unique_count",
        "distinct_ratio",
        "total_rows",
    ];
    let local = octa::data::summary::build_summary_table(&data, &exact).unwrap();
    let (server, _) = p::summary::run(c.as_mut(), &src, &data, total, &exact, &stop).unwrap();
    assert_eq!(server.columns.len(), local.columns.len());
    for r in 0..local.row_count() {
        // Column 0 is the name, 1 the type (BIGINT in memory, Int64 here).
        for col in 2..local.col_count() {
            let id = local.columns[col].name.as_str();
            // The in-memory quartiles are approximate; the server's
            // interpolate (`quartiles_interpolate_on_the_server` pins them).
            if matches!(id, "median" | "q25" | "q75") || (r == flag && !counts_only.contains(&id)) {
                continue;
            }
            assert_cells_match(
                &format!("summary {} {id}", local.get(r, 0).unwrap()),
                local.get(r, col).unwrap(),
                server.get(r, col).unwrap(),
            );
        }
    }

    // Data quality: the server-counted columns.
    let want = octa::data::quality::build_quality_report(&data).unwrap();
    let (got, local_parts) = p::quality::run(c.as_mut(), &src, &data, total, &stop).unwrap();
    assert!(!local_parts.by_design.is_empty());
    let ids = octa::data::quality::quality_column_ids();
    for id in [
        "null_percentage",
        "distinct_ratio",
        "outlier_count",
        "score",
    ] {
        let k = ids.iter().position(|x| *x == id).unwrap();
        for r in 0..data.col_count() {
            assert_cells_match(
                &format!("quality {} {id}", data.columns[r].name),
                want.table.get(r, k).unwrap(),
                got.table.get(r, k).unwrap(),
            );
        }
    }

    for col in 0..data.col_count() {
        let local = octa::data::value_frequency::compute_value_frequency(
            &data,
            col,
            None,
            Default::default(),
        )
        .unwrap();
        let server = p::value_frequency::run(
            c.as_mut(),
            &src,
            &data.columns[col],
            None,
            Default::default(),
            &stop,
        )
        .unwrap();
        assert_eq!(
            (local.nulls, local.total_non_null, local.unique_count),
            (server.nulls, server.total_non_null, server.unique_count),
            "column {col}"
        );
    }

    // Correlation over the columns the app picks (the text one stays out:
    // Postgres refuses CAST('ada' AS DOUBLE PRECISION)), Boolean included.
    let cols: Vec<ColumnInfo> = octa::data::correlation::numeric_columns(&data)
        .into_iter()
        .map(|i| data.columns[i].clone())
        .collect();
    assert_eq!(cols.len(), 4);
    for method in [
        octa::data::correlation::CorrMethod::Pearson,
        octa::data::correlation::CorrMethod::Spearman,
    ] {
        let want = octa::data::correlation::correlation_matrix(&data, method);
        let m = p::correlation::run(c.as_mut(), &src, &cols, method, &stop).unwrap();
        assert_eq!(m.columns, want.columns);
        for i in 0..cols.len() {
            for j in 0..cols.len() {
                match (want.matrix[i][j], m.matrix[i][j]) {
                    (Some(x), Some(y)) => {
                        assert!((x - y).abs() < 1e-9, "{method:?} [{i}][{j}] {x} vs {y}")
                    }
                    (x, y) => assert_eq!(x, y, "{method:?} [{i}][{j}]"),
                }
            }
        }
    }
    c.execute(&format!("DROP TABLE {target}")).ok();
}

#[test]
fn postgres_pushdown_parity_live() {
    pushdown_parity(DbEngine::Postgres, "OCTA_TEST_POSTGRES_URL", "public");
}

#[test]
fn mysql_pushdown_parity_live() {
    // MySQL "schemas" are databases: create a real one, as mysql_live does.
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_MYSQL_URL", DbEngine::MySql) else {
        eprintln!("skipped: OCTA_TEST_MYSQL_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    c.execute("CREATE DATABASE IF NOT EXISTS octa_pushdown_db")
        .expect("create db");
    drop(c);
    pushdown_parity(DbEngine::MySql, "OCTA_TEST_MYSQL_URL", "octa_pushdown_db");
    let mut c = connect(&conn, Some(&pass), None).expect("reconnect");
    c.execute("DROP DATABASE octa_pushdown_db")
        .expect("drop db");
}

/// Join key finder, relationship Measure, Join diagnostics (regex fixes
/// included), Find lookup tables and its row fetch, and the row count, on
/// the server against the in-memory engines, over the same rows.
/// Keys are lowercase with no padding, so MySQL's case-insensitive
/// collation cannot make the two paths disagree.
fn key_pushdown_parity(engine: DbEngine, env_var: &str, schema: &str) {
    use octa::db::pushdown as p;
    let _cap = row_cap_lock();
    let Some((conn, secret)) = conn_from_env(env_var, engine) else {
        eprintln!("skipped: {env_var} not set");
        return;
    };
    let mut c = connect(&conn, Some(&secret), None).expect("connect");
    let text_col = |n: &str| ColumnInfo {
        name: n.into(),
        data_type: "Utf8".into(),
    };
    let s = |v: &str| CellValue::String(v.into());
    let mut customers = DataTable::empty();
    customers.columns = vec![text_col("id"), text_col("city")];
    customers.rows = (1..=6)
        .map(|i| vec![s(&format!("{i:03}")), s(["x", "y"][i % 2])])
        .collect();
    let mut orders = DataTable::empty();
    orders.columns = vec![text_col("cust"), text_col("city"), text_col("note")];
    orders.rows = (0..40)
        .map(|i| {
            let cust = 1 + i % 8; // 7 and 8 have no customer
            // 3 and 6 only ever appear unpadded, so ignoring leading zeros
            // is a fix; the last row gives 008 a second city, so it breaks
            // the cust -> city lookup.
            vec![
                s(&if cust % 3 == 0 {
                    cust.to_string()
                } else {
                    format!("{cust:03}")
                }),
                s(if i == 39 { "z" } else { ["x", "y"][cust % 2] }),
                s(&format!("n{}", i % 3)),
            ]
        })
        .collect();
    let src = |t: &str| p::ServerSource {
        conn: conn.clone(),
        catalog: None,
        schema: schema.into(),
        table: t.into(),
        filter: None,
        derived: Vec::new(),
    };
    for (name, t) in [
        ("octa_pd_customers", &customers),
        ("octa_pd_orders", &orders),
    ] {
        c.execute(&format!(
            "DROP TABLE IF EXISTS {}.{}",
            engine.quote_ident(schema),
            engine.quote_ident(name)
        ))
        .ok();
        c.write_table(None, schema, name, DbWriteMode::Create, t)
            .expect("seed");
    }
    let stop = std::sync::atomic::AtomicBool::new(false);

    let want = octa::data::join_keys::suggest_keys(&[&orders, &customers], usize::MAX);
    assert!(!want.is_empty(), "the fixture must yield key candidates");
    let tables = vec![
        (src("octa_pd_orders"), orders.columns.clone()),
        (src("octa_pd_customers"), customers.columns.clone()),
    ];
    assert_eq!(p::join_keys::run(c.as_mut(), &tables, &stop).unwrap(), want);

    let named = vec![
        ("orders".to_string(), orders.clone()),
        ("customers".to_string(), customers.clone()),
    ];
    let refs: Vec<(String, &DataTable)> = named.iter().map(|(n, t)| (n.clone(), t)).collect();
    let mut want_map = octa::data::rel_map::build_map(
        &refs,
        &octa::data::rel_map::RelMapOptions {
            min_score: 0.0,
            ..Default::default()
        },
    );
    assert!(
        !want_map.edges.is_empty(),
        "the map must have lines to measure"
    );
    let mut got_map = want_map.clone();
    octa::data::rel_map::score_edges(&named, &mut want_map, usize::MAX);
    let froms = vec![
        src("octa_pd_orders").from_sql(),
        src("octa_pd_customers").from_sql(),
    ];
    p::rel_measure::run(c.as_mut(), engine, &froms, &mut got_map, &stop).unwrap();
    assert_eq!(got_map, want_map);

    let mut want_diag = octa::data::join_diag::diagnose(&orders, 0, &customers, 0, usize::MAX);
    let (got_diag, not_checked) = p::join_diag::run(
        c.as_mut(),
        &src("octa_pd_orders"),
        "cust",
        &src("octa_pd_customers"),
        "id",
        &stop,
    )
    .unwrap();
    want_diag.fixes.retain(|f| !not_checked.contains(&f.kind));
    assert!(!want_diag.fixes.is_empty(), "leading zeros must be a fix");
    assert_eq!(got_diag, want_diag);

    let want_lk = octa::data::lookups::find_lookups(&orders, 0.95, &stop);
    assert!(!want_lk.is_empty(), "cust -> city must be a lookup");
    let got_lk = p::lookups::run(
        c.as_mut(),
        &src("octa_pd_orders"),
        &orders.columns,
        orders.row_count(),
        0.95,
        &stop,
    )
    .unwrap();
    assert_eq!(got_lk, want_lk);

    let deps = ["city".to_string()];
    let (got_b, _) = p::lookups::breaking_rows(
        c.as_mut(),
        &src("octa_pd_orders"),
        "cust",
        &deps,
        usize::MAX,
        &stop,
    )
    .unwrap();
    let want_b = octa::data::lookups::breaking_rows(&orders, 0, &[1]).len();
    assert!(want_b > 0, "008 has two cities");
    assert_eq!(got_b.row_count(), want_b);

    // A fresh table may have no statistics yet; then it is counted exactly.
    let rc = p::row_estimate::row_count(c.as_mut(), &src("octa_pd_orders"), &stop).unwrap();
    assert!(rc.estimate || rc.rows == orders.row_count(), "{rc:?}");

    for name in ["octa_pd_customers", "octa_pd_orders"] {
        c.execute(&format!(
            "DROP TABLE {}.{}",
            engine.quote_ident(schema),
            engine.quote_ident(name)
        ))
        .ok();
    }
}

#[test]
fn postgres_key_pushdown_parity_live() {
    key_pushdown_parity(DbEngine::Postgres, "OCTA_TEST_POSTGRES_URL", "public");
}

#[test]
fn mysql_key_pushdown_parity_live() {
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_MYSQL_URL", DbEngine::MySql) else {
        eprintln!("skipped: OCTA_TEST_MYSQL_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    c.execute("CREATE DATABASE IF NOT EXISTS octa_pd_keys_db")
        .expect("create db");
    drop(c);
    key_pushdown_parity(DbEngine::MySql, "OCTA_TEST_MYSQL_URL", "octa_pd_keys_db");
    let mut c = connect(&conn, Some(&pass), None).expect("reconnect");
    c.execute("DROP DATABASE octa_pd_keys_db").expect("drop db");
}

/// Sort, filters, the value list and the exact sample on the server against
/// the in-memory engines, over the same rows.
fn view_pushdown_parity(engine: DbEngine, env_var: &str, schema: &str) {
    use octa::data::conditional_format::CondOp;
    use octa::data::predicate_filter::{PredicateFilter, row_passes};
    use octa::data::search::RowMatcher;
    use octa::db::pushdown as p;
    use octa::db::pushdown::view::{ServerView, SortKey, ViewFilter, page_sql};
    let _cap = row_cap_lock();
    let Some((conn, secret)) = conn_from_env(env_var, engine) else {
        eprintln!("skipped: {env_var} not set");
        return;
    };
    let mut c = connect(&conn, Some(&secret), None).expect("connect");
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "n".into(),
            data_type: "Int64".into(),
        },
    ];
    // "APPLE" beside "Apple": MySQL's default collation folds case, which
    // must not merge them in the value list.
    let names = [
        "Apple",
        "apple pie",
        "Banana",
        "",
        "50% off",
        "a_b",
        "Cherry",
        "O'Brien",
        "APPLE",
    ];
    t.rows = (0..400)
        .map(|i| {
            let name = if i % 11 == 4 {
                CellValue::Null
            } else {
                CellValue::String(names[i % names.len()].into())
            };
            let n = if i % 7 == 3 {
                CellValue::Null
            } else {
                CellValue::Int((i as i64 * 37) % 101 - 50)
            };
            vec![CellValue::Int(i as i64), name, n]
        })
        .collect();
    let name = "octa_view_parity";
    c.execute(&format!(
        "DROP TABLE IF EXISTS {}.{}",
        engine.quote_ident(schema),
        engine.quote_ident(name)
    ))
    .ok();
    c.write_table(None, schema, name, DbWriteMode::Create, &t)
        .expect("seed");
    let src = p::ServerSource {
        conn: conn.clone(),
        catalog: None,
        schema: schema.into(),
        table: name.into(),
        filter: None,
        derived: Vec::new(),
    };
    let ids = |table: &DataTable| -> Vec<String> {
        (0..table.row_count())
            .map(|r| table.get(r, 0).map(|v| v.to_string()).unwrap_or_default())
            .collect()
    };
    let server = |c: &mut Box<dyn octa::db::DbConnector>, view: &ServerView| -> Vec<String> {
        ids(&c
            .query(&page_sql(
                engine,
                view,
                &src.table_sql(),
                &["id".into()],
                1000,
                0,
            ))
            .unwrap())
    };
    let local = |keep: &dyn Fn(usize) -> bool| -> Vec<String> {
        (0..t.row_count())
            .filter(|&r| keep(r))
            .map(|r| t.get(r, 0).unwrap().to_string())
            .collect()
    };
    let sorted = |mut v: Vec<String>| {
        v.sort();
        v
    };

    // Value filters, comparisons and both search modes.
    let allowed = [
        "apple".to_string(),
        String::new(),
        "O'Brien".to_string(),
        "APPLE".to_string(),
    ];
    let v = ServerView {
        order: vec![],
        filters: vec![ViewFilter::values("name", allowed.clone())],
        derived: Vec::new(),
    };
    assert_eq!(
        sorted(server(&mut c, &v)),
        sorted(local(
            &|r| allowed.contains(&t.get(r, 1).unwrap().to_string())
        ))
    );
    for (col, ty, op, value) in [
        (1, "Utf8", CondOp::Eq, "apple"),
        (1, "Utf8", CondOp::Contains, "%"),
        (1, "Utf8", CondOp::NotContains, "an"),
        (2, "Int64", CondOp::Lt, "0"),
        (2, "Int64", CondOp::Ge, "25"),
        (1, "Utf8", CondOp::Empty, ""),
    ] {
        let pf = PredicateFilter {
            col,
            op,
            value: value.into(),
            case_sensitive: false,
        };
        let f = ViewFilter::compare(&t.columns[col].name, ty, op, value, false).unwrap();
        let v = ServerView {
            order: vec![],
            filters: vec![f],
            derived: Vec::new(),
        };
        assert_eq!(
            sorted(server(&mut c, &v)),
            sorted(local(&|r| row_passes(std::slice::from_ref(&pf), &t, r))),
            "{op:?} {value:?}"
        );
    }
    for (wild, q) in [(false, "APP"), (false, "'b"), (true, "a*e"), (true, "a_b")] {
        let mode = if wild {
            octa::data::SearchMode::Wildcard
        } else {
            octa::data::SearchMode::Plain
        };
        let m = RowMatcher::with_options(q, mode, false, false);
        let cols = vec!["name".to_string(), "n".to_string()];
        let f = if wild {
            ViewFilter::Wildcard {
                columns: cols,
                pattern: q.into(),
                case_sensitive: false,
            }
        } else {
            ViewFilter::Contains {
                columns: cols,
                needle: q.into(),
                case_sensitive: false,
            }
        };
        let v = ServerView {
            order: vec![],
            filters: vec![f],
            derived: Vec::new(),
        };
        assert_eq!(
            sorted(server(&mut c, &v)),
            sorted(local(&|r| [1, 2]
                .iter()
                .any(|&k| m.matches(&t.get(r, k).unwrap().to_string())))),
            "search {q:?}"
        );
    }

    // The sort, with `id` as the tie-break: Octa's stable sort keeps id order.
    for keys in [
        vec![(1usize, true)],
        vec![(1, false)],
        vec![(2, true), (1, false)],
    ] {
        let mut local_sorted = t.clone();
        local_sorted.sort_rows_by_columns(&keys);
        let order = keys
            .iter()
            .map(|&(k, asc)| SortKey {
                column: t.columns[k].name.clone(),
                ascending: asc,
                text: p::view::is_text_type(&t.columns[k].data_type),
            })
            .collect();
        let v = ServerView {
            order,
            filters: vec![],
            derived: Vec::new(),
        };
        assert_eq!(server(&mut c, &v), ids(&local_sorted), "{keys:?}");
    }

    // The value list (counted by exact text), and the exact sample.
    let stop = std::sync::atomic::AtomicBool::new(false);
    let want = octa::data::value_frequency::compute_value_frequency(
        &t,
        1,
        None,
        octa::data::value_frequency::BinningMode::None,
    )
    .unwrap();
    let got = p::facets::run(c.as_mut(), &src, &t.columns[1], "", 50, &stop).unwrap();
    assert_eq!(got.unique_count, want.unique_count, "APPLE and Apple apart");
    assert_eq!(got.nulls, want.nulls);
    let label_counts = |rows: &[octa::data::value_frequency::ValueFrequencyRow]| {
        let mut v: Vec<(String, usize)> = rows.iter().map(|r| (r.label.clone(), r.count)).collect();
        v.sort();
        v
    };
    assert_eq!(label_counts(&got.rows), label_counts(&want.rows));
    let (s, capped, _) = p::sample::run(
        c.as_mut(),
        &src,
        25,
        p::sample::SampleMethod::Exact,
        1_000,
        &stop,
    )
    .unwrap();
    assert!(!capped);
    let mut picked = ids(&s);
    picked.sort();
    picked.dedup();
    assert_eq!(picked.len(), 25, "25 distinct rows");
    // Fast runs: block sampling where the catalog has a figure, else exact.
    let (f, _, ran) = p::sample::run(
        c.as_mut(),
        &src,
        10,
        p::sample::SampleMethod::Fast,
        1_000,
        &stop,
    )
    .unwrap();
    assert!(f.row_count() <= 10);
    if engine == DbEngine::MySql {
        assert_eq!(
            ran,
            p::sample::SampleMethod::Exact,
            "MySQL has no block sampling"
        );
    }

    c.execute(&format!(
        "DROP TABLE {}.{}",
        engine.quote_ident(schema),
        engine.quote_ident(name)
    ))
    .ok();
}

#[test]
fn postgres_view_pushdown_parity_live() {
    view_pushdown_parity(DbEngine::Postgres, "OCTA_TEST_POSTGRES_URL", "public");
}

#[test]
fn mysql_view_pushdown_parity_live() {
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_MYSQL_URL", DbEngine::MySql) else {
        eprintln!("skipped: OCTA_TEST_MYSQL_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    c.execute("CREATE DATABASE IF NOT EXISTS octa_view_db")
        .expect("create db");
    drop(c);
    view_pushdown_parity(DbEngine::MySql, "OCTA_TEST_MYSQL_URL", "octa_view_db");
    let mut c = connect(&conn, Some(&pass), None).expect("reconnect");
    c.execute("DROP DATABASE octa_view_db").expect("drop db");
}

/// Pivot, Resample and Rolling on the server against the file path's DuckDB
/// SQL over the same rows (see the DuckConn parity tests for the shapes).
fn reshape_pushdown_parity(engine: DbEngine, env_var: &str, schema: &str) {
    use octa::data::pivot::{PivotAgg, pivot_sql};
    use octa::data::timeseries::{
        Interval, ResampleSpec, RollingSpec, TimeAgg, build_resample_sql, build_rolling_sql,
    };
    use octa::db::pushdown as p;
    let _cap = row_cap_lock();
    let Some((conn, secret)) = conn_from_env(env_var, engine) else {
        eprintln!("skipped: {env_var} not set");
        return;
    };
    let mut c = connect(&conn, Some(&secret), None).expect("connect");
    let mut t = DataTable::empty();
    let col = |name: &str, ty: &str| ColumnInfo {
        name: name.into(),
        data_type: ty.into(),
    };
    t.columns = vec![
        col("id", "Int64"),
        col("ts", "Timestamp(Microsecond, None)"),
        col("region", "Utf8"),
        col("kind", "Utf8"),
        col("n", "Int64"),
    ];
    // Distinct times (hour and minute pin i mod 120), so the window order
    // has no ties; "B" beside "b" and "10" beside "9" in kind.
    let kinds = ["a", "b", "B", "10", "9"];
    t.rows = (0..120)
        .map(|i: i64| {
            let day = 1 + (i * 7919 % 360);
            let ts = format!(
                "2024-{:02}-{:02} {:02}:{:02}:00",
                1 + (day - 1) / 30,
                1 + (day - 1) % 28,
                i % 24,
                i % 60
            );
            vec![
                CellValue::Int(i),
                CellValue::DateTime(ts),
                CellValue::String(if i % 3 == 0 { "north" } else { "south" }.into()),
                CellValue::String(kinds[i as usize % kinds.len()].into()),
                if i % 11 == 5 {
                    CellValue::Null
                } else {
                    CellValue::Int((i * 37) % 101 - 50)
                },
            ]
        })
        .collect();
    let name = "octa_reshape_parity";
    c.execute(&format!(
        "DROP TABLE IF EXISTS {}.{}",
        engine.quote_ident(schema),
        engine.quote_ident(name)
    ))
    .ok();
    c.write_table(None, schema, name, DbWriteMode::Create, &t)
        .expect("seed");
    let src = p::ServerSource {
        conn: conn.clone(),
        catalog: None,
        schema: schema.into(),
        table: name.into(),
        filter: None,
        derived: Vec::new(),
    };
    let cols: Vec<String> = t.columns.iter().map(|c| c.name.clone()).collect();
    let stop = std::sync::atomic::AtomicBool::new(false);
    let norm = |t: &DataTable| -> Vec<Vec<String>> {
        let mut rows: Vec<Vec<String>> = t
            .rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|c| match p::cell_f64(c) {
                        Some(x) => format!("{x:.6}"),
                        None => c.to_string().chars().take(19).collect(),
                    })
                    .collect()
            })
            .collect();
        rows.sort();
        rows
    };
    let names = |t: &DataTable| t.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>();

    for agg in [PivotAgg::Count, PivotAgg::Sum, PivotAgg::Max] {
        let want = octa::sql::run_query(&t, &pivot_sql("kind", agg, "n", &["region".into()]))
            .unwrap()
            .table;
        let spec = p::pivot::PivotSpec {
            columns: cols.clone(),
            on: "kind".into(),
            agg,
            value: "n".into(),
            group: vec!["region".into()],
        };
        let (got, _) = p::pivot::run(c.as_mut(), &src, &spec, 2, 1_000, &stop).unwrap();
        assert_eq!(names(&got), names(&want), "pivot {agg:?}");
        assert_eq!(norm(&got), norm(&want), "pivot {agg:?}");
    }
    for &agg in TimeAgg::ALL {
        for interval in [Interval::Week, Interval::Month, Interval::Quarter] {
            let spec = ResampleSpec {
                time_col: "ts".into(),
                value_cols: vec!["n".into()],
                interval,
                agg,
                group_by: vec!["region".into()],
            };
            let want = octa::sql::run_query(&t, &build_resample_sql(&spec, &cols).unwrap())
                .unwrap()
                .table;
            let (got, _) =
                p::timeseries::resample(c.as_mut(), &src, &cols, &spec, 1_000, &stop).unwrap();
            assert_eq!(norm(&got), norm(&want), "resample {agg:?} {interval:?}");
        }
        let spec = RollingSpec {
            order_col: "ts".into(),
            value_col: "n".into(),
            window: 3,
            agg,
            partition_by: vec!["region".into()],
        };
        let want = octa::sql::run_query(&t, &build_rolling_sql(&spec, &cols).unwrap())
            .unwrap()
            .table;
        let (got, _) =
            p::timeseries::rolling(c.as_mut(), &src, &spec, &["id".into()], 1_000, &stop).unwrap();
        assert_eq!(norm(&got), norm(&want), "rolling {agg:?}");
    }

    c.execute(&format!(
        "DROP TABLE {}.{}",
        engine.quote_ident(schema),
        engine.quote_ident(name)
    ))
    .ok();
}

#[test]
fn postgres_reshape_pushdown_parity_live() {
    reshape_pushdown_parity(DbEngine::Postgres, "OCTA_TEST_POSTGRES_URL", "public");
}

#[test]
fn mysql_reshape_pushdown_parity_live() {
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_MYSQL_URL", DbEngine::MySql) else {
        eprintln!("skipped: OCTA_TEST_MYSQL_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    c.execute("CREATE DATABASE IF NOT EXISTS octa_reshape_db")
        .expect("create db");
    drop(c);
    reshape_pushdown_parity(DbEngine::MySql, "OCTA_TEST_MYSQL_URL", "octa_reshape_db");
    let mut c = connect(&conn, Some(&pass), None).expect("reconnect");
    c.execute("DROP DATABASE octa_reshape_db").expect("drop db");
}

/// Charts on the server against the local builder over the same rows:
/// Histogram (number and date), Bar per aggregate, Line, and Box (MySQL:
/// not expressible).
fn chart_pushdown_parity(engine: DbEngine, env_var: &str, schema: &str) {
    use octa::data::chart::{
        Aggregation, ChartConfig, ChartData, ChartKind, ChartLimits, build_chart,
    };
    use octa::db::pushdown as p;
    use octa::db::pushdown::chart::{ChartKey, ChartRequest, ServerChart, column_reads};
    let _cap = row_cap_lock();
    let Some((conn, secret)) = conn_from_env(env_var, engine) else {
        eprintln!("skipped: {env_var} not set");
        return;
    };
    let mut c = connect(&conn, Some(&secret), None).expect("connect");
    let col = |name: &str, ty: &str| ColumnInfo {
        name: name.into(),
        data_type: ty.into(),
    };
    let mut t = DataTable::empty();
    t.columns = vec![
        col("g", "Utf8"),
        col("n", "Int64"),
        col("f", "Float64"),
        col("d", "Date32"),
    ];
    // Sorted by g then f, g lowercase (MySQL's collation cannot reorder it),
    // distinct f, a NULL n now and then.
    let groups = ["alpha", "beta", "gamma", "delta"];
    let mut rows: Vec<Vec<CellValue>> = (0..80i64)
        .map(|i| {
            vec![
                CellValue::String(groups[(i % 4) as usize].into()),
                if i % 9 == 4 {
                    CellValue::Null
                } else {
                    CellValue::Int((i * 37) % 101 - 50)
                },
                CellValue::Float(i as f64 + 0.5),
                CellValue::Date(format!("2024-{:02}-{:02}", 1 + i % 12, 1 + i % 28)),
            ]
        })
        .collect();
    rows.sort_by(|a, b| a[0].to_string().cmp(&b[0].to_string()));
    t.rows = rows;
    let name = "octa_chart_parity";
    c.execute(&format!(
        "DROP TABLE IF EXISTS {}.{}",
        engine.quote_ident(schema),
        engine.quote_ident(name)
    ))
    .ok();
    c.write_table(None, schema, name, DbWriteMode::Create, &t)
        .expect("seed");
    let src = p::ServerSource {
        conn: conn.clone(),
        catalog: None,
        schema: schema.into(),
        table: name.into(),
        filter: None,
        derived: Vec::new(),
    };
    let limits = ChartLimits {
        max_points: 10_000,
        max_categories: 200,
    };
    let all: Vec<usize> = (0..t.row_count()).collect();
    let stop = std::sync::atomic::AtomicBool::new(false);
    let ask = |c: &mut Box<dyn octa::db::DbConnector>, cfg: &ChartConfig| {
        let req = ChartRequest {
            key: ChartKey::of(cfg, limits),
            columns: t.columns.clone(),
            reads: column_reads(&t),
            tie: Vec::new(),
        };
        p::chart::run(c.as_mut(), &src, &req, &stop).unwrap()
    };
    let drawn = |s: ServerChart| match s {
        ServerChart::Drawn { chart, .. } => chart.unwrap(),
        ServerChart::NotExpressible => panic!("expected a chart"),
    };

    for x in [1, 3] {
        let cfg = ChartConfig {
            kind: ChartKind::Histogram,
            x_col: Some(x),
            ..ChartConfig::default()
        };
        let want = build_chart(&t, &all, &cfg, limits).unwrap();
        assert_eq!(drawn(ask(&mut c, &cfg)), want, "histogram x {x}");
    }
    for agg in Aggregation::ALL.iter().copied() {
        let cfg = ChartConfig {
            kind: ChartKind::Bar,
            x_col: Some(0),
            y_cols: vec![1],
            agg,
            ..ChartConfig::default()
        };
        let mut want = build_chart(&t, &all, &cfg, limits).unwrap();
        let mut got = drawn(ask(&mut c, &cfg));
        // Avg / Sum over doubles: compare to six places.
        for prep in [&mut want, &mut got] {
            if let ChartData::Bars { series, .. } = &mut prep.data {
                for s in series {
                    for pt in &mut s.points {
                        pt[1] = (pt[1] * 1e6).round() / 1e6;
                    }
                }
            }
        }
        assert_eq!(got, want, "bar {agg:?}");
    }
    let cfg = ChartConfig {
        kind: ChartKind::Line,
        x_col: Some(2),
        y_cols: vec![1],
        ..ChartConfig::default()
    };
    assert_eq!(
        drawn(ask(&mut c, &cfg)),
        build_chart(&t, &all, &cfg, limits).unwrap(),
        "line"
    );
    let cfg = ChartConfig {
        kind: ChartKind::Box,
        y_cols: vec![1, 2],
        ..ChartConfig::default()
    };
    match (engine, ask(&mut c, &cfg)) {
        (DbEngine::MySql, got) => assert_eq!(got, ServerChart::NotExpressible),
        (_, got) => {
            let (ChartData::Boxes(got), ChartData::Boxes(want)) = (
                drawn(got).data,
                build_chart(&t, &all, &cfg, limits).unwrap().data,
            ) else {
                panic!("boxes");
            };
            assert_eq!(got.len(), want.len(), "boxes");
            for (g, w) in got.iter().zip(&want) {
                for (a, b) in [
                    (g.lower_whisker, w.lower_whisker),
                    (g.q1, w.q1),
                    (g.median, w.median),
                    (g.q3, w.q3),
                    (g.upper_whisker, w.upper_whisker),
                ] {
                    assert!((a - b).abs() < 1e-9, "box {}: {a} vs {b}", g.name);
                }
            }
        }
    }

    c.execute(&format!(
        "DROP TABLE {}.{}",
        engine.quote_ident(schema),
        engine.quote_ident(name)
    ))
    .ok();
}

#[test]
fn postgres_chart_pushdown_parity_live() {
    chart_pushdown_parity(DbEngine::Postgres, "OCTA_TEST_POSTGRES_URL", "public");
}

/// The database's hash of every row (MD5, SHA-256, SHA-512, with trim,
/// upper-case and a NULL text) is the local engine's hash of the same text,
/// and a page of a view with the hash column filters on it.
fn hash_pushdown_parity(engine: DbEngine, env_var: &str, schema: &str) {
    use octa::data::transform::hash_columns::{
        HashColumnsAlgo, HashColumnsSpec, hash_columns_row, row_input,
    };
    use octa::db::pushdown as p;
    use octa::db::pushdown::hash::ServerHash;
    use octa::db::pushdown::view::{ServerView, ViewFilter, page_sql};
    let _cap = row_cap_lock();
    let Some((conn, secret)) = conn_from_env(env_var, engine) else {
        eprintln!("skipped: {env_var} not set");
        return;
    };
    let mut c = connect(&conn, Some(&secret), None).expect("connect");
    let mut t = DataTable::empty();
    t.columns = ["a", "b"]
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).into(),
            data_type: "Utf8".into(),
        })
        .collect();
    let cell = |v: Option<&str>| v.map_or(CellValue::Null, |s| CellValue::String(s.into()));
    t.rows = [
        (Some("x"), Some("y")),
        (Some("  Mixed Case "), None),
        (None, Some("tail ")),
        (Some("o'quote"), Some("caf\u{e9} \u{fc}ber")),
    ]
    .into_iter()
    .map(|(a, b)| vec![cell(a), cell(b)])
    .collect();
    let name = "octa_hash_parity";
    let table = format!(
        "{}.{}",
        engine.quote_ident(schema),
        engine.quote_ident(name)
    );
    c.execute(&format!("DROP TABLE IF EXISTS {table}")).ok();
    c.write_table(None, schema, name, DbWriteMode::Create, &t)
        .expect("seed");
    let src = p::ServerSource {
        conn: conn.clone(),
        catalog: None,
        schema: schema.into(),
        table: name.into(),
        filter: None,
        derived: Vec::new(),
    };
    let names = vec!["a".to_string(), "b".to_string()];
    let stop = std::sync::atomic::AtomicBool::new(false);
    for algo in HashColumnsAlgo::ALL {
        for (trim, upper) in [(false, false), (true, true)] {
            let spec = HashColumnsSpec {
                columns: vec![0, 1],
                algo,
                delimiter: "|".into(),
                null_text: "-".into(),
                trim,
                upper,
            };
            let h = ServerHash::of(&spec, &names, "h");
            let mut got = p::hash::preview(c.as_mut(), &src, &h, 10, &stop).unwrap();
            let mut want: Vec<(String, String)> = (0..t.row_count())
                .map(|r| (row_input(&t, r, &spec), hash_columns_row(&t, r, &spec)))
                .collect();
            got.sort();
            want.sort();
            assert_eq!(got, want, "{algo:?} trim {trim} upper {upper}");
        }
    }
    // A page of a view carrying the hash, filtered on it.
    let spec = HashColumnsSpec {
        columns: vec![0, 1],
        ..HashColumnsSpec::default()
    };
    let digest = hash_columns_row(&t, 3, &spec);
    let view = ServerView {
        filters: vec![ViewFilter::values("h", [digest.clone()])],
        derived: vec![ServerHash::of(&spec, &names, "h")],
        ..Default::default()
    };
    let sql = page_sql(engine, &view, &view.from_item(engine, &table), &[], 10, 0);
    let page = c.query(&sql).unwrap();
    let col = page.columns.iter().position(|c| c.name == "h").expect("h");
    assert_eq!(page.row_count(), 1, "{sql}");
    assert_eq!(page.get(0, col).unwrap().to_string(), digest);
    c.execute(&format!("DROP TABLE {table}")).ok();
}

#[test]
fn postgres_hash_pushdown_parity_live() {
    hash_pushdown_parity(DbEngine::Postgres, "OCTA_TEST_POSTGRES_URL", "public");
}

#[test]
fn mysql_hash_pushdown_parity_live() {
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_MYSQL_URL", DbEngine::MySql) else {
        eprintln!("skipped: OCTA_TEST_MYSQL_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    c.execute("CREATE DATABASE IF NOT EXISTS octa_hash_db")
        .expect("create db");
    drop(c);
    hash_pushdown_parity(DbEngine::MySql, "OCTA_TEST_MYSQL_URL", "octa_hash_db");
    let mut c = connect(&conn, Some(&pass), None).expect("reconnect");
    c.execute("DROP DATABASE octa_hash_db").expect("drop db");
}

/// A TIMESTAMPTZ X under a non-UTC session: the histogram follows the UTC
/// time the connector shows, not the session's zone; a DATE stays at its
/// midnight.
#[test]
fn postgres_chart_timestamptz_follows_the_shown_time_live() {
    use octa::data::chart::{ChartConfig, ChartKind, ChartLimits, build_chart};
    use octa::db::pushdown as p;
    use octa::db::pushdown::chart::{ChartKey, ChartRequest, ServerChart, column_reads};
    let _cap = row_cap_lock();
    let Some((conn, secret)) = conn_from_env("OCTA_TEST_POSTGRES_URL", DbEngine::Postgres) else {
        eprintln!("skipped: OCTA_TEST_POSTGRES_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&secret), None).expect("connect");
    c.execute("DROP TABLE IF EXISTS public.octa_chart_tz").ok();
    c.execute("CREATE TABLE public.octa_chart_tz (ts TIMESTAMPTZ, d DATE)")
        .expect("create");
    c.execute(
        "INSERT INTO public.octa_chart_tz VALUES \
         ('2024-01-07 13:45:12+00', '2024-02-29'), \
         ('2024-07-01 08:00:00+00', '2024-07-01'), \
         ('2024-12-31 23:30:00+00', '2024-12-31')",
    )
    .expect("insert");
    c.execute("SET TIME ZONE 'Europe/Berlin'").expect("zone");
    let t = c
        .query("SELECT ts, d FROM public.octa_chart_tz")
        .expect("read");
    let src = p::ServerSource {
        conn: conn.clone(),
        catalog: None,
        schema: "public".into(),
        table: "octa_chart_tz".into(),
        filter: None,
        derived: Vec::new(),
    };
    let limits = ChartLimits {
        max_points: 1_000,
        max_categories: 200,
    };
    let all: Vec<usize> = (0..t.row_count()).collect();
    let stop = std::sync::atomic::AtomicBool::new(false);
    for x in [0, 1] {
        let cfg = ChartConfig {
            kind: ChartKind::Histogram,
            x_col: Some(x),
            ..ChartConfig::default()
        };
        let req = ChartRequest {
            key: ChartKey::of(&cfg, limits),
            columns: t.columns.clone(),
            reads: column_reads(&t),
            tie: Vec::new(),
        };
        let ServerChart::Drawn { chart, .. } =
            p::chart::run(c.as_mut(), &src, &req, &stop).unwrap()
        else {
            panic!("expected a chart");
        };
        assert_eq!(
            chart.unwrap(),
            build_chart(&t, &all, &cfg, limits).unwrap(),
            "histogram x {x}"
        );
    }
    c.execute("DROP TABLE public.octa_chart_tz").ok();
}

#[test]
fn mysql_chart_pushdown_parity_live() {
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_MYSQL_URL", DbEngine::MySql) else {
        eprintln!("skipped: OCTA_TEST_MYSQL_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass), None).expect("connect");
    c.execute("CREATE DATABASE IF NOT EXISTS octa_chart_db")
        .expect("create db");
    drop(c);
    chart_pushdown_parity(DbEngine::MySql, "OCTA_TEST_MYSQL_URL", "octa_chart_db");
    let mut c = connect(&conn, Some(&pass), None).expect("reconnect");
    c.execute("DROP DATABASE octa_chart_db").expect("drop db");
}
