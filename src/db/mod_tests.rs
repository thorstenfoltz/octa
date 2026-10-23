use super::*;
use crate::data::CellValue;

fn conn(allow_writes: bool) -> DbConnection {
    DbConnection {
        id: "db-1".into(),
        name: "prod".into(),
        engine: DbEngine::Postgres,
        host: "localhost".into(),
        port: 5432,
        database: "app".into(),
        username: "octa".into(),
        auth: DbAuth::Password,
        allow_writes,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: super::DEFAULT_QUERY_TIMEOUT_SECS,
        tls: Default::default(),
        tunnel_port: None,
    }
}

#[test]
fn cancel_flag_is_shared_across_clones() {
    let flag = CancelFlag::new();
    let clone = flag.clone();
    assert!(!flag.is_cancelled());
    clone.cancel();
    assert!(flag.is_cancelled(), "a clone must share the same flag");
    flag.reset();
    assert!(!clone.is_cancelled());
}

#[test]
fn dbauth_serde_roundtrip() {
    let a = DbAuth::KeyPairJwt {
        private_key_path: "/k.pem".into(),
    };
    let s = serde_json::to_string(&a).unwrap();
    assert_eq!(serde_json::from_str::<DbAuth>(&s).unwrap(), a);
}

#[test]
fn connection_roundtrips_through_serde() {
    for auth in [
        DbAuth::AwsIam {
            region: Some("eu-central-1".into()),
            sso_start_url: None,
            sso_region: None,
            sso_account_id: None,
            sso_role: None,
        },
        DbAuth::AwsIam {
            region: None,
            sso_start_url: Some("https://acme.awsapps.com/start".into()),
            sso_region: Some("eu-central-1".into()),
            sso_account_id: Some("123456789012".into()),
            sso_role: Some("DBReader".into()),
        },
        DbAuth::AzureAd,
        DbAuth::GcpIam,
    ] {
        let mut c = conn(true);
        c.auth = auth;
        let toml = toml::to_string(&c).unwrap();
        let back: DbConnection = toml::from_str(&toml).unwrap();
        assert_eq!(c, back);
    }
}

#[test]
fn old_connection_without_new_fields_still_loads() {
    // A settings.toml written before `auth`/`allow_writes` existed.
    let raw = r#"
id = "db-2"
name = "legacy"
engine = "MySql"
host = "h"
port = 3306
database = "d"
username = "u"
"#;
    let c: DbConnection = toml::from_str(raw).unwrap();
    assert_eq!(c.auth, DbAuth::Password);
    assert!(!c.allow_writes);
}

#[test]
fn a_refusal_says_octa_refused_it_and_quotes_the_word_it_read() {
    // A typo used to read like any other error, with nothing saying that
    // Octa, not the database, had stopped it, nor which word it had read.
    let pg = conn(false);
    let err = ensure_write_allowed(&pg, Some("selct * from t"))
        .unwrap_err()
        .to_string();
    assert!(
        err.starts_with("Octa did not send this to 'prod', because the connection is read-only."),
        "{err}"
    );
    assert!(err.contains("starts with selct,"), "{err}");
    // Every kind of refusal is plain sentences.
    let mut my = conn(false);
    my.engine = DbEngine::MySql;
    let mut ms = conn(false);
    ms.engine = DbEngine::Mssql;
    for (c, q) in [
        (&pg, "selct 1"),
        (&pg, "DELETE FROM t"),
        (&my, "SELECT 1; DROP TABLE t"),
        (&ms, "sp_who"),
        (&ms, "SELECT 1 COMMIT"),
    ] {
        let err = ensure_write_allowed(c, Some(q)).unwrap_err().to_string();
        assert!(err.starts_with("Octa did not send this"), "{q}: {err}");
        assert!(!err.contains(';'), "{q}: {err}");
    }
    let err = ensure_write_allowed(&pg, None).unwrap_err().to_string();
    assert!(err.starts_with("Octa did not send this"), "{err}");
}

