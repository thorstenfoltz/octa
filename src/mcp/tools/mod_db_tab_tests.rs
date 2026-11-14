//! A database tab that holds part of its table: nothing is downloaded. The tools
//! that can run as SQL on the server do, and the rest say they saw the loaded rows.
//! The server is a DuckDB file, so nothing here needs a network.

use super::*;
use crate::app::chat::tools::dispatch;
use octa::data::ColumnInfo;
use octa::db::pushdown::ServerSource;
use octa::formats::ThreadRowCap;
use serde_json::json;

const ROWS: i64 = 500;
const LOADED: i64 = 100;
/// Fewer rows than the table has. With this in force a tool that fetched the table
/// to work on it gets the wrong answer, so a right answer proves the server did it.
const FETCH_LIMIT: usize = 150;

fn duckdb_connection(path: &std::path::Path) -> octa::db::DbConnection {
    octa::db::DbConnection::file(
        "octa-test-db-tab".into(),
        "Warehouse",
        octa::db::DbEngine::DuckDb,
        path.to_str().unwrap(),
        false,
    )
}

fn source(conn: &octa::db::DbConnection, table: &str) -> ServerSource {
    ServerSource {
        conn: conn.clone(),
        catalog: None,
        schema: "main".into(),
        table: table.into(),
        filter: None,
        derived: Vec::new(),
    }
}

fn tab(handle: &str, table: &str, db: SnapshotDb) -> TableSnapshot {
    let mut page = DataTable::empty();
    page.columns = [
        ("id", "Int64"),
        ("amount", "Int64"),
        ("grp", "Utf8"),
        ("day", "Date32"),
    ]
    .map(|(name, data_type)| ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
    })
    .to_vec();
    page.rows = (1..=LOADED)
        .map(|i| {
            let grp = ["a", "b", "c"][(i % 3) as usize];
            vec![
                CellValue::Int(i),
                CellValue::Int(i * 2),
                CellValue::String(grp.into()),
                CellValue::Null,
            ]
        })
        .collect();
    TableSnapshot {
        handle: handle.into(),
        display_name: table.into(),
        source_path: None,
        table: page,
        window: None,
        read_only: false,
        db: Some(db),
    }
}

/// A 500-row table behind a saved connection, and a tab holding its first 100.
fn fixture() -> (tempfile::TempDir, ToolContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.duckdb");
    duckdb::Connection::open(&path)
        .unwrap()
        .execute_batch(&format!(
            "CREATE TABLE sales AS SELECT i AS id, i * 2 AS amount,
                 ['a', 'b', 'c'][i % 3 + 1] AS grp,
                 DATE '2026-01-01' + CAST(i % 10 AS INTEGER) AS day
             FROM range(1, {ROWS} + 1) t(i);
             CREATE TABLE regions AS SELECT i AS id, 'r' || i AS region FROM range(1, 4) t(i);"
        ))
        .unwrap();
    let conn = duckdb_connection(&path);
    let db = SnapshotDb {
        table: "main.sales".into(),
        connection: "Warehouse".into(),
        source: Some(source(&conn, "sales")),
        edited: false,
    };
    let mut ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, vec![conn], true, 0);
    // Keeps `db_secret` from reading the real settings file.
    ctx.cloud_settings = Some(octa::ui::settings::AppSettings::default());
    ctx.open_tabs = vec![tab("#1", "sales", db)];
    ctx.active_tab = Some(0);
    (dir, ctx)
}

fn params<P: serde::de::DeserializeOwned>(value: Value) -> P {
    serde_json::from_value(value).unwrap()
}

fn sql(ctx: &ToolContext, query: &str) -> anyhow::Result<Value> {
    run_sql::run(ctx, &params(json!({"open_tab": "@active", "query": query})))
}

fn stays_on_loaded_rows(ctx: &mut ToolContext, edited: bool) {
    let db = ctx.open_tabs[0].db.as_mut().unwrap();
    db.source = None;
    db.edited = edited;
}

