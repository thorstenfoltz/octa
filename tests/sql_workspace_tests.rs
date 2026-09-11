//! Integration tests for the multi-table SQL workspace.
//!
//! Covers cross-format JOIN, name de-duplication, ATTACH (DuckDB native),
//! write-back to fresh DuckDB and SQLite files, the cross-schema write
//! path, and the in-place "create new table inside the open file" flow.

use std::collections::HashMap;
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;

use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::sql::{
    AttachKind, QueryKind, SqlWorkspace, TableOrigin, WriteMode, WriteTarget, dedupe_sql_name,
};
use rusqlite::Connection;
use tempfile::TempDir;

fn table_with(columns: &[(&str, &str)], rows: Vec<Vec<CellValue>>) -> DataTable {
    DataTable {
        columns: columns
            .iter()
            .map(|(n, t)| ColumnInfo {
                name: (*n).to_string(),
                data_type: (*t).to_string(),
            })
            .collect(),
        rows,
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

fn sales_table() -> DataTable {
    table_with(
        &[("cid", "Int64"), ("amount", "Float64")],
        vec![
            vec![CellValue::Int(1), CellValue::Float(100.0)],
            vec![CellValue::Int(2), CellValue::Float(200.0)],
            vec![CellValue::Int(3), CellValue::Float(50.0)],
        ],
    )
}

fn customers_table() -> DataTable {
    table_with(
        &[("cid", "Int64"), ("name", "Utf8")],
        vec![
            vec![CellValue::Int(1), CellValue::String("Alice".into())],
            vec![CellValue::Int(2), CellValue::String("Bob".into())],
            vec![CellValue::Int(3), CellValue::String("Carla".into())],
        ],
    )
}

#[test]
fn join_across_active_and_registered_table() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    ws.add_table("customers", &customers_table(), TableOrigin::ActiveTab)
        .unwrap();
    let out = ws
        .execute(
            "SELECT c.name, SUM(d.amount) AS total \
             FROM data d JOIN customers c ON d.cid = c.cid \
             GROUP BY c.name ORDER BY c.name",
        )
        .unwrap();
    assert_eq!(out.kind, QueryKind::Select);
    assert_eq!(out.table.row_count(), 3);
    assert_eq!(out.table.columns.len(), 2);
}

#[test]
fn dedupe_sql_name_resolves_collisions() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.add_table("customers", &customers_table(), TableOrigin::ActiveTab)
        .unwrap();
    let existing: std::collections::HashSet<String> = ws
        .list_tables()
        .iter()
        .map(|t| t.sql_name.clone())
        .collect();
    let unique = dedupe_sql_name("customers", |s| existing.contains(s));
    assert_eq!(unique, "customers_2");
    ws.add_table(&unique, &customers_table(), TableOrigin::ActiveTab)
        .unwrap();
    assert_eq!(ws.list_tables().len(), 2);
}

#[test]
fn add_table_from_csv_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("regions.csv");
    let mut f = fs::File::create(&path).unwrap();
    writeln!(f, "cid,region").unwrap();
    writeln!(f, "1,North").unwrap();
    writeln!(f, "2,South").unwrap();
    writeln!(f, "3,East").unwrap();
    drop(f);

    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    ws.add_table_from_file(&path, None, "regions").unwrap();
    let out = ws
        .execute(
            "SELECT r.region, SUM(d.amount) AS total \
             FROM data d JOIN regions r ON d.cid = r.cid \
             GROUP BY r.region ORDER BY r.region",
        )
        .unwrap();
    assert_eq!(out.table.row_count(), 3);
}

#[test]
fn attach_duckdb_file_and_join_cross_attachment() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("warehouse.duckdb");
    let conn = duckdb::Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE products (cid BIGINT, sku TEXT); \
         INSERT INTO products VALUES (1, 'A'), (2, 'B'), (3, 'C');",
    )
    .unwrap();
    drop(conn);

    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    let attachment = ws.attach(&path, "wh", AttachKind::DuckDb).unwrap();
    assert!(attachment.native);
    let out = ws
        .execute(
            "SELECT p.sku, SUM(d.amount) AS total \
             FROM data d JOIN wh.main.products p ON d.cid = p.cid \
             GROUP BY p.sku ORDER BY p.sku",
        )
        .unwrap();
    assert_eq!(out.table.row_count(), 3);
    let attached = ws.list_attached_tables("wh").unwrap();
    assert!(
        attached
            .iter()
            .any(|t| t.schema == "main" && t.table == "products")
    );

    ws.detach("wh").unwrap();
}