#[test]
fn write_gate_refuses_mutations_on_readonly() {
    let c = conn(false);
    let err = ensure_write_allowed(&c, Some("UPDATE t SET x = 1"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Allow writes"), "{err}");
    assert!(ensure_write_allowed(&c, Some("SELECT 1")).is_ok());
    assert!(ensure_write_allowed(&c, None).is_err());
}

#[test]
fn a_second_statement_is_a_write_on_mysql_and_bigquery() {
    for engine in [DbEngine::MySql, DbEngine::BigQuery] {
        let ro = DbConnection {
            engine,
            ..conn(false)
        };
        for q in [
            "SELECT 1; DELETE FROM t",
            "SELECT 1 --1; DROP TABLE t",
            "SELECT * FROM t # it's\n;\nDELETE FROM t",
            "SELECT 'a;b' FROM t",
        ] {
            let err = ensure_write_allowed(&ro, Some(q)).unwrap_err().to_string();
            assert!(err.contains("semicolon"), "{engine:?} {q}: {err}");
        }
        assert!(ensure_write_allowed(&ro, Some("SELECT 1;\n")).is_ok());
        let rw = DbConnection {
            engine,
            ..conn(true)
        };
        assert!(ensure_write_allowed(&rw, Some("SELECT 1; DELETE FROM t")).is_ok());
    }
    // PostgreSQL prepares first, which refuses a second statement by itself.
    assert!(!may_write(DbEngine::Postgres, "SELECT 1; SELECT 2"));
    assert!(ensure_write_allowed(&conn(false), Some("SELECT 1; SELECT 2")).is_ok());
}

#[test]
fn a_read_only_connection_runs_only_statements_that_start_with_a_read() {
    let ro = |engine| DbConnection {
        engine,
        ..conn(false)
    };
    let pg = ro(DbEngine::Postgres);
    for q in [
        "SELECT 1",
        "  -- a note\nSELECT 1",
        "/* note */ (SELECT 1) UNION (SELECT 2)",
        "WITH x AS (SELECT 1) SELECT * FROM x",
        "SHOW search_path",
        "EXPLAIN ANALYZE SELECT 1",
        "",
    ] {
        assert!(ensure_write_allowed(&pg, Some(q)).is_ok(), "{q}");
    }
    for q in [
        "CALL refresh_all()",
        "DO $$ BEGIN DELETE FROM t; END $$",
        "GRANT SELECT ON t TO bob",
        "VACUUM t",
        "-- a note\nDELETE FROM t",
        "/* note */DELETE/**/FROM t",
        // PostgreSQL nests comments, so this is a DELETE there.
        "/* /* */ SELECT 1 */ DELETE FROM t",
    ] {
        let err = ensure_write_allowed(&pg, Some(q)).unwrap_err().to_string();
        assert!(err.contains("Allow writes"), "{q}: {err}");
    }
    // Only an engine that holds the session read-only itself may run EXPLAIN
    // ANALYZE, which runs what it explains.
    assert!(may_write(DbEngine::Trino, "EXPLAIN ANALYZE SELECT 1"));
    assert!(!may_write(DbEngine::Trino, "EXPLAIN SELECT 1"));
    assert!(may_write(DbEngine::Redshift, "SELECT * INTO t2 FROM t"));
    assert!(may_write(DbEngine::Redshift, "SELECT * INTO #tmp FROM t"));
    assert!(!may_write(
        DbEngine::Redshift,
        "SELECT * FROM t WHERE note = 'moved into archive' -- into staging later"
    ));
    assert!(!may_write(DbEngine::Redshift, "SELECT \"into\" FROM t"));
    // A backslash or a dollar sign leaves the quotes open to two readings.
    assert!(may_write(DbEngine::Redshift, "SELECT 'a\\' INTO t2 FROM t"));
    assert!(may_write(DbEngine::Redshift, "SELECT $$ x $$ INTO t2"));
    assert!(may_write(
        DbEngine::Databricks,
        "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x"
    ));
    assert!(!may_write(
        DbEngine::Databricks,
        "WITH h AS (SELECT * FROM hist WHERE operation = 'MERGE') SELECT * FROM h"
    ));
    assert!(!may_write(
        DbEngine::Snowflake,
        "WITH x AS (SELECT 1) SELECT * FROM x"
    ));
}

#[test]
fn a_read_only_sql_server_batch_cannot_end_its_transaction() {
    let ms = DbConnection {
        engine: DbEngine::Mssql,
        ..conn(false)
    };
    // The rolled-back transaction undoes these, so they may run.
    for q in [
        "SELECT 1 DELETE FROM t",
        "DECLARE @n int = 1 SELECT @n",
        ";WITH x AS (SELECT 1 AS a) SELECT * FROM x",
        "SELECT * FROM sys.dm_exec_sessions",
    ] {
        assert!(ensure_write_allowed(&ms, Some(q)).is_ok(), "{q}");
    }
    // The words in text, a comment or a bracketed name do not run.
    for q in [
        "SELECT 'commit'",
        "SELECT * FROM job_log WHERE step = 'EXECUTE' -- rollback if needed",
        "SELECT [commit], [exec] FROM t /* exec /* nested */ commit */",
        "SELECT @exec = 1",
    ] {
        assert!(ensure_write_allowed(&ms, Some(q)).is_ok(), "{q}");
    }
    for q in [
        "SELECT 1 COMMIT DELETE FROM t",
        "SELECT 1 ROLLBACK DELETE FROM t",
        "SELECT 1 EXEC('COM' + 'MIT')",
        "SELECT 'a' /* note */ COMMIT",
        // An unclosed quote has no single reading: every word counts.
        "SELECT 'commit",
        "sp_who",
        "EXEC sp_help 't'",
    ] {
        let err = ensure_write_allowed(&ms, Some(q)).unwrap_err().to_string();
        assert!(err.contains("Allow writes"), "{q}: {err}");
    }
}

#[test]
fn write_gate_open_when_allowed() {
    let c = conn(true);
    assert!(ensure_write_allowed(&c, Some("DROP TABLE t")).is_ok());
    assert!(ensure_write_allowed(&c, None).is_ok());
}

#[test]
fn fresh_ids_are_distinct() {
    assert_ne!(DbConnection::fresh_id(), DbConnection::fresh_id());
}

#[test]
fn timestamps_keep_their_fraction_without_trailing_zeros() {
    let at = |s: &str| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f").unwrap();
    assert_eq!(
        datetime_text(&at("2024-01-02 03:04:05")),
        "2024-01-02 03:04:05"
    );
    assert_eq!(
        datetime_text(&at("2024-01-02 03:04:05.5")),
        "2024-01-02 03:04:05.5"
    );
    assert_eq!(
        datetime_text(&at("2024-01-02 03:04:05.789012")),
        "2024-01-02 03:04:05.789012"
    );
    assert_eq!(
        datetime_text(&at("2024-01-02 03:04:05.1234567")),
        "2024-01-02 03:04:05.1234567"
    );
}

/// A server whose catalog lists `columns` (schema, table, column), or refuses
/// to be read when `None`; records every statement it is sent.
struct CatalogConn {
    columns: Option<Vec<(&'static str, &'static str, &'static str)>>,
    sent: Vec<String>,
}
impl DbConnector for CatalogConn {
    fn engine(&self) -> DbEngine {
        DbEngine::Oracle
    }
    fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn query(&mut self, _: &str) -> anyhow::Result<DataTable> {
        let Some(columns) = &self.columns else {
            anyhow::bail!("ORA-01031: insufficient privileges");
        };
        let mut t = DataTable::empty();
        t.columns = ["TABLE_SCHEMA", "TABLE_NAME", "COLUMN_NAME"]
            .map(|n| crate::data::ColumnInfo {
                name: n.into(),
                data_type: "Utf8".into(),
            })
            .to_vec();
        t.rows = columns
            .iter()
            .map(|(s, tb, c)| {
                [s, tb, c]
                    .map(|v| CellValue::String(v.to_string()))
                    .to_vec()
            })
            .collect();
        Ok(t)
    }
    fn execute(&mut self, sql: &str) -> anyhow::Result<u64> {
        self.sent.push(sql.to_string());
        Ok(1)
    }
    fn write_table(
        &mut self,
        _: Option<&str>,
        _: &str,
        _: &str,
        _: DbWriteMode,
        _: &DataTable,
    ) -> anyhow::Result<DbWriteReport> {
        unimplemented!()
    }
}

fn id_note_extra() -> DataTable {
    let mut data = DataTable::empty();
    data.columns = ["id", "note", "extra"]
        .map(|n| crate::data::ColumnInfo {
            name: n.into(),
            data_type: "Utf8".into(),
        })
        .to_vec();
    data.rows = vec![vec![CellValue::Int(1), CellValue::Null, CellValue::Null]];
    data
}

fn insert(c: &CatalogConn) -> &str {
    c.sent
        .iter()
        .find(|s| s.starts_with("INSERT"))
        .expect("an INSERT")
}

/// Every identifier is quoted, so an append has to name the table and its
/// columns the way the server stores them: Oracle folds an unquoted `orders`
/// to `ORDERS`, and the `"orders"` / `"id"` the INSERT used to send are other
/// names there (ORA-00942 / ORA-00904). A column the table lacks keeps its
/// own name, so the server's error names it.
#[test]
fn append_uses_the_servers_spelling_of_table_and_columns() {
    let mut c = CatalogConn {
        columns: Some(vec![
            ("APP", "ORDERS", "ID"),
            ("APP", "ORDERS", "NOTE"),
            ("APP", "OTHER", "EXTRA"),
        ]),
        sent: vec![],
    };
    let data = id_note_extra();
    write_table_generic(
        &mut c,
        DbEngine::Oracle,
        None,
        "APP",
        "orders",
        DbWriteMode::Append,
        &data,
    )
    .expect("write");
    let sql = insert(&c);
    assert!(
        sql.contains(r#"INTO "APP"."ORDERS" ("ID", "NOTE", "extra")"#),
        "{sql}"
    );
}

/// The lookup is a convenience: a catalog the user may not read must not
/// block a write that would have worked with the data's own names.
#[test]
fn append_keeps_the_datas_names_when_the_catalog_cannot_be_read() {
    let mut c = CatalogConn {
        columns: None,
        sent: vec![],
    };
    let data = id_note_extra();
    write_table_generic(
        &mut c,
        DbEngine::Oracle,
        None,
        "APP",
        "orders",
        DbWriteMode::Append,
        &data,
    )
    .expect("write");
    let sql = insert(&c);
    assert!(
        sql.contains(r#"INTO "APP"."orders" ("id", "note", "extra")"#),
        "{sql}"
    );
}