fn point_at_nothing(ctx: &mut ToolContext) {
    let db = ctx.open_tabs[0].db.as_mut().unwrap();
    db.source.as_mut().unwrap().conn.database = "/nowhere/at/all.duckdb".into();
}

/// The control for every test using [`FETCH_LIMIT`]: reading the table does stop
/// there, so those tests' right answers cannot have come from reading it.
#[test]
fn the_fetch_limit_bites_on_this_server() {
    let (_dir, ctx) = fixture();
    let _limit = ThreadRowCap::new(FETCH_LIMIT);
    let (_, src) = ctx.db_tab(&Source::ActiveTab).unwrap();
    let mut c = ctx.db_tab_connect(src).unwrap();
    let all = c.query("SELECT * FROM main.sales").unwrap();
    assert_eq!(all.row_count(), FETCH_LIMIT);
}

#[test]
fn run_sql_runs_on_the_server_and_fetches_only_the_answer() {
    let (_dir, ctx) = fixture();
    let _limit = ThreadRowCap::new(FETCH_LIMIT);
    let out = sql(&ctx, "SELECT CAST(SUM(amount) AS BIGINT) FROM data").unwrap();
    assert_eq!(out["result"]["rows"][0][0], json!(ROWS * (ROWS + 1)));
    assert_eq!(out["on_server"], json!(true));
}

#[test]
fn a_query_of_its_own_with_or_a_trailing_semicolon_still_runs() {
    let (_dir, ctx) = fixture();
    for query in [
        "WITH big AS (SELECT * FROM data WHERE id > 490) SELECT COUNT(*) FROM big;",
        "with big as (select * from data where id > 490) select count(*) from big",
        "SELECT COUNT(*) FROM data WHERE id > 490 ;\n",
    ] {
        let out = sql(&ctx, query).unwrap();
        assert_eq!(out["result"]["rows"][0][0], json!(10), "{query}");
    }
}

#[test]
fn select_star_fetches_what_it_can_show_and_says_there_is_more() {
    let (_dir, mut ctx) = fixture();
    ctx.default_row_limit = Some(20);
    let out = sql(&ctx, "SELECT * FROM data").unwrap();
    let result = &out["result"];
    assert_eq!(result["rows"].as_array().unwrap().len(), 20);
    assert_eq!(result["total_rows_available"], Value::Null);
    assert!(result["note"].as_str().unwrap().contains("did not fetch"));
}

#[test]
fn it_only_reads_whatever_the_profile_and_the_connection_allow() {
    let (dir, mut ctx) = fixture();
    ctx.read_only = false;
    ctx.allow_existing_writes = true;
    let db = ctx.open_tabs[0].db.as_mut().unwrap();
    db.source.as_mut().unwrap().conn.allow_writes = true;
    for query in [
        "DELETE FROM data",
        "DROP TABLE sales",
        "CREATE TABLE copy AS SELECT * FROM data",
        "WITH x AS (SELECT 1) DELETE FROM sales",
    ] {
        assert!(sql(&ctx, query).is_err(), "{query}");
    }
    let after = duckdb::Connection::open(dir.path().join("server.duckdb")).unwrap();
    let rows: i64 = after
        .query_row("SELECT COUNT(*) FROM sales", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, ROWS);
}

#[test]
fn write_to_takes_the_whole_result_of_the_server_query() {
    let (dir, mut ctx) = fixture();
    ctx.read_only = false;
    let out_path = dir.path().join("big.csv");
    let out = run_sql::run(
        &ctx,
        &params(json!({
            "open_tab": "@active",
            "query": "SELECT id FROM data WHERE id > 200",
            "write_to": {"path": out_path},
        })),
    )
    .unwrap();
    assert_eq!(out["rows_written"], json!(300), "{out}");
    let lines = std::fs::read_to_string(&out_path).unwrap().lines().count();
    assert_eq!(lines, 301, "a header and every row");
}

