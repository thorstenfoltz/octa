//! The assistant's tools on a database tab against real servers, one engine per
//! `OCTA_TEST_*_URL` (the variables of `tests/db_live_tests.rs`), a no-op when
//! unset. Every SQL dialect the tools reach is exercised end to end here.
//!
//! Databricks has no entry and must not get one: those tests would run on a real
//! warehouse. Its SQL is checked locally against Apache Spark instead.

use super::*;
use crate::app::chat::tools::dispatch;
use octa::data::ColumnInfo;
use octa::db::DbEngine;
use octa::db::pushdown::ServerSource;
use octa::db::pushdown::view::{ServerView, page_sql};
use serde_json::json;

const ROWS: i64 = 250;
const LOADED: usize = 50;
const TABLE: &str = "octa_tab_probe";
/// The sum of `amount`, which is the row's id times 1.5.
const AMOUNT_SUM: f64 = 47_062.5;

fn conn_from_env(var: &str, engine: DbEngine) -> Option<(octa::db::DbConnection, String)> {
    let raw = std::env::var(var).ok()?;
    let part = |key: &str| {
        raw.split(';')
            .filter_map(|p| p.split_once('='))
            .find(|(k, _)| k.trim() == key)
            .map(|(_, v)| v.trim().to_string())
            .unwrap_or_default()
    };
    let conn = octa::db::DbConnection {
        host: part("host"),
        port: part("port").parse().unwrap_or(engine.default_port()),
        username: part("user"),
        ..octa::db::DbConnection::file(
            format!("test-{var}"),
            engine.label(),
            engine,
            &part("db"),
            true,
        )
    };
    Some((conn, part("pass")))
}

/// Where the probe table lives on `engine`, and the statements that make it.
fn layout(conn: &octa::db::DbConnection) -> (Option<String>, String, Vec<String>) {
    let columns =
        |text: &str, float: &str| format!("id BIGINT, grp {text}, amount {float}, day DATE");
    match conn.engine {
        DbEngine::Postgres | DbEngine::Redshift => (
            None,
            "public".into(),
            vec![format!(
                "CREATE TABLE public.{TABLE} ({})",
                columns("VARCHAR(8)", "DOUBLE PRECISION")
            )],
        ),
        DbEngine::MySql => (
            None,
            conn.database.clone(),
            vec![format!(
                "CREATE TABLE {TABLE} ({})",
                columns("VARCHAR(8)", "DOUBLE")
            )],
        ),
        DbEngine::Mssql => (
            None,
            "dbo".into(),
            vec![format!(
                "CREATE TABLE dbo.{TABLE} ({})",
                columns("VARCHAR(8)", "FLOAT")
            )],
        ),
        DbEngine::ClickHouse => (
            None,
            "default".into(),
            vec![format!(
                "CREATE TABLE default.{TABLE} (id Int64, grp String, amount Float64, day Date) \
                 ENGINE = Memory"
            )],
        ),
        DbEngine::Trino => (
            Some("memory".into()),
            "octa".into(),
            vec![
                "CREATE SCHEMA IF NOT EXISTS memory.octa".into(),
                format!(
                    "CREATE TABLE memory.octa.{TABLE} ({})",
                    columns("VARCHAR", "DOUBLE")
                ),
            ],
        ),
        DbEngine::Oracle => (
            None,
            conn.username.to_uppercase(),
            vec![format!(
                "CREATE TABLE {TABLE} (id NUMBER(19), grp VARCHAR2(8), amount NUMBER(12, 2), day DATE)"
            )],
        ),
        other => panic!("no layout for {other:?}"),
    }
}

fn probe_rows() -> DataTable {
    let mut data = DataTable::empty();
    data.columns = [
        ("id", "Int64"),
        ("grp", "Utf8"),
        ("amount", "Float64"),
        ("day", "Date32"),
    ]
    .map(|(name, data_type)| ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
    })
    .to_vec();
    let start = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    data.rows = (1..=ROWS)
        .map(|i| {
            vec![
                CellValue::Int(i),
                CellValue::String(["a", "b", "c"][(i % 3) as usize].into()),
                CellValue::Float(i as f64 * 1.5),
                CellValue::Date((start + chrono::Days::new((i % 10) as u64)).to_string()),
            ]
        })
        .collect();
    data
}

fn number(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())
}

fn rows_of(v: &Value) -> usize {
    v["rows"].as_array().map_or(0, Vec::len)
}

