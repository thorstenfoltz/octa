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

    // PK discovery via the shared information_schema query.
    let pk = c
        .query(&octa::db::primary_key_sql(
            engine,
            None,
            schema,
            &table_name,
        ))
        .expect("pk query");
    let pk_cols: Vec<String> = pk
        .rows
        .iter()
        .filter_map(|r| r.first().map(|v| v.to_string()))
        .collect();
    assert_eq!(pk_cols, vec!["id".to_string()]);

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

    let plan = build_write_back_plan(&t, &pk_cols).expect("plan");
    assert_eq!(plan.change_count(), 3);
    let report = apply_write_back(
        c.as_mut(),
        engine,
        schema,
        &table_name,
        &t.columns,
        &pk_cols,
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
        &pk_cols,
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
