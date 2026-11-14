//! `run_sql` takes SQL the caller wrote, and DuckDB SQL can read and write files
//! by itself. "Not allowed" has to hold against every way of asking.

use std::path::{Path, PathBuf};

use super::*;

/// A read-only session, as `--mcp-read-only` and a chat profile without writes
/// both get. `large_min`: files at least this big are scanned, 0 = always loaded.
fn read_only_ctx(large_min: usize) -> ToolContext {
    ToolContext::for_mcp(Some(1000), 65536, false, false, vec![], true, large_min)
}

/// A chat with "Allow writes" on: writes go through tools, reads stay confined.
fn chat_with_writes(readable: &Path, export_dir: &Path) -> ToolContext {
    ToolContext {
        loaded_rows_note: Default::default(),
        restrict_filesystem: true,
        allowed_read_paths: vec![readable.canonicalize().unwrap()],
        export_dir: Some(export_dir.to_path_buf()),
        allow_existing_writes: true,
        ..ToolContext::for_mcp(Some(1000), 65536, true, false, vec![], false, 0)
    }
}

fn params(path: &Path, query: &str) -> Params {
    serde_json::from_value(serde_json::json!({ "path": path, "query": query })).unwrap()
}

fn csv_in(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, "id,name\n1,a\n2,b\n").unwrap();
    path
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// Every statement that could reach outside, each given its own target.
fn escapes(out: &Path) -> Vec<String> {
    let at = |name: &str| out.join(name).to_string_lossy().to_string();
    vec![
        format!("COPY (SELECT 1 AS a) TO '{}'", at("copy.csv")),
        format!("COPY data TO '{}' (FORMAT PARQUET)", at("copy.parquet")),
        format!("ATTACH '{}' AS x", at("attach.duckdb")),
        format!(
            "ATTACH '{}' AS x; CREATE TABLE x.t AS SELECT 1 AS a",
            at("chain.duckdb")
        ),
        format!("SELECT 1; COPY (SELECT 1 AS a) TO '{}'", at("second.csv")),
        format!(
            "EXPLAIN ANALYZE COPY (SELECT 1 AS a) TO '{}'",
            at("analyze.csv")
        ),
        format!("EXPORT DATABASE '{}'", at("export")),
        format!(
            "  /* note */ COPY (SELECT 1 AS a) TO '{}'",
            at("comment.csv")
        ),
    ]
}

#[test]
fn no_statement_writes_anything_in_a_read_only_session() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let out = tempfile::tempdir().unwrap();
    let ctx = read_only_ctx(0);
    for query in escapes(out.path()) {
        // Whether DuckDB refuses it or skips it, nothing may land on disk.
        let _ = run(&ctx, &params(&source, &query));
        assert_eq!(files_in(out.path()), Vec::<String>::new(), "{query}");
    }
}

#[test]
fn a_statement_that_changes_something_gets_a_plain_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let ctx = read_only_ctx(0);
    for query in [
        "COPY data TO 'x.csv'",
        "UPDATE data SET name = 'z'",
        "CREATE TABLE t AS SELECT 1",
        "SET enable_external_access = true",
        "ATTACH 'x.duckdb'",
    ] {
        let err = format!("{:#}", run(&ctx, &params(&source, query)).unwrap_err());
        assert!(err.contains("writes are off"), "{query}: {err}");
    }
}

#[test]
fn asking_for_a_plan_writes_nothing_either() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let out = tempfile::tempdir().unwrap();
    let ctx = read_only_ctx(0);
    for query in escapes(out.path()) {
        let mut p = params(&source, &query);
        p.explain = true;
        let _ = run(&ctx, &p);
        assert_eq!(files_in(out.path()), Vec::<String>::new(), "{query}");
    }
}

#[test]
fn the_seal_cannot_be_lifted_from_inside() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.seal(None).unwrap();
    for query in [
        "SET enable_external_access = true",
        "SET lock_configuration = false",
        "RESET lock_configuration",
    ] {
        assert!(ws.execute(query).is_err(), "{query}");
    }
}

/// A big file stays readable through its view, and that exception must not turn
/// into a way to overwrite it.
#[test]
fn a_scanned_file_can_be_read_and_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let csv = csv_in(dir.path(), "big.csv");
    let parquet = dir.path().join("big.parquet");
    let mut ws = SqlWorkspace::new().unwrap();
    ws.execute(&format!(
        "COPY (SELECT 1 AS id UNION ALL SELECT 2) TO '{}'",
        parquet.display()
    ))
    .unwrap();

    let ctx = read_only_ctx(1);
    for source in [&csv, &parquet] {
        let before = std::fs::read(source).unwrap();
        let out = run(&ctx, &params(source, "SELECT count(*) AS n FROM data")).unwrap();
        assert_eq!(out["streamed"], Value::Bool(true), "{}", source.display());
        assert_eq!(
            out["result"]["rows"][0][0].to_string().trim_matches('"'),
            "2"
        );

        for query in [
            format!("COPY (SELECT 9 AS id) TO '{}'", source.display()),
            format!("SELECT 1; COPY (SELECT 9 AS id) TO '{}'", source.display()),
            format!(
                "EXPLAIN ANALYZE COPY (SELECT 9 AS id) TO '{}'",
                source.display()
            ),
        ] {
            let _ = run(&ctx, &params(source, &query));
            assert_eq!(std::fs::read(source).unwrap(), before, "{query}");
        }
    }
}