/// Every tool with a server path, on one engine: one line per tool, and the list
/// of what went wrong.
fn exercise(ctx: &ToolContext, column: &dyn Fn(&str) -> String) -> Vec<String> {
    let (grp, amount, day, id) = (column("grp"), column("amount"), column("day"), column("id"));
    type Check = Box<dyn Fn(&Value) -> bool>;
    let on_server = |v: &Value| v["on_server"] == json!(true);
    let checks: Vec<(&str, &str, Value, Check)> = vec![
        (
            "schema",
            "columns",
            json!({}),
            Box::new(|v| v["column_count"] == json!(4)),
        ),
        (
            "count_rows",
            "total",
            json!({}),
            Box::new(move |v| number(&v["row_count"]) == Some(ROWS as f64) && on_server(v)),
        ),
        (
            "run_sql",
            "aggregate",
            json!({"query": "SELECT COUNT(*), SUM(amount) FROM data"}),
            Box::new(|v| {
                let row = &v["result"]["rows"][0];
                number(&row[0]) == Some(ROWS as f64) && number(&row[1]) == Some(AMOUNT_SUM)
            }),
        ),
        (
            "run_sql",
            "own WITH",
            json!({"query": "WITH hi AS (SELECT * FROM data WHERE id > 240) SELECT COUNT(*) FROM hi;"}),
            Box::new(|v| number(&v["result"]["rows"][0][0]) == Some(10.0)),
        ),
        (
            "run_sql",
            "SELECT *",
            json!({"query": "SELECT * FROM data"}),
            Box::new(|v| rows_of(&v["result"]) == 20 && v["result"]["note"].is_string()),
        ),
        (
            "value_frequency",
            "counts",
            json!({"column": grp}),
            Box::new(|v| v["unique_count"] == json!(3) && v["total_non_null"] == json!(ROWS)),
        ),
        (
            "read_table",
            "first rows",
            json!({"limit": 60}),
            Box::new(move |v| rows_of(v) == 60 && on_server(v)),
        ),
        (
            "read_table",
            "sorted",
            json!({"limit": 3, "sort": [{"column": id, "descending": true}]}),
            Box::new(|v| number(&v["rows"][0][0]) == Some(ROWS as f64)),
        ),
        (
            "sample",
            "draw",
            json!({"limit": 7}),
            Box::new(move |v| rows_of(v) >= 1 && on_server(v)),
        ),
        (
            "profile",
            "summary",
            json!({}),
            Box::new(move |v| {
                // The sum as well as the count: a dialect's cast that the driver
                // cannot read back only shows in the values.
                let sum = v["columns"].as_array().into_iter().flatten().find(|c| {
                    c["column_name"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case("amount"))
                });
                v["row_count"] == json!(ROWS)
                    && on_server(v)
                    && sum.is_some_and(|c| number(&c["sum"]) == Some(AMOUNT_SUM))
            }),
        ),
        (
            "correlation",
            "pearson",
            json!({}),
            Box::new(move |v| {
                on_server(v)
                    && v["matrix"].as_array().is_some_and(|m| m.len() >= 2)
                    && number(&v["matrix"][0][1]).is_some_and(|r| (r - 1.0).abs() < 1e-6)
            }),
        ),
        (
            "correlation",
            "spearman",
            json!({"method": "spearman"}),
            Box::new(move |v| {
                on_server(v) && number(&v["matrix"][0][1]).is_some_and(|r| (r - 1.0).abs() < 1e-6)
            }),
        ),
        (
            "pivot",
            "sum by group",
            json!({"on": grp, "value": amount, "agg": "sum", "group": [day]}),
            Box::new(move |v| {
                let sum: f64 = v["rows"].as_array().map_or(0.0, |rows| {
                    rows.iter()
                        .flat_map(|r| (1..=3).filter_map(|c| number(&r[c])))
                        .sum()
                });
                rows_of(v) == 10 && sum == AMOUNT_SUM && on_server(v)
            }),
        ),
        (
            "resample_timeseries",
            "by day",
            json!({"time_col": day, "value_cols": [amount], "agg": "sum"}),
            Box::new(move |v| {
                let sum: f64 = v["rows"]
                    .as_array()
                    .map_or(0.0, |rows| rows.iter().filter_map(|r| number(&r[1])).sum());
                rows_of(v) == 10 && sum == AMOUNT_SUM && on_server(v)
            }),
        ),
        (
            "rolling_window",
            "last two",
            json!({"order_col": id, "value_col": amount, "window": 2, "agg": "sum", "limit": 5}),
            Box::new(move |v| rows_of(v) == 5 && on_server(v)),
        ),
        (
            "quality_report",
            "report",
            json!({}),
            Box::new(move |v| v["columns"]["row_count"] == json!(4) && on_server(v)),
        ),
    ];
    let mut failures = Vec::new();
    for (tool, label, mut args, check) in checks {
        args["open_tab"] = json!("@active");
        let outcome = dispatch(ctx, tool, args);
        let verdict = match &outcome {
            Ok(v) if check(v) && v.get("loaded_rows_note").is_none() => "ok".to_string(),
            Ok(v) => format!(
                "WRONG {}",
                v.to_string().chars().take(420).collect::<String>()
            ),
            Err(e) => format!("ERROR {}", e.chars().take(420).collect::<String>()),
        };
        println!("    {tool} ({label}): {verdict}");
        if verdict != "ok" {
            failures.push(format!("{tool} ({label}): {verdict}"));
        }
    }
    // Refused by Octa or by the server, and the rows are all still there.
    let writes = ["DELETE FROM data", "DROP TABLE data"];
    for query in writes {
        let refused = dispatch(
            ctx,
            "run_sql",
            json!({"open_tab": "@active", "query": query}),
        );
        println!(
            "    run_sql ({query}): {}",
            if refused.is_err() { "refused" } else { "RAN" }
        );
        if refused.is_ok() {
            failures.push(format!("{query} ran"));
        }
    }
    let after = dispatch(ctx, "count_rows", json!({"open_tab": "@active"}));
    if after.as_ref().map(|v| number(&v["row_count"])) != Ok(Some(ROWS as f64)) {
        failures.push(format!("rows after the writes: {after:?}"));
    }
    failures
}