/// The server's rows stop at the limit with no way to look one further, so a
/// result that reaches it may hold more, and a smaller one says nothing.
#[test]
fn write_to_says_when_the_server_read_reached_the_row_limit() {
    let (dir, mut ctx) = fixture();
    ctx.read_only = false;
    let _cap = octa::formats::ThreadRowCap::new(10);
    let save = |name: &str, query: &str| {
        run_sql::run(
            &ctx,
            &params(json!({
                "open_tab": "@active",
                "query": query,
                "write_to": {"path": dir.path().join(name)},
            })),
        )
        .unwrap()
    };

    let out = save("cut.csv", "SELECT id FROM data WHERE id > 200");
    assert_eq!(out["rows_written"], json!(10), "{out}");
    assert_eq!(out["initial_load_capped"], json!(true), "{out}");
    assert!(out.get("truncated").is_none(), "{out}");
    assert!(
        out["initial_load_note"]
            .as_str()
            .unwrap_or_default()
            .contains("unlimited"),
        "{out}"
    );

    let out = save("small.csv", "SELECT id FROM data WHERE id > 495");
    assert_eq!(out["rows_written"], json!(5), "{out}");
    assert!(out.get("initial_load_capped").is_none(), "{out}");
}

#[test]
fn another_tab_on_the_same_server_can_be_joined_and_a_local_one_cannot() {
    let (dir, mut ctx) = fixture();
    let conn = duckdb_connection(&dir.path().join("server.duckdb"));
    let other = SnapshotDb {
        table: "main.regions".into(),
        connection: "Warehouse".into(),
        source: Some(source(&conn, "regions")),
        edited: false,
    };
    ctx.open_tabs.push(tab("#2", "regions", other));
    let join = |ctx: &ToolContext| {
        run_sql::run(
            ctx,
            &params(json!({
                "open_tab": "#1",
                "query": "SELECT COUNT(*) FROM data JOIN r ON data.id = r.id",
                "extra_tables": [{"name": "r", "path": "#2"}],
            })),
        )
    };
    let out = join(&ctx).unwrap();
    assert_eq!(out["result"]["rows"][0][0], json!(3));

    ctx.open_tabs[1].db.as_mut().unwrap().source = None;
    let err = join(&ctx).unwrap_err();
    assert!(format!("{err:#}").contains("query_db"), "{err:#}");
}

#[test]
fn a_filter_the_tab_has_on_the_server_goes_along() {
    let (_dir, mut ctx) = fixture();
    let db = ctx.open_tabs[0].db.as_mut().unwrap();
    db.source.as_mut().unwrap().filter = Some("id <= 10".into());
    let count = count_rows::run(&ctx, &params(json!({"open_tab": "@active"}))).unwrap();
    assert_eq!(count["row_count"], json!(10));
    let out = sql(&ctx, "SELECT COUNT(*) FROM data").unwrap();
    assert_eq!(out["result"]["rows"][0][0], json!(10));
}

