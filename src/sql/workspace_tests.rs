//! Unit tests for [`workspace`](workspace). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::data::ColumnInfo;

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
        edits: HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
        formulas: std::collections::HashMap::new(),
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
        ssh: None,
        query_timeout_secs: crate::db::DEFAULT_QUERY_TIMEOUT_SECS,
        tunnel_port: None,
    };
    assert_eq!(
        duckdb_attach_sql(&conn, "pw", "prod", true),
        "ATTACH 'host=db.example.com port=5432 dbname=app user=octa password=pw' \
         AS \"prod\" (TYPE postgres, READ_ONLY)"
    );
    conn.engine = crate::db::DbEngine::MySql;
    conn.port = 3306;
    assert_eq!(
        duckdb_attach_sql(&conn, "pw", "prod", true),
        "ATTACH 'host=db.example.com port=3306 database=app user=octa password=pw' \
         AS \"prod\" (TYPE mysql, READ_ONLY)"
    );
    // The writable form (the server-to-server copy's target) drops READ_ONLY.
    assert!(
        duckdb_attach_sql(&conn, "pw", "prod", false).ends_with("(TYPE mysql)"),
        "{}",
        duckdb_attach_sql(&conn, "pw", "prod", false)
    );
}

#[test]
fn duckdb_attach_sql_escapes_awkward_values() {
    let conn = crate::db::DbConnection {
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
        ssh: None,
        query_timeout_secs: crate::db::DEFAULT_QUERY_TIMEOUT_SECS,
        tunnel_port: None,
    };
    let sql = duckdb_attach_sql(&conn, "p'w \\x", "a", true);
    // libpq quoting inside ('' doubled for the outer SQL literal): the space
    // forces quotes, the quote and backslash are backslash-escaped.
    assert_eq!(
        sql,
        "ATTACH 'host=h port=5432 dbname=''d b'' user=u password=''p\\''w \\\\x''' \
         AS \"a\" (TYPE postgres, READ_ONLY)"
    );
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