/// A relative path has to reach the seal's exception the way it reached the view.
#[test]
fn a_scanned_file_named_by_a_relative_path_stays_readable() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let source = csv_in(dir.path(), "relative.csv");
    let relative = source
        .strip_prefix(std::env::current_dir().unwrap())
        .unwrap_or(&source)
        .to_path_buf();
    let out = run(
        &read_only_ctx(1),
        &params(&relative, "SELECT count(*) AS n FROM data"),
    )
    .unwrap();
    assert_eq!(out["streamed"], Value::Bool(true));
}

#[test]
fn files_outside_the_source_cannot_be_read() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let other = csv_in(dir.path(), "secret.csv");
    let export = tempfile::tempdir().unwrap();
    let contexts = [read_only_ctx(0), chat_with_writes(&source, export.path())];
    for ctx in &contexts {
        for query in [
            format!("SELECT * FROM read_csv('{}')", other.display()),
            format!("SELECT * FROM '{}'", other.display()),
            format!("SELECT * FROM read_text('{}')", other.display()),
            format!("SELECT * FROM glob('{}/*')", dir.path().display()),
        ] {
            let result = run(ctx, &params(&source, &query));
            assert!(result.is_err(), "{query}: {result:?}");
        }
    }
}

#[test]
fn reading_still_works_in_a_sealed_session() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let ctx = read_only_ctx(0);
    for query in [
        "SELECT count(*) FROM data",
        "WITH t AS (SELECT * FROM data) SELECT * FROM t",
        "FROM data",
        "DESCRIBE data",
        "SUMMARIZE data",
        "SELECT name, count(*) FROM data GROUP BY name",
    ] {
        run(&ctx, &params(&source, query)).unwrap_or_else(|e| panic!("{query}: {e:#}"));
    }
    let mut plan = params(&source, "SELECT count(*) FROM data");
    plan.explain = true;
    run(&ctx, &plan).unwrap();
}

#[test]
fn extra_tables_and_attached_databases_still_work_when_sealed() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let extra = csv_in(dir.path(), "extra.csv");
    let db = dir.path().join("attached.duckdb");
    {
        let conn = duckdb::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE t AS SELECT 7 AS v")
            .unwrap();
    }
    let p: Params = serde_json::from_value(serde_json::json!({
        "path": source,
        "query": "SELECT (SELECT count(*) FROM extra) AS e, (SELECT v FROM a.main.t) AS v",
        "extra_tables": [{ "name": "extra", "path": extra }],
        "attach": [{ "alias": "a", "path": db }],
    }))
    .unwrap();
    let out = run(&read_only_ctx(0), &p).unwrap();
    assert_eq!(
        out["result"]["rows"][0][1].to_string().trim_matches('"'),
        "7"
    );
}

/// Writes on means writes through the tool, where the export folder and backups
/// apply, not through SQL of its own.
#[test]
fn a_chat_with_writes_on_writes_through_write_to_only() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let export = tempfile::tempdir().unwrap();
    let ctx = chat_with_writes(&source, export.path());

    let sneaky = export.path().join("sneaky.csv");
    let copy = format!("COPY (SELECT 1 AS a) TO '{}'", sneaky.display());
    assert!(run(&ctx, &params(&source, &copy)).is_err());
    assert!(!sneaky.exists());

    let p: Params = serde_json::from_value(serde_json::json!({
        "path": source,
        "query": "SELECT * FROM data",
        "write_to": { "path": "out.csv" },
    }))
    .unwrap();
    assert_eq!(run(&ctx, &p).unwrap()["rows_written"], 2);
    assert!(export.path().join("out.csv").exists());

    let p: Params = serde_json::from_value(serde_json::json!({
        "path": source,
        "query": "SELECT * FROM data",
        "write_to": { "path": "out.duckdb", "table": "t" },
    }))
    .unwrap();
    assert_eq!(run(&ctx, &p).unwrap()["rows_written"], 2);
}

fn saved_into(source: &Path, query: &str, target: &Path) -> Params {
    serde_json::from_value(serde_json::json!({
        "path": source,
        "query": query,
        "write_to": { "path": target, "table": "t" },
    }))
    .unwrap()
}