#[test]
fn inspect_registered_workspace_table_returns_columns_count_and_sample() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    let info = ws.inspect_registered_table("data", 5).unwrap();
    assert_eq!(info.qualified_name, "data");
    assert_eq!(info.row_count, Some(3));
    let col_names: Vec<String> = info.columns.iter().map(|c| c.name.clone()).collect();
    assert!(col_names.contains(&"cid".to_string()));
    assert!(col_names.contains(&"amount".to_string()));
    // DESCRIBE reports DuckDB type names, not Arrow names.
    assert!(info.columns.iter().any(|c| c.data_type.contains("BIGINT")));
    assert!(!info.sample_rows.is_empty());
    assert_eq!(info.sample_rows[0].len(), info.columns.len());
}

#[test]
fn collect_autocomplete_identifiers_spans_workspace_and_attachments() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("warehouse.duckdb");
    let conn = duckdb::Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE products (cid BIGINT, sku TEXT); \
         INSERT INTO products VALUES (1, 'A');",
    )
    .unwrap();
    drop(conn);

    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    ws.add_table("customers", &customers_table(), TableOrigin::ActiveTab)
        .unwrap();
    ws.attach(&path, "wh", AttachKind::DuckDb).unwrap();

    let idents = ws.collect_autocomplete_identifiers();
    // Registered tables surface as identifiers.
    assert!(idents.iter().any(|i| i == "data"));
    assert!(idents.iter().any(|i| i == "customers"));
    // Attachment alias surfaces.
    assert!(idents.iter().any(|i| i == "wh"));
    // Attached table name surfaces.
    assert!(idents.iter().any(|i| i == "products"));
    // Columns from every workspace + attached table surface.
    assert!(idents.iter().any(|i| i == "cid"));
    assert!(idents.iter().any(|i| i == "amount"));
    assert!(idents.iter().any(|i| i == "name"));
    assert!(idents.iter().any(|i| i == "sku"));
    // Output is sorted and deduplicated.
    let mut sorted = idents.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(idents, sorted);
    ws.detach("wh").unwrap();
}

#[test]
fn inspect_unknown_registered_table_errors() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    let err = ws.inspect_registered_table("ghost", 5).unwrap_err();
    assert!(err.to_string().contains("ghost"));
}

#[test]
fn inspect_attached_table_returns_columns_count_and_sample() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("warehouse.duckdb");
    let conn = duckdb::Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE products (cid BIGINT, sku TEXT); \
         INSERT INTO products VALUES (1, 'A'), (2, 'B'), (3, 'C');",
    )
    .unwrap();
    drop(conn);

    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    ws.attach(&path, "wh", AttachKind::DuckDb).unwrap();
    let info = ws
        .inspect_attached_table("wh", "main", "products", 5)
        .unwrap();
    assert_eq!(info.qualified_name, "wh.main.products");
    assert_eq!(info.row_count, Some(3));
    let names: Vec<String> = info.columns.iter().map(|c| c.name.clone()).collect();
    assert_eq!(names, vec!["cid", "sku"]);
    assert_eq!(info.sample_rows.len(), 3);
    ws.detach("wh").unwrap();
}

#[test]
fn inspect_attached_table_on_unknown_alias_errors() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    let err = ws
        .inspect_attached_table("ghost", "main", "products", 5)
        .unwrap_err();
    assert!(err.to_string().contains("ghost"));
}

#[test]
fn write_back_create_table_in_new_duckdb() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("out.duckdb");
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    ws.add_table("customers", &customers_table(), TableOrigin::ActiveTab)
        .unwrap();
    let report = ws
        .write_result_to_db(&WriteTarget {
            path: target.clone(),
            kind: AttachKind::DuckDb,
            schema: Some("reports".into()),
            table: "summary".into(),
            mode: WriteMode::Create,
            source_query:
                "SELECT c.name, SUM(d.amount) AS total FROM data d JOIN customers c ON d.cid = c.cid GROUP BY c.name"
                    .into(),
            create_schema_if_missing: true,
        })
        .unwrap();
    assert!(report.created_schema);
    assert_eq!(report.rows_written, 3);

    let verify = duckdb::Connection::open(&target).unwrap();
    let n: i64 = verify
        .query_row("SELECT COUNT(*) FROM reports.summary", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3);
}