#[test]
fn the_sandbox_seals_a_duckdb_file_whose_sql_reads_and_writes_any_file() {
    let (dir, mut ctx) = fixture();
    let secret = dir.path().join("secret.csv");
    std::fs::write(&secret, "a\n42\n").unwrap();
    let read = format!("SELECT * FROM read_csv('{}')", secret.display());
    let query_db = |ctx: &ToolContext, sql: &str| {
        query_db::run(ctx, &params(json!({"connection": "Warehouse", "sql": sql})))
    };

    // The plain MCP server has a trusted client: this is the control.
    ctx.read_only = false;
    ctx.restrict_filesystem = false;
    let open = query_db(&ctx, &read).unwrap();
    assert_eq!(open["rows"][0][0], json!(42), "{open}");

    // The assistant, whatever it is allowed to change.
    ctx.restrict_filesystem = true;
    ctx.db_connections[0].allow_writes = true;
    let copied = dir.path().join("copied.csv");
    let copy = format!("COPY (SELECT 1) TO '{}'", copied.display());
    assert!(query_db(&ctx, &read).is_err(), "reads another file");
    assert!(query_db(&ctx, &copy).is_err(), "writes another file");
    assert!(!copied.exists());
    assert!(sql(&ctx, &read).is_err(), "and through run_sql");

    // Its own tables still answer, on the server.
    let own = query_db(&ctx, "SELECT COUNT(*) FROM main.sales").unwrap();
    assert_eq!(own["rows"][0][0], json!(ROWS), "{own}");
    let out = sql(&ctx, "SELECT COUNT(*) FROM data").unwrap();
    assert_eq!(out["result"]["rows"][0][0], json!(ROWS));
    assert_eq!(out["on_server"], json!(true));

    // A read-only MCP server is sealed the same way.
    ctx.restrict_filesystem = false;
    ctx.read_only = true;
    assert!(query_db(&ctx, &read).is_err());
}

#[test]
fn a_sandboxed_sqlite_file_cannot_attach_another() {
    let dir = tempfile::tempdir().unwrap();
    let (main, other) = (dir.path().join("a.sqlite"), dir.path().join("b.sqlite"));
    for path in [&main, &other] {
        rusqlite::Connection::open(path)
            .unwrap()
            .execute_batch("CREATE TABLE t (id INTEGER)")
            .unwrap();
    }
    let conn = octa::db::DbConnection::file(
        "octa-test-attach".into(),
        "Lite",
        octa::db::DbEngine::Sqlite,
        main.to_str().unwrap(),
        true,
    );
    let attach = format!("ATTACH '{}' AS other", other.display());
    let mut open = octa::db::connect(&conn, None, None).unwrap();
    open.execute("BEGIN").unwrap();
    assert!(open.execute(&attach).is_ok(), "the control");
    open.execute("ROLLBACK").unwrap();

    let mut sealed = octa::db::connect_sandboxed(&conn, None, None).unwrap();
    sealed.execute("BEGIN").unwrap();
    assert!(sealed.execute(&attach).is_err());
    sealed.execute("ROLLBACK").unwrap();
    assert!(
        sealed.query("SELECT COUNT(*) FROM t").is_ok(),
        "its own table"
    );
}

/// What every dialect would be sent, without a server: the statement is built the
/// same way and the write gate reads it the same way on each engine. Whether a
/// server takes the SQL is `mod_db_tab_live_tests.rs`.
#[test]
fn every_dialect_gets_a_read_through_the_gate_and_refuses_a_write() {
    let reads = [
        "SELECT COUNT(*) FROM data",
        "SELECT * FROM data;",
        "WITH hi AS (SELECT * FROM data) SELECT COUNT(*) FROM hi",
    ];
    let writes = [
        "DELETE FROM data",
        "DROP TABLE sales",
        "UPDATE data SET id = 1",
        "WITH hi AS (SELECT 1) DELETE FROM sales",
        "CREATE TABLE copy AS SELECT * FROM data",
    ];
    for engine in octa::db::DbEngine::ALL {
        let conn = octa::db::DbConnection {
            engine: *engine,
            allow_writes: false,
            ..octa::db::DbConnection::file("c".into(), "C", *engine, "x", false)
        };
        let ctx = ToolContext::for_mcp(None, 0, false, true, vec![conn.clone()], true, 0);
        let src = source(&conn, "sales");
        let statement = |query: &str| {
            let p: run_sql::Params = params(json!({"open_tab": "#1", "query": query}));
            run_sql::server_statement(&ctx, &p, &src).unwrap()
        };
        for query in reads {
            let sql = statement(query);
            let quoted = engine.quote_ident("sales");
            assert!(sql.starts_with("WITH data AS (SELECT * FROM "), "{sql}");
            assert!(sql.contains(&quoted), "{engine:?}: {sql}");
            assert!(!sql.contains(';'), "{engine:?}: {sql}");
            let gate = run_sql::server_gate(&src, query, &sql);
            assert!(gate.is_ok(), "{engine:?} refused a read: {sql}: {gate:?}");
        }
        for query in writes {
            let sql = statement(query);
            let refused = run_sql::server_gate(&src, query, &sql).is_err();
            // On these the server refuses it too, and the live tests run that.
            let server_refuses = engine.enforces_read_only() || engine.is_file();
            assert!(refused || server_refuses, "{engine:?} would run: {sql}");
        }
    }
}