#[test]
fn every_dialect_answers_on_the_server_live() {
    let engines = [
        ("OCTA_TEST_POSTGRES_URL", DbEngine::Postgres),
        ("OCTA_TEST_MYSQL_URL", DbEngine::MySql),
        ("OCTA_TEST_MSSQL_URL", DbEngine::Mssql),
        ("OCTA_TEST_CLICKHOUSE_URL", DbEngine::ClickHouse),
        ("OCTA_TEST_TRINO_URL", DbEngine::Trino),
        ("OCTA_TEST_ORACLE_URL", DbEngine::Oracle),
    ];
    let mut failures = Vec::new();
    for (var, engine) in engines {
        let Some((conn, secret)) = conn_from_env(var, engine) else {
            continue;
        };
        println!("  {}", engine.label());
        let (catalog, schema, create) = layout(&conn);
        let src = ServerSource {
            conn: octa::db::DbConnection {
                allow_writes: false,
                ..conn.clone()
            },
            catalog: catalog.clone(),
            schema: schema.clone(),
            // As the sidebar lists it: Oracle folds an unquoted name to upper case.
            table: if engine == DbEngine::Oracle {
                TABLE.to_uppercase()
            } else {
                TABLE.into()
            },
            filter: None,
            derived: Vec::new(),
        };
        let mut setup = octa::db::connect(&conn, Some(&secret), None).expect("connect");
        // A failed statement costs Oracle's driver its connection, so its DROP cannot
        // be allowed to fail.
        let drop_probe = if engine == DbEngine::Oracle {
            format!(
                "BEGIN EXECUTE IMMEDIATE 'DROP TABLE {TABLE}'; EXCEPTION WHEN OTHERS THEN \
                 IF SQLCODE != -942 THEN RAISE; END IF; END;"
            )
        } else {
            format!("DROP TABLE IF EXISTS {}", src.table_sql())
        };
        for statement in create {
            if statement.contains("SCHEMA") {
                setup.execute(&statement).expect("create the schema");
            } else {
                setup.execute(&drop_probe).expect("drop the probe table");
                setup.execute(&statement).expect("create the probe table");
            }
        }
        setup
            .write_table(
                catalog.as_deref(),
                &schema,
                &src.table,
                octa::db::DbWriteMode::Append,
                &probe_rows(),
            )
            .expect("fill the probe table");
        let view = ServerView::default();
        let page = setup
            .query(&page_sql(engine, &view, &src.from_sql(), &[], LOADED, 0))
            .expect("the tab's first page");
        assert_eq!(page.row_count(), LOADED);

        let mut settings = octa::ui::settings::AppSettings::default();
        settings.db_secrets.insert(conn.id.clone(), secret.clone());
        let mut ctx =
            ToolContext::for_mcp(Some(20), 65536, false, true, vec![conn.clone()], true, 0);
        ctx.cloud_settings = Some(settings);
        ctx.restrict_filesystem = true;
        // Oracle folds names to upper case: name them as the server spells them.
        let names: Vec<String> = page.columns.iter().map(|c| c.name.clone()).collect();
        let column = move |name: &str| {
            names
                .iter()
                .find(|n| n.eq_ignore_ascii_case(name))
                .cloned()
                .unwrap_or_else(|| name.to_string())
        };
        ctx.open_tabs = vec![TableSnapshot {
            handle: "#1".into(),
            display_name: TABLE.into(),
            source_path: None,
            table: page,
            window: None,
            read_only: true,
            db: Some(SnapshotDb {
                table: format!("{schema}.{TABLE}"),
                connection: conn.name.clone(),
                source: Some(src.clone()),
                edited: false,
            }),
        }];
        ctx.active_tab = Some(0);
        let found = exercise(&ctx, &column);
        failures.extend(
            found
                .into_iter()
                .map(|f| format!("{}: {f}", engine.label())),
        );
        let _ = setup.execute(&drop_probe);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