#[test]
fn write_back_replace_drops_and_recreates_duckdb_target() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("out.duckdb");
    // Pre-populate the target so Create would fail and Replace must drop.
    {
        let c = duckdb::Connection::open(&target).unwrap();
        c.execute_batch("CREATE SCHEMA reports; CREATE TABLE reports.summary (x INT); INSERT INTO reports.summary VALUES (42);")
            .unwrap();
    }

    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    ws.write_result_to_db(&WriteTarget {
        path: target.clone(),
        kind: AttachKind::DuckDb,
        schema: Some("reports".into()),
        table: "summary".into(),
        mode: WriteMode::Replace,
        source_query: "SELECT cid, amount FROM data".into(),
        create_schema_if_missing: false,
    })
    .unwrap();

    let c = duckdb::Connection::open(&target).unwrap();
    let n: i64 = c
        .query_row("SELECT COUNT(*) FROM reports.summary", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3);
}

#[test]
fn write_back_append_into_existing_duckdb_target() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("out.duckdb");
    {
        let c = duckdb::Connection::open(&target).unwrap();
        c.execute_batch(
            "CREATE TABLE main.tally (cid BIGINT, amount DOUBLE); INSERT INTO main.tally VALUES (99, 999.0);",
        )
        .unwrap();
    }

    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    ws.write_result_to_db(&WriteTarget {
        path: target.clone(),
        kind: AttachKind::DuckDb,
        schema: Some("main".into()),
        table: "tally".into(),
        mode: WriteMode::Append,
        source_query: "SELECT cid, amount FROM data".into(),
        create_schema_if_missing: false,
    })
    .unwrap();

    let c = duckdb::Connection::open(&target).unwrap();
    let n: i64 = c
        .query_row("SELECT COUNT(*) FROM main.tally", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 4);
}

#[test]
fn write_back_create_sqlite_target_via_rusqlite() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("out.sqlite");
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    let report = ws
        .write_result_to_db(&WriteTarget {
            path: target.clone(),
            kind: AttachKind::Sqlite,
            schema: None,
            table: "summary".into(),
            mode: WriteMode::Create,
            source_query: "SELECT cid, amount FROM data".into(),
            create_schema_if_missing: false,
        })
        .unwrap();
    assert_eq!(report.rows_written, 3);

    let c = Connection::open(&target).unwrap();
    let n: i64 = c
        .query_row("SELECT COUNT(*) FROM summary", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3);
}

#[test]
fn write_back_append_into_existing_sqlite_target() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("out.sqlite");
    {
        let c = Connection::open(&target).unwrap();
        c.execute_batch(
            "CREATE TABLE tally (cid INTEGER, amount REAL); \
             INSERT INTO tally VALUES (99, 999.0);",
        )
        .unwrap();
    }

    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    ws.write_result_to_db(&WriteTarget {
        path: target.clone(),
        kind: AttachKind::Sqlite,
        schema: None,
        table: "tally".into(),
        mode: WriteMode::Append,
        source_query: "SELECT cid, amount FROM data".into(),
        create_schema_if_missing: false,
    })
    .unwrap();

    let c = Connection::open(&target).unwrap();
    let n: i64 = c
        .query_row("SELECT COUNT(*) FROM tally", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 4);
}

#[test]
fn write_back_sqlite_rejects_non_main_schema() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("out.sqlite");
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    let err = ws
        .write_result_to_db(&WriteTarget {
            path: target,
            kind: AttachKind::Sqlite,
            schema: Some("reports".into()),
            table: "summary".into(),
            mode: WriteMode::Create,
            source_query: "SELECT 1".into(),
            create_schema_if_missing: false,
        })
        .unwrap_err();
    assert!(err.to_string().contains("SQLite has no schemas"));
}

#[test]
fn write_back_create_into_open_duckdb_file_then_picker_sees_it() {
    use octa::formats::FormatReader;
    use octa::formats::duckdb_reader::DuckDbReader;

    // Create a DuckDB file with one table in main, then write back a new
    // table into a fresh schema and confirm `list_tables` returns both
    // (and the new entry carries `schema: Some("reports")`).
    let dir = TempDir::new().unwrap();
    let target: PathBuf = dir.path().join("open.duckdb");
    {
        let c = duckdb::Connection::open(&target).unwrap();
        c.execute_batch("CREATE TABLE main.base (x INT); INSERT INTO main.base VALUES (1);")
            .unwrap();
    }

    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&sales_table()).unwrap();
    ws.write_result_to_db(&WriteTarget {
        path: target.clone(),
        kind: AttachKind::DuckDb,
        schema: Some("reports".into()),
        table: "q4".into(),
        mode: WriteMode::Create,
        source_query: "SELECT cid, amount FROM data".into(),
        create_schema_if_missing: true,
    })
    .unwrap();

    let listing = DuckDbReader.list_tables(&target).unwrap().unwrap();
    let schemas: Vec<Option<String>> = listing.iter().map(|t| t.schema.clone()).collect();
    assert!(schemas.contains(&Some("main".into())));
    assert!(schemas.contains(&Some("reports".into())));
    assert!(
        listing
            .iter()
            .any(|t| t.schema.as_deref() == Some("reports") && t.name == "q4")
    );
}