/// Saving a result is a write, but the SQL is still the caller's: it reads what it
/// was given and nothing else, whichever kind of database file it lands in.
#[test]
fn a_query_saved_into_a_database_cannot_reach_other_files() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let secret = csv_in(dir.path(), "secret.csv");
    let export = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let ctx = chat_with_writes(&source, export.path());
    for target in ["out.duckdb", "out.sqlite"] {
        let target = export.path().join(target);
        for query in [
            format!("SELECT * FROM read_csv('{}')", secret.display()),
            format!("SELECT * FROM '{}'", secret.display()),
            format!(
                "SELECT 1 AS a; COPY (SELECT 1 AS a) TO '{}'",
                out.path().join("copy.csv").display()
            ),
            format!(
                "ATTACH '{}' AS x; SELECT 1 AS a",
                out.path().join("attach.duckdb").display()
            ),
        ] {
            let result = run(&ctx, &saved_into(&source, &query, &target));
            assert!(result.is_err(), "{query}: {result:?}");
            assert_eq!(files_in(out.path()), Vec::<String>::new(), "{query}");
        }
    }
}

/// The target is named by the same call, but naming a file to write to is not
/// permission for the SQL to read what the file already holds, or to change it.
#[test]
fn a_query_saved_into_a_database_cannot_see_that_database() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let export = tempfile::tempdir().unwrap();
    let target = export.path().join("existing.duckdb");
    {
        let conn = duckdb::Connection::open(&target).unwrap();
        conn.execute_batch("CREATE TABLE private AS SELECT 7 AS v")
            .unwrap();
    }
    let ctx = chat_with_writes(&source, export.path());
    for query in [
        "SELECT * FROM __octa_write_target__.main.private",
        "SELECT * FROM existing.main.private",
        "DROP TABLE __octa_write_target__.main.private; SELECT 1 AS a",
    ] {
        let result = run(&ctx, &saved_into(&source, query, &target));
        assert!(result.is_err(), "{query}: {result:?}");
    }
    let conn = duckdb::Connection::open(&target).unwrap();
    let v: i32 = conn
        .query_row("SELECT v FROM private", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 7);
}

/// Running the query first must cost nothing: more rows than the initial-load cap,
/// and a DECIMAL still a DECIMAL rather than whatever a loaded table would make it.
#[test]
fn a_query_saved_into_a_database_keeps_every_row_and_type() {
    // Just over the default initial-load cap of 2,000,000.
    const ROWS: usize = 2_000_100;
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let export = tempfile::tempdir().unwrap();
    let target = export.path().join("big.duckdb");
    let ctx = chat_with_writes(&source, export.path());
    let p: Params = serde_json::from_value(serde_json::json!({
        "path": source,
        "query": format!("SELECT i, 1.50::DECIMAL(10,2) AS d FROM range({ROWS}) t(i)"),
        "write_to": {
            "path": target,
            "schema": "reports",
            "table": "t",
            "create_schema_if_missing": true,
        },
    }))
    .unwrap();
    let out = run(&ctx, &p).unwrap();
    assert_eq!(out["rows_written"], ROWS);
    assert_eq!(out["created_schema"], Value::Bool(true));

    let conn = duckdb::Connection::open(&target).unwrap();
    let (n, d_type): (i64, String) = conn
        .query_row(
            "SELECT count(*), any_value(typeof(d)) FROM reports.t",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(n as usize, ROWS);
    assert_eq!(d_type, "DECIMAL(10,2)");
}

#[test]
fn a_query_saved_into_a_database_honours_the_write_mode() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let export = tempfile::tempdir().unwrap();
    let ctx = chat_with_writes(&source, export.path());
    for target in ["modes.duckdb", "modes.sqlite"] {
        let target = export.path().join(target);
        let save = |mode: &str| {
            let p: Params = serde_json::from_value(serde_json::json!({
                "path": source,
                "query": "SELECT * FROM data",
                "write_to": { "path": target, "table": "t", "mode": mode },
            }))
            .unwrap();
            run(&ctx, &p)
        };
        assert_eq!(save("create").unwrap()["rows_written"], 2);
        assert!(save("create").is_err(), "the table already exists");
        assert_eq!(save("replace").unwrap()["rows_written"], 2);
        assert_eq!(save("append").unwrap()["rows_written"], 2);
    }
}