#[test]
fn the_analyses_run_on_the_server() {
    let (_dir, ctx) = fixture();
    let _limit = ThreadRowCap::new(FETCH_LIMIT);

    let count = count_rows::run(&ctx, &params(json!({"open_tab": "@active"}))).unwrap();
    assert_eq!(count["row_count"], json!(ROWS));
    assert_eq!(count["on_server"], json!(true));

    let frequency = value_frequency::run(
        &ctx,
        &params(json!({"open_tab": "@active", "column": "id", "top_n": 5})),
    )
    .unwrap();
    assert_eq!(frequency["unique_count"], json!(ROWS));
    assert_eq!(frequency["rows"].as_array().unwrap().len(), 5);

    let profile = profile::run(&ctx, &params(json!({"open_tab": "@active"}))).unwrap();
    assert_eq!(profile["row_count"], json!(ROWS));
    let id = &profile["columns"][0];
    assert!(id.to_string().contains("500"), "the largest id: {id}");

    let drawn = sample::run(&ctx, &params(json!({"open_tab": "@active", "limit": 7}))).unwrap();
    assert_eq!(drawn["rows"].as_array().unwrap().len(), 7);
    assert_eq!(drawn["on_server"], json!(true));

    let quality = quality_report::run(&ctx, &params(json!({"open_tab": "@active"}))).unwrap();
    assert_eq!(quality["columns"]["row_count"], json!(4), "{quality}");
    assert_eq!(quality["on_server"], json!(true));
    assert!(quality["from_loaded_rows"].is_array(), "{quality}");
}

#[test]
fn the_reshaping_analyses_run_on_the_server() {
    let (_dir, ctx) = fixture();
    let _limit = ThreadRowCap::new(FETCH_LIMIT);
    let total = |rows: &Value, col: usize| -> f64 {
        rows.as_array()
            .unwrap()
            .iter()
            .map(|r| r[col].as_f64().unwrap_or(0.0))
            .sum()
    };

    let corr = correlation::run(&ctx, &params(json!({"open_tab": "@active"}))).unwrap();
    assert_eq!(corr["on_server"], json!(true));
    let r = corr["matrix"][0][1].as_f64().unwrap();
    assert!((r - 1.0).abs() < 1e-9, "{corr}");

    let wide = pivot::run(
        &ctx,
        &params(json!({
            "open_tab": "@active", "on": "grp", "value": "amount", "agg": "sum", "group": ["day"],
        })),
    )
    .unwrap();
    assert_eq!(wide["on_server"], json!(true), "{wide}");
    assert_eq!(wide["rows"].as_array().unwrap().len(), 10, "{wide}");
    let summed = (1..=3).map(|col| total(&wide["rows"], col)).sum::<f64>();
    assert_eq!(summed, (ROWS * (ROWS + 1)) as f64, "every row: {wide}");

    let daily = resample::run(
        &ctx,
        &params(json!({
            "open_tab": "@active", "time_col": "day", "value_cols": ["amount"], "agg": "sum",
        })),
    )
    .unwrap();
    assert_eq!(daily["on_server"], json!(true), "{daily}");
    assert_eq!(daily["rows"].as_array().unwrap().len(), 10);
    assert_eq!(total(&daily["rows"], 1), (ROWS * (ROWS + 1)) as f64);

    let rolled = rolling_window::run(
        &ctx,
        &params(json!({
            "open_tab": "@active", "order_col": "id", "value_col": "amount", "window": 2,
            "agg": "sum", "limit": 5,
        })),
    )
    .unwrap();
    assert_eq!(rolled["on_server"], json!(true), "{rolled}");
    assert_eq!(rolled["rows"].as_array().unwrap().len(), 5);
    assert_eq!(rolled["rows"][1][4].as_f64(), Some(6.0), "2 + 4: {rolled}");
    assert_eq!(rolled["total_rows_available"], Value::Null, "{rolled}");
}

