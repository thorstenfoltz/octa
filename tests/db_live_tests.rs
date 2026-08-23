//! Live-server integration tests for the DB connectors. Each engine's tests
//! run only when its env var is set (see the plan's docker rig):
//!
//! ```bash
//! export OCTA_TEST_POSTGRES_URL='host=127.0.0.1;port=5432;db=postgres;user=postgres;pass=pw'
//! export OCTA_TEST_MYSQL_URL='host=127.0.0.1;port=3306;db=mysql;user=root;pass=pw'
//! export OCTA_TEST_MSSQL_URL='host=127.0.0.1;port=1433;db=master;user=sa;pass=Str0ng!Pw'
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
    let mut c = connect(&conn, Some(&pass)).expect("connect");

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
    let mut c = connect(&conn, Some(&pass)).expect("connect");
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

/// A huge SELECT must stop collecting at the initial-load row cap instead
/// of materialising every row (used to OOM-crash the app), and the
/// connection must stay usable afterwards (MySQL/MSSQL drain the remaining
/// wire packets after the early stop). One test fn for all three engines so
/// the process-wide guard is held once; cap 5 stays above every row count
/// the other live tests read in parallel.
#[test]
fn query_row_cap_live() {
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
        let mut c = connect(&conn, Some(&pass)).expect("connect");
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
        let mut c = connect(&conn, Some(&pass)).expect("connect");
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
    let mut c = connect(&conn, Some(&pass)).expect("connect");
    c.execute("CREATE DATABASE IF NOT EXISTS octa_wb_db")
        .expect("create db");
    drop(c);
    exercise_write_back(DbEngine::MySql, "OCTA_TEST_MYSQL_URL", "octa_wb_db");
    let mut c = connect(&conn, Some(&pass)).expect("reconnect");
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
    let mut c = connect(&conn, Some(&pass)).expect("connect");
    c.execute("CREATE DATABASE IF NOT EXISTS octa_test_db")
        .expect("create db");
    drop(c);
    exercise(
        DbEngine::MySql,
        "OCTA_TEST_MYSQL_URL",
        "octa_test_db",
        "octa_test_db",
    );
    let mut c = connect(&conn, Some(&pass)).expect("reconnect");
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
    let mut c = connect(&conn, Some(&pass)).expect("connect");

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
    let mut c = connect(&conn, Some(&pass)).expect("connect");
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
    let mut c = connect(&conn, Some(&pass)).expect("connect");
    let one = c.query("SELECT 1 AS ONE").expect("select 1");
    assert_eq!(one.row_count(), 1);
    assert_eq!(one.rows[0][0], CellValue::Int(1));
}

#[test]
fn snowflake_read_roundtrip() {
    // Read-only smoke test over the SQL API v2 (auth via the connection's
    // configured mode). Env-gated on a real Snowflake account URL.
    let Some((conn, pass)) = conn_from_env("OCTA_TEST_SNOWFLAKE_URL", DbEngine::Snowflake) else {
        eprintln!("skipped: OCTA_TEST_SNOWFLAKE_URL not set");
        return;
    };
    let mut c = connect(&conn, Some(&pass)).expect("connect");
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
    let mut c = connect(&conn, Some(&pass)).expect("connect");
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
    let mut c = connect(&conn, Some(&pass)).expect("connect");
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
    let mut my = connect(&my_conn, Some(&my_pass)).expect("connect mysql");
    my.execute("CREATE DATABASE IF NOT EXISTS octa_copy_db")
        .expect("create db");
    my.execute("DROP TABLE IF EXISTS octa_copy_db.people")
        .expect("pre-clean");
    my.execute("CREATE TABLE octa_copy_db.people (id BIGINT PRIMARY KEY, name VARCHAR(50))")
        .expect("create source");
    my.execute("INSERT INTO octa_copy_db.people VALUES (1, 'ada'), (2, 'o''hara'), (3, 'zoe')")
        .expect("seed");

    let source = DbCopyEnd {
        conn: my_conn.clone(),
        catalog: None,
        schema: "octa_copy_db".into(),
        table: "people".into(),
    };
    let target = DbCopyEnd {
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
    )
    .expect("create copy");
    assert_eq!(report.rows_copied, 3);
    assert!(report.created);

    let mut pg = connect(&pg_conn, Some(&pg_pass)).expect("connect pg");
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
    )
    .expect("replace copy");
    let n = pg
        .query("SELECT COUNT(*) FROM public.octa_copied_people")
        .expect("recount");
    assert_eq!(n.rows[0][0], CellValue::Int(3));

    // The target's write gate is enforced.
    pg_conn.allow_writes = false;
    let gated = DbCopyEnd {
        conn: pg_conn,
        ..target.clone()
    };
    let err = copy_table(
        &source,
        Some(&my_pass),
        &gated,
        Some(&pg_pass),
        DbWriteMode::Replace,
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
    let mut c = connect(&conn, Some(&secret)).expect("connect");
    c.execute("DROP TABLE IF EXISTS octa_diff_live").unwrap();
    c.execute("CREATE TABLE octa_diff_live (id INT PRIMARY KEY, name TEXT)")
        .unwrap();
    c.execute("INSERT INTO octa_diff_live VALUES (1,'a'),(2,'b'),(3,'c')")
        .unwrap();

    let db_side =
        octa::db::fetch_table::fetch_table(&conn, Some(&secret), None, "public", "octa_diff_live")
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
    let mut c = connect(&conn, Some(&secret)).expect("connect");
    c.execute("DROP TABLE IF EXISTS octa_fetch_live").unwrap();
    c.execute("CREATE TABLE octa_fetch_live (id INT PRIMARY KEY)")
        .unwrap();
    c.execute("INSERT INTO octa_fetch_live VALUES (1),(2)")
        .unwrap();

    let t = octa::db::fetch_table::fetch_table(&conn, Some(&secret), None, "", "octa_fetch_live")
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
    let mut c = connect(&conn, Some(&secret)).expect("connect");

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