/// A big file is read through a view over the scan, so the file itself has to stay
/// readable from the sealed connection.
#[test]
fn a_scanned_source_can_be_saved_into_a_database() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "big.csv");
    let export = tempfile::tempdir().unwrap();
    let target = export.path().join("scanned.duckdb");
    let ctx = ToolContext {
        loaded_rows_note: Default::default(),
        restrict_filesystem: true,
        allowed_read_paths: vec![source.canonicalize().unwrap()],
        export_dir: Some(export.path().to_path_buf()),
        allow_existing_writes: true,
        ..ToolContext::for_mcp(Some(1000), 65536, true, false, vec![], false, 1)
    };
    let out = run(&ctx, &saved_into(&source, "SELECT * FROM data", &target)).unwrap();
    assert_eq!(out["rows_written"], 2);
}

/// The scratch file is an implementation detail: nothing of it stays beside the
/// target, and a failed query leaves no target behind.
#[test]
fn a_refused_save_leaves_nothing_in_the_export_folder() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let secret = csv_in(dir.path(), "secret.csv");
    let export = tempfile::tempdir().unwrap();
    let ctx = chat_with_writes(&source, export.path());
    let query = format!("SELECT * FROM read_csv('{}')", secret.display());
    let target = export.path().join("never.duckdb");
    assert!(run(&ctx, &saved_into(&source, &query, &target)).is_err());
    assert_eq!(files_in(export.path()), Vec::<String>::new());
}

fn plain_server() -> ToolContext {
    ToolContext::for_mcp(Some(1000), 65536, false, false, vec![], false, 0)
}

/// A partial file looks exactly like a whole one once it is on disk, so the
/// response has to say, on the plain server and in a chat alike.
#[test]
fn a_save_the_row_limit_cut_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let export = tempfile::tempdir().unwrap();
    let chat = chat_with_writes(&source, export.path());
    let plain = plain_server();
    let _cap = octa::formats::ThreadRowCap::new(3);
    for (ctx, name) in [
        (&plain, "plain.sqlite"),
        (&plain, "plain.csv"),
        (&chat, "chat.sqlite"),
        (&chat, "chat.csv"),
    ] {
        let target = export.path().join(name);
        let query = "SELECT i FROM range(5) t(i)";
        let out = run(ctx, &saved_into(&source, query, &target)).unwrap();
        assert_eq!(out["rows_written"], 3, "{name}: {out}");
        assert_eq!(
            out["initial_load_capped"],
            Value::Bool(true),
            "{name}: {out}"
        );
        assert!(out.get("truncated").is_none(), "{name}: {out}");
        let note = out["initial_load_note"].as_str().unwrap_or_default();
        assert!(note.contains("unlimited"), "{name}: {out}");
        assert!(note.contains('3'), "{name}: {out}");
    }
}

#[test]
fn a_save_that_lost_nothing_says_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let export = tempfile::tempdir().unwrap();
    let plain = plain_server();
    let _cap = octa::formats::ThreadRowCap::new(3);
    for (name, query, rows) in [
        ("exact.sqlite", "SELECT i FROM range(3) t(i)", 3),
        ("exact.csv", "SELECT i FROM range(3) t(i)", 3),
        ("whole.duckdb", "SELECT i FROM range(5) t(i)", 5),
    ] {
        let target = export.path().join(name);
        let out = run(&plain, &saved_into(&source, query, &target)).unwrap();
        assert_eq!(out["rows_written"], rows, "{name}: {out}");
        assert!(out.get("initial_load_capped").is_none(), "{name}: {out}");
        assert!(out.get("initial_load_note").is_none(), "{name}: {out}");
    }
}

/// Every row of `data` reaches the target, but `data` was never all of the file.
#[test]
fn a_save_from_a_partly_read_source_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("long.csv");
    std::fs::write(&source, "id\n1\n2\n3\n4\n5\n").unwrap();
    let plain = plain_server();
    let _cap = octa::formats::ThreadRowCap::new(3);
    let target = dir.path().join("out.duckdb");
    let out = run(&plain, &saved_into(&source, "SELECT * FROM data", &target)).unwrap();
    assert_eq!(out["rows_written"], 3, "{out}");
    assert_eq!(out["initial_load_capped"], Value::Bool(true), "{out}");
    assert!(out.get("truncated").is_none(), "{out}");
    assert!(
        out["initial_load_note"]
            .as_str()
            .unwrap_or_default()
            .contains("source"),
        "{out}"
    );
}

/// The plain MCP server is trusted by design (`for_mcp`: no sandbox), so its SQL
/// keeps the file access a client may rely on.
#[test]
fn an_unrestricted_server_is_not_sealed() {
    let dir = tempfile::tempdir().unwrap();
    let source = csv_in(dir.path(), "source.csv");
    let target = dir.path().join("out.csv");
    let ctx = ToolContext::for_mcp(Some(1000), 65536, false, false, vec![], false, 0);
    let copy = format!("COPY (SELECT 1 AS a) TO '{}'", target.display());
    run(&ctx, &params(&source, &copy)).unwrap();
    assert!(target.exists());
}