#[test]
fn what_the_server_cannot_do_stays_on_the_loaded_rows_and_says_so() {
    let (_dir, ctx) = fixture();
    let out = dispatch(
        &ctx,
        "pivot",
        json!({
            "open_tab": "@active", "mode": "unpivot", "columns": ["id", "amount"],
        }),
    )
    .unwrap();
    assert!(out["loaded_rows_note"].is_string(), "{out}");
    let text_date = dispatch(
        &ctx,
        "resample_timeseries",
        json!({"open_tab": "@active", "time_col": "grp", "value_cols": ["amount"]}),
    );
    if let Ok(out) = text_date {
        assert!(out["loaded_rows_note"].is_string(), "{out}");
    }
}

#[test]
fn a_column_of_ids_cannot_fetch_every_value() {
    let (_dir, mut ctx) = fixture();
    ctx.default_row_limit = Some(10);
    let out = value_frequency::run(
        &ctx,
        &params(json!({"open_tab": "@active", "column": "id"})),
    )
    .unwrap();
    assert_eq!(out["rows"].as_array().unwrap().len(), 10);
    assert_eq!(out["unique_count"], json!(ROWS));
}

#[test]
fn read_table_pages_and_sorts_on_the_server() {
    let (_dir, ctx) = fixture();
    let out = read_table::run(&ctx, &params(json!({"open_tab": "@active", "limit": 150}))).unwrap();
    assert_eq!(
        out["rows"].as_array().unwrap().len(),
        150,
        "more than loaded"
    );
    assert_eq!(out["total_rows_available"], Value::Null, "{out}");

    let top = read_table::run(
        &ctx,
        &params(json!({
            "open_tab": "@active",
            "limit": 3,
            "sort": [{"column": "id", "descending": true}],
        })),
    )
    .unwrap();
    assert_eq!(top["rows"][0][0], json!(ROWS), "a row the tab never loaded");
}

#[test]
fn asking_for_every_row_does_not_fetch_the_table() {
    let (_dir, ctx) = fixture();
    for args in [
        json!({"open_tab": "@active", "limit": 0}),
        json!({"open_tab": "@active", "unlimited": true, "limit": 0}),
    ] {
        let out = read_table::run(&ctx, &params(args)).unwrap();
        assert_eq!(out["rows"].as_array().unwrap().len(), LOADED as usize);
    }
    let out = sample::run(&ctx, &params(json!({"open_tab": "@active", "limit": 0}))).unwrap();
    assert_eq!(out["rows"].as_array().unwrap().len(), LOADED as usize);
}

#[test]
fn the_columns_come_from_the_tab_without_asking_the_server() {
    let (_dir, mut ctx) = fixture();
    point_at_nothing(&mut ctx);
    let out = schema::run(&ctx, &params(json!({"open_tab": "@active"}))).unwrap();
    assert_eq!(out["column_count"], json!(4));
}

#[test]
fn a_tool_with_no_server_path_says_it_only_saw_the_loaded_rows() {
    let (_dir, ctx) = fixture();
    let _limit = ThreadRowCap::new(FETCH_LIMIT);
    let out = dispatch(&ctx, "tail", json!({"open_tab": "@active", "limit": 3})).unwrap();
    assert_eq!(out["rows"][2][0], json!(LOADED), "the tab's last row");
    let note = out["loaded_rows_note"].as_str().unwrap();
    assert!(note.contains("100") && note.contains("run_sql"), "{note}");

    let next = dispatch(&ctx, "count_rows", json!({"open_tab": "@active"})).unwrap();
    assert!(next.get("loaded_rows_note").is_none(), "taken once: {next}");
}