/// A three-level engine cannot be enumerated without naming a catalog.
///
/// The regression: `attach_import` asked for `list_schemas(None)`, which on
/// Databricks means "the schemas of whatever catalog this session happens to
/// sit in". The attach reported success as a fallback and imported nothing,
/// while the sidebar - which walks catalog -> schema -> table - listed every
/// table. The guard fires before any connection is opened, so this needs no
/// server.
#[test]
fn attaching_a_catalog_engine_without_a_catalog_is_refused() {
    use octa::db::{DbAuth, DbConnection, DbEngine};

    let conn = DbConnection {
        id: "t".into(),
        name: "warehouse".into(),
        engine: DbEngine::Databricks,
        host: "example.cloud.databricks.com".into(),
        port: 443,
        database: "abc123".into(),
        username: "token".into(),
        auth: DbAuth::Password,
        allow_writes: false,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        query_timeout_secs: octa::db::DEFAULT_QUERY_TIMEOUT_SECS,
        ssh: None,
        tunnel_port: None,
    };

    let mut ws = SqlWorkspace::new().unwrap();
    let err = ws
        .attach_db(
            &conn,
            Some("secret"),
            None,
            "wh",
            &octa::sql::AttachScope::default(),
        )
        .expect_err("a catalog engine with no catalog must not attach");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("catalog"),
        "the refusal has to say a catalog is needed, got: {msg}"
    );
    assert!(
        ws.list_attached().is_empty(),
        "nothing may be left attached after the refusal"
    );

    // A table without its schema addresses nothing, so it is refused before
    // a connection is opened too (this test has no server to open one on).
    let err = ws
        .attach_db(
            &conn,
            Some("secret"),
            None,
            "wh",
            &octa::sql::AttachScope {
                catalog: Some("fab".into()),
                schema: None,
                table: Some("orders".into()),
            },
        )
        .expect_err("a table with no schema must not attach");
    assert!(
        format!("{err:#}").contains("schema"),
        "the refusal has to say the schema is missing, got: {err:#}"
    );
}

/// An imported table is named after itself so a `FROM` clause is short, and
/// the row can be renamed to whatever the user wants to type. The rename has
/// to move the DuckDB table too, or the registry would point at nothing.
#[test]
fn a_workspace_table_can_be_renamed_and_keeps_its_rows() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.add_table(
        "orders",
        &sales_table(),
        TableOrigin::Db("wh main.sales.orders".into()),
    )
    .unwrap();
    ws.add_table(
        "customers",
        &customers_table(),
        TableOrigin::Db("wh main.sales.c".into()),
    )
    .unwrap();

    ws.rename_table("orders", "o").unwrap();
    let out = ws.execute("SELECT * FROM o").unwrap();
    assert_eq!(out.table.row_count(), 3, "the rows travel with the name");
    assert!(
        ws.execute("SELECT * FROM orders").is_err(),
        "the old name must be gone"
    );
    assert!(ws.list_tables().iter().any(|t| t.sql_name == "o"));

    // A name someone else holds is refused, and the refusal changes nothing.
    let err = ws
        .rename_table("customers", "o")
        .expect_err("name is taken");
    assert!(format!("{err:#}").contains('o'), "{err:#}");
    assert_eq!(
        ws.execute("SELECT * FROM customers")
            .unwrap()
            .table
            .row_count(),
        3
    );

    // Typed names are sanitised, so a FROM clause never needs quoting.
    ws.rename_table("customers", "My Customers").unwrap();
    assert_eq!(
        ws.execute("SELECT * FROM my_customers")
            .unwrap()
            .table
            .row_count(),
        3
    );

    // An empty name is refused rather than sanitised into "table".
    assert!(ws.rename_table("my_customers", "   ").is_err());
    assert!(
        ws.list_tables()
            .iter()
            .any(|t| t.sql_name == "my_customers")
    );

    // `data` keeps its name: refresh, the edit path and the mutation flow all
    // address the active tab's table by it. Refused in the workspace itself,
    // not only by hiding the affordance in the panel.
    ws.set_active_table(&sales_table()).unwrap();
    let err = ws
        .rename_table("data", "rows")
        .expect_err("the active tab's table cannot be renamed");
    assert!(format!("{err:#}").contains("data"), "{err:#}");
    assert_eq!(
        ws.execute("SELECT * FROM data").unwrap().table.row_count(),
        3
    );
}