#[test]
fn a_tab_with_unsaved_edits_stays_on_its_own_rows_and_says_so() {
    let (_dir, mut ctx) = fixture();
    stays_on_loaded_rows(&mut ctx, true);
    ctx.db_connections.clear();
    let count = dispatch(&ctx, "count_rows", json!({"open_tab": "@active"})).unwrap();
    assert_eq!(count["row_count"], json!(LOADED));
    let note = count["loaded_rows_note"].as_str().unwrap();
    assert!(note.contains("unsaved"), "{note}");
}

#[test]
fn with_analyses_on_the_database_off_tools_stay_on_the_loaded_rows() {
    let (_dir, mut ctx) = fixture();
    stays_on_loaded_rows(&mut ctx, false);
    let out = dispatch(
        &ctx,
        "run_sql",
        json!({"open_tab": "@active", "query": "SELECT COUNT(*) FROM data"}),
    )
    .unwrap();
    assert_eq!(out["result"]["rows"][0][0], json!(LOADED));
    let note = out["loaded_rows_note"].as_str().unwrap();
    assert!(note.contains("query_db"), "{note}");
}

#[test]
fn a_lost_server_is_an_error_not_an_answer_from_the_loaded_rows() {
    let (_dir, mut ctx) = fixture();
    point_at_nothing(&mut ctx);
    assert!(sql(&ctx, "SELECT COUNT(*) FROM data").is_err());
    assert!(count_rows::run(&ctx, &params(json!({"open_tab": "@active"}))).is_err());
}

#[test]
fn the_model_is_told_where_the_table_is_and_which_sql_to_write() {
    let (_dir, mut ctx) = fixture();
    let summaries = ctx.open_tab_summaries();
    assert_eq!(summaries[0]["row_count"], json!(LOADED));
    let note = summaries[0]["note"].as_str().unwrap();
    let engine = octa::db::DbEngine::DuckDb.label();
    let words = [
        "100",
        "main.sales",
        "Warehouse",
        "run_sql",
        engine,
        "edit_open_tab",
    ];
    for word in words {
        assert!(note.contains(word), "{word} is missing from: {note}");
    }

    stays_on_loaded_rows(&mut ctx, true);
    let summaries = ctx.open_tab_summaries();
    assert!(summaries[0]["note"].as_str().unwrap().contains("unsaved"));
    stays_on_loaded_rows(&mut ctx, false);
    let summaries = ctx.open_tab_summaries();
    assert!(summaries[0]["note"].as_str().unwrap().contains("query_db"));
}

#[test]
fn an_edit_says_it_only_reached_the_loaded_rows() {
    let (_dir, mut ctx) = fixture();
    ctx.allow_existing_writes = true;
    ctx.pending_tab_edits = Some(std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
    let out = edit_open_tab::run(
        &ctx,
        &params(json!({"open_tab": "#1", "ops": [{"op": "delete_rows", "rows": [0]}]})),
    )
    .unwrap();
    let note = out["note"].as_str().unwrap();
    assert!(note.contains("100"), "{note}");
}

#[test]
fn the_row_cap_of_one_thread_leaves_the_others_alone() {
    let global = octa::formats::initial_load_rows();
    let limit = ThreadRowCap::new(3);
    assert_eq!(octa::formats::initial_load_rows(), 3);
    let elsewhere = std::thread::spawn(octa::formats::initial_load_rows)
        .join()
        .unwrap();
    assert_eq!(elsewhere, global);
    drop(limit);
    assert_eq!(octa::formats::initial_load_rows(), global);
}
