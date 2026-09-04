//! End-to-end tests for `octa --mcp`. These spawn the real binary and
//! speak newline-delimited JSON-RPC 2.0 to it over stdio, exactly as an
//! MCP client does.
//!
//! Everything else that touches `src/mcp/` tests it in-process: the
//! sandbox helpers, the JSON cell coercion, and `tool_router` route
//! removal. None of that exercises the wire. This file is what catches a
//! broken handshake, a tool schema `schemars` can emit but a client will
//! reject, and an rmcp upgrade changing the transport out from under us.
//!
//! Kept to a smoke test on purpose: per-tool behaviour belongs with the
//! `src/data/` engines the tools wrap. What is asserted here is that the
//! server starts, advertises a well-formed tool surface, and can execute
//! a call end to end.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

/// Generous: CI runners are slow and the first call reads a file from
/// disk. Any real hang still fails instead of blocking the suite.
const TIMEOUT: Duration = Duration::from_secs(60);

/// A running `octa --mcp` child plus the plumbing to talk to it. stdout
/// and stderr are each drained by a thread so the child can never block
/// on a full pipe buffer.
struct Server {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    stderr: Arc<Mutex<String>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    /// `OCTA_CONFIG_DIR` points into the temp dir so the server starts
    /// from stock defaults and never reads the developer's real
    /// `settings.toml` (which would change the row caps and inject saved
    /// database connections).
    fn start(config_dir: &std::path::Path, extra_args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_octa"))
            .arg("--mcp")
            .args(extra_args)
            .env("OCTA_CONFIG_DIR", config_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn `octa --mcp`");

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let child_stderr = child.stderr.take().unwrap();

        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });

        // Drained into a buffer rather than discarded: when the server
        // dies during a handshake, its stderr is the only explanation.
        let stderr = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&stderr);
        std::thread::spawn(move || {
            for line in BufReader::new(child_stderr).lines().map_while(Result::ok) {
                let mut buf = sink.lock().unwrap();
                buf.push_str(&line);
                buf.push('\n');
            }
        });

        Self {
            child,
            stdin,
            lines,
            stderr,
        }
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").expect("write to MCP stdin");
        self.stdin.flush().unwrap();
    }

    /// Send a request and return the whole JSON-RPC response, skipping
    /// any notification or unrelated response that arrives first. Panics
    /// (with the server's stderr attached) on timeout or a dead server.
    fn request(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        loop {
            let line = match self.lines.recv_timeout(TIMEOUT) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => {
                    panic!("`{method}` timed out\nserver stderr:\n{}", self.stderr())
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!(
                        "server exited during `{method}`\nstderr:\n{}",
                        self.stderr()
                    )
                }
            };
            // stdout is the JSON-RPC channel; anything else on it is a bug.
            let msg: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("non-JSON line on stdout: {e}\nline: {line}"));
            if msg.get("id").and_then(Value::as_i64) == Some(id) {
                return msg;
            }
        }
    }

    /// Like [`Self::request`], but asserts the call succeeded and hands
    /// back just the `result`.
    fn ok_request(&mut self, id: i64, method: &str, params: Value) -> Value {
        let msg = self.request(id, method, params);
        assert!(
            msg.get("error").is_none(),
            "`{method}` returned a JSON-RPC error: {}",
            msg["error"]
        );
        msg["result"].clone()
    }

    fn notify(&mut self, method: &str) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method }));
    }

    /// Full client handshake: initialize, then the initialized
    /// notification. Returns the initialize result.
    fn handshake(&mut self) -> Value {
        let result = self.ok_request(
            1,
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "octa-smoke-test", "version": "1" },
            }),
        );
        self.notify("notifications/initialized");
        result
    }

    fn stderr(&self) -> String {
        self.stderr.lock().unwrap().clone()
    }
}

/// Unwrap a successful `tools/call` result: the payload is JSON encoded
/// as the text of the first content block.
fn call_payload(result: &Value) -> Value {
    assert_ne!(
        result["isError"],
        json!(true),
        "tool call reported an error: {result}"
    );
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text content in tool result: {result}"));
    serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("tool result text is not JSON: {e}\ntext: {text}"))
}

/// A call that must not succeed. MCP allows either shape: a tool that
/// runs and fails reports `isError`, while a tool the router does not
/// know is a JSON-RPC error. Both are acceptable refusals.
fn assert_call_failed(msg: &Value, what: &str) {
    let rpc_error = msg.get("error").is_some();
    let tool_error = msg["result"]["isError"] == json!(true);
    assert!(
        rpc_error || tool_error,
        "{what} should have been refused, got: {msg}"
    );
}

fn fixture_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.csv"),
        "id,city,amount\n1,Tokyo,10\n2,Helsinki,20\n3,Tokyo,30\n",
    )
    .unwrap();
    // A second table that joins `a.csv` on id under a different column name,
    // for the join-key finder.
    std::fs::write(
        dir.path().join("cities.csv"),
        "city_id,country\n1,JP\n2,FI\n3,JP\n9,DE\n",
    )
    .unwrap();
    // Two months and three rows, for the time-series tools.
    std::fs::write(
        dir.path().join("ts.csv"),
        "day,amount\n2024-01-05,10\n2024-01-20,5\n2024-02-02,7\n",
    )
    .unwrap();
    // A key that differs from `a.csv`'s only by a trailing space, so a join
    // matches nothing and the cause is exactly one normalisation away.
    std::fs::write(
        dir.path().join("padded.csv"),
        "id,label\n1 ,one\n2 ,two\n3 ,three\n",
    )
    .unwrap();
    dir
}

#[test]
fn handshake_advertises_a_wellformed_tool_surface() {
    let dir = fixture_dir();
    let mut server = Server::start(&dir.path().join("config"), &[]);

    let init = server.handshake();
    assert_eq!(init["protocolVersion"], json!("2024-11-05"));
    assert!(
        init["capabilities"]["tools"].is_object(),
        "server must advertise tool support: {init}"
    );
    // Identify as octa, not as the transport library (rmcp's
    // `Implementation::from_build_env` expands inside rmcp).
    assert_eq!(init["serverInfo"]["name"], json!("octa"));
    assert!(
        init["instructions"]
            .as_str()
            .unwrap_or_default()
            .contains("Octa MCP server"),
        "instructions missing: {init}"
    );

    let listed = server.ok_request(2, "tools/list", json!({}));
    let tools = listed["tools"].as_array().expect("tools array");
    assert!(
        tools.len() >= 30,
        "expected the full tool surface, got {}",
        tools.len()
    );

    // Every advertised tool must be usable by a client: a non-empty
    // description to pick it by, and an object input schema to fill in.
    // A `schemars` change that emits something else fails here rather
    // than at an agent's first call.
    for tool in tools {
        let name = tool["name"].as_str().expect("tool name");
        assert!(
            !tool["description"].as_str().unwrap_or_default().is_empty(),
            "`{name}` has no description"
        );
        assert_eq!(
            tool["inputSchema"]["type"],
            json!("object"),
            "`{name}` input schema is not an object: {}",
            tool["inputSchema"]
        );
        // `properties` is absent for the parameterless tools (e.g.
        // `list_db_connections`), which is valid; when present it has to
        // be an object.
        let props = &tool["inputSchema"]["properties"];
        assert!(
            props.is_null() || props.is_object(),
            "`{name}` input schema has a non-object `properties`: {}",
            tool["inputSchema"]
        );
    }

    // Every advertised tool has to be documented somewhere under `docs/`,
    // and every page under `docs/mcp/tools/` has to be reachable from the
    // mkdocs nav. Both halves shipped broken on one branch: two tool pages
    // were written and never added to the nav, so they existed in the repo
    // and not on the site, and two tools shipped with no page at all.
    let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs");
    let mut prose = String::new();
    for entry in walk_docs(&docs) {
        prose.push_str(&std::fs::read_to_string(&entry).unwrap_or_default());
    }
    let nav = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("mkdocs.yml"),
    )
    .expect("mkdocs.yml");
    for tool in tools {
        let name = tool["name"].as_str().expect("tool name");
        assert!(
            prose.contains(name),
            "`{name}` is advertised but never mentioned anywhere under docs/"
        );
        // Being named in the tools index is not documentation. Five live-database
        // tools were listed there and had no page of their own for a whole
        // release, which the "mentioned anywhere" check above could not see.
        assert!(
            docs.join(format!("mcp/tools/{name}.md")).is_file(),
            "`{name}` has no page at docs/mcp/tools/{name}.md"
        );
    }
    for entry in std::fs::read_dir(docs.join("mcp/tools")).expect("docs/mcp/tools") {
        let path = entry.expect("dir entry").path();
        let Some(file) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !file.ends_with(".md") {
            continue;
        }
        assert!(
            nav.contains(&format!("mcp/tools/{file}")),
            "docs/mcp/tools/{file} is not in the mkdocs nav, so it never reaches the site"
        );
    }

    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for expected in [
        "read_table",
        "schema",
        "run_sql",
        "list_tables",
        "count_rows",
        "describe_file",
        "write_table",
        "convert",
        "sync_sql",
        "write_workbook",
        "db_relationships",
    ] {
        assert!(
            names.contains(&expected),
            "`{expected}` missing from {names:?}"
        );
    }
}

#[test]
fn tool_calls_read_the_file_and_run_sql() {
    let dir = fixture_dir();
    let csv = dir.path().join("a.csv").to_string_lossy().into_owned();
    let mut server = Server::start(&dir.path().join("config"), &[]);
    server.handshake();

    let schema = call_payload(&server.ok_request(
        2,
        "tools/call",
        json!({ "name": "schema", "arguments": { "path": csv } }),
    ));
    let cols: Vec<&str> = schema["columns"]
        .as_array()
        .expect("columns array")
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert_eq!(cols, ["id", "city", "amount"]);

    let sql = call_payload(&server.ok_request(
        3,
        "tools/call",
        json!({
            "name": "run_sql",
            "arguments": {
                "path": csv,
                "query": "SELECT city, sum(amount) AS total FROM data GROUP BY city ORDER BY city",
            },
        }),
    ));
    assert_eq!(sql["kind"], json!("select"), "unexpected shape: {sql}");
    let rows = sql["result"]["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("no rows array in: {sql}"));
    assert_eq!(rows.len(), 2, "unexpected rows: {sql}");
    // ORDER BY city, so Helsinki precedes Tokyo. The SQL workspace hands
    // every result column back as text, so compare on the string form
    // rather than a numeric type.
    assert_eq!(rows[0][0].as_str(), Some("Helsinki"));
    assert_eq!(rows[0][1].as_str(), Some("20"));
    // Tokyo appears twice in the fixture: 10 + 30.
    assert_eq!(rows[1][0].as_str(), Some("Tokyo"));
    assert_eq!(rows[1][1].as_str(), Some("40"));

    // Distribution comparison over the wire. The fixture is tiny, so the
    // engine's own floor answers rather than a verdict - which is the
    // response shape a client has to handle, and the one worth pinning.
    let dist = call_payload(&server.ok_request(
        4,
        "tools/call",
        json!({
            "name": "compare_distributions",
            "arguments": { "path": csv, "column": "amount", "column_b": "amount" },
        }),
    ));
    assert_eq!(
        dist["skipped"],
        json!("na_too_few_values"),
        "unexpected shape: {dist}"
    );

    // Referential integrity over the wire, as a self-reference: every id
    // matches itself, so `clean` is true and the orphan list is empty.
    let refs = call_payload(&server.ok_request(
        5,
        "tools/call",
        json!({
            "name": "check_references",
            "arguments": { "path": csv, "parent_column": "id", "child_column": "id" },
        }),
    ));
    assert_eq!(refs["clean"], json!(true), "unexpected shape: {refs}");
    assert_eq!(refs["orphan_rows"], json!(0));

    // The two time-series tools: same builders as the GUI dialog and the
    // CLI flags, reached over the wire.
    let ts = dir.path().join("ts.csv").to_string_lossy().into_owned();
    let payload = call_payload(&server.ok_request(
        50,
        "tools/call",
        json!({
            "name": "resample_timeseries",
            "arguments": {
                "path": ts,
                "time_col": "day",
                "value_cols": ["amount"],
                "interval": "month",
                "agg": "sum"
            }
        }),
    ));
    assert_eq!(
        payload["row_count"],
        json!(2),
        "two months in the fixture: {payload}"
    );

    let payload = call_payload(&server.ok_request(
        51,
        "tools/call",
        json!({
            "name": "rolling_window",
            "arguments": {
                "path": ts,
                "order_col": "day",
                "value_col": "amount",
                "window": 2,
                "agg": "mean"
            }
        }),
    ));
    assert!(
        payload["schema"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == json!("amount_rolling_2")),
        "rolling column missing: {payload}"
    );

    // batch_convert is a write tool: it produces files on disk.
    let out_dir = dir.path().join("batch_out");
    let payload = call_payload(&server.ok_request(
        60,
        "tools/call",
        json!({
            "name": "batch_convert",
            "arguments": {
                "inputs": [dir.path().join("a.csv").to_string_lossy()],
                "out_dir": out_dir.to_string_lossy(),
                "to": "json"
            }
        }),
    ));
    assert_eq!(payload["converted"], json!(1), "{payload}");
    assert!(out_dir.join("a.json").exists(), "output file missing");

    // A failing call must come back as a tool error, not kill the server
    // or return a malformed frame. Absolute path: the server inherits the
    // test binary's working directory, not the fixture dir.
    let absent = dir.path().join("no-such-file.csv");
    let missing = server.request(
        4,
        "tools/call",
        json!({ "name": "schema", "arguments": { "path": absent } }),
    );
    assert_call_failed(&missing, "reading a missing file");

    // ...and the server is still usable afterwards.
    let count = call_payload(&server.ok_request(
        5,
        "tools/call",
        json!({ "name": "count_rows", "arguments": { "path": csv } }),
    ));
    assert_eq!(count["row_count"], json!(3));
}

#[test]
fn read_only_server_hides_the_write_tools() {
    let dir = fixture_dir();
    let mut server = Server::start(&dir.path().join("config"), &["--mcp-read-only"]);
    server.handshake();

    let listed = server.ok_request(2, "tools/list", json!({}));
    let names: Vec<&str> = listed["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();

    for write_tool in [
        "write_table",
        "edit_table",
        "convert",
        "transform_columns",
        "anonymize",
        "partition_table",
        "write_db_table",
        "copy_db_table",
        "write_workbook",
        "batch_convert",
        "copy_object",
        "move_object",
        "delete_object",
        "create_report",
    ] {
        assert!(
            !names.contains(&write_tool),
            "`{write_tool}` must not be advertised under --mcp-read-only"
        );
    }
    for read_tool in [
        "read_table",
        "schema",
        "run_sql",
        "pivot",
        "grep_files",
        "resample_timeseries",
        "rolling_window",
        "data_drift",
        "check_rules",
        // Reads a table and returns SQL text; writes nothing, so a read-only
        // server keeps it.
        "sync_sql",
    ] {
        assert!(
            names.contains(&read_tool),
            "`{read_tool}` should still be available under --mcp-read-only"
        );
    }

    // Dropped from the router means the call itself is refused, not just
    // hidden from the listing.
    let refused = server.request(
        3,
        "tools/call",
        json!({
            "name": "write_table",
            "arguments": {
                "path": dir.path().join("out.csv").to_string_lossy(),
                "columns": [{ "name": "x" }],
                "rows": [["1"]],
            },
        }),
    );
    assert_call_failed(&refused, "calling a tool removed by --mcp-read-only");
    // The file must not exist either: refusal has to happen before any
    // write, not after one.
    assert!(
        !dir.path().join("out.csv").exists(),
        "a refused write_table still created the file"
    );
}

/// `describe_file` with `deep` carries the file's physical layout, so an
/// agent asked "why is this file slow" has something to answer with.
#[test]
fn describe_file_deep_returns_internals() {
    let dir = fixture_dir();
    let csv = dir.path().join("a.csv").to_string_lossy().into_owned();
    let parquet = dir.path().join("a.parquet").to_string_lossy().into_owned();
    let mut server = Server::start(&dir.path().join("config"), &[]);
    server.handshake();

    call_payload(&server.ok_request(
        2,
        "tools/call",
        json!({ "name": "convert", "arguments": { "input": csv, "output": parquet } }),
    ));

    let deep = call_payload(&server.ok_request(
        3,
        "tools/call",
        json!({ "name": "describe_file", "arguments": { "path": parquet, "deep": true } }),
    ));
    let internals = &deep["internals"];
    assert!(
        internals.is_object(),
        "deep describe_file must carry internals: {deep}"
    );
    assert!(
        internals["facts"]["row_groups"].is_string(),
        "facts missing row_groups: {internals}"
    );
    assert!(internals["hints"].is_array(), "hints missing: {internals}");

    // Without `deep` the response is unchanged.
    let plain = call_payload(&server.ok_request(
        4,
        "tools/call",
        json!({ "name": "describe_file", "arguments": { "path": parquet } }),
    ));
    assert!(plain["internals"].is_null(), "{plain}");
}

/// `diff_tables` accepts a live database table as its B side. Without a
/// server configured, the assertion that matters is the failure mode: it
/// must name the unknown connection rather than complain about a missing
/// second file.
#[test]
fn diff_tables_rejects_an_unknown_db_connection() {
    let dir = fixture_dir();
    let csv = dir.path().join("a.csv").to_string_lossy().into_owned();
    let mut server = Server::start(&dir.path().join("config"), &[]);
    server.handshake();

    let resp = server.request(
        2,
        "tools/call",
        json!({
            "name": "diff_tables",
            "arguments": {
                "path_a": csv,
                "b_db": { "connection": "no_such_connection", "table": "public.orders" },
                "mode": "join",
                "on": ["id"]
            }
        }),
    );
    assert_call_failed(&resp, "diff_tables with an unknown connection");
    let text = serde_json::to_string(&resp).unwrap();
    assert!(
        text.contains("no_such_connection"),
        "the error should name the connection: {text}"
    );
}

/// `db_relationships` is reachable over the wire and reports an unknown
/// connection rather than answering with an empty map. The roster assertion
/// above only proves the tool is advertised, not that a call reaches it.
#[test]
fn db_relationships_rejects_an_unknown_db_connection() {
    let dir = fixture_dir();
    let mut server = Server::start(&dir.path().join("config"), &[]);
    server.handshake();

    let resp = server.request(
        2,
        "tools/call",
        json!({
            "name": "db_relationships",
            "arguments": { "connection": "no_such_connection" }
        }),
    );
    assert_call_failed(&resp, "db_relationships with an unknown connection");
    let text = serde_json::to_string(&resp).unwrap();
    assert!(
        text.contains("no_such_connection"),
        "the error should name the connection: {text}"
    );
}

/// `suggest_join_keys` finds the pairing whose names do not match: `a.csv`'s
/// `id` against `cities.csv`'s `city_id`.
#[test]
fn suggest_join_keys_finds_the_planted_key() {
    let dir = fixture_dir();
    let a = dir.path().join("a.csv").to_string_lossy().into_owned();
    let b = dir.path().join("cities.csv").to_string_lossy().into_owned();
    let mut server = Server::start(&dir.path().join("config"), &[]);
    server.handshake();

    let out = call_payload(&server.ok_request(
        2,
        "tools/call",
        json!({ "name": "suggest_join_keys", "arguments": { "paths": [a, b] } }),
    ));
    let first = &out["candidates"][0];
    assert_eq!(first["left_column"], json!("id"), "got {out}");
    assert_eq!(first["right_column"], json!("city_id"), "got {out}");
    assert!(
        first["overlap"].as_f64().unwrap_or(0.0) > 0.9,
        "overlap should be high: {first}"
    );
    // `a.csv` has ids 1..3 and `cities.csv` has 1, 2, 3 and 9, so the left
    // side is fully covered: the orphan count is the tie-breaker and must be
    // present on every candidate.
    assert_eq!(first["left_orphans"], json!(0), "got {first}");
    assert_eq!(first["left_distinct_values"], json!(3), "got {first}");
}

/// One table is not a comparison: the tool says so rather than returning an
/// empty list that looks like "no keys found".
#[test]
fn suggest_join_keys_needs_two_tables() {
    let dir = fixture_dir();
    let a = dir.path().join("a.csv").to_string_lossy().into_owned();
    let mut server = Server::start(&dir.path().join("config"), &[]);
    server.handshake();

    let resp = server.request(
        2,
        "tools/call",
        json!({ "name": "suggest_join_keys", "arguments": { "paths": [a] } }),
    );
    assert_call_failed(&resp, "suggest_join_keys with one table");
}

/// `diagnose_join` explains a join that matches nothing: the keys differ only
/// by a trailing space, so it must report zero matches AND name trimming as the
/// fix. Read-only, so it must survive `--mcp-read-only` too.
#[test]
fn diagnose_join_names_the_whitespace_culprit() {
    let dir = fixture_dir();
    let a = dir.path().join("a.csv").to_string_lossy().into_owned();
    let b = dir.path().join("padded.csv").to_string_lossy().into_owned();
    let mut server = Server::start(&dir.path().join("config"), &["--mcp-read-only"]);
    server.handshake();

    let out = call_payload(&server.ok_request(
        2,
        "tools/call",
        json!({
            "name": "diagnose_join",
            "arguments": {
                "path": a, "left_column": "id",
                "path_b": b, "right_column": "id"
            }
        }),
    ));
    assert_eq!(out["matched_left"], json!(0), "got {out}");
    assert_eq!(out["distinct_left"], json!(3), "got {out}");
    let fixes = out["fixes"].as_array().expect("fixes array");
    assert!(
        fixes
            .iter()
            .any(|f| f["kind"] == json!("trim_whitespace") && f["would_match"] == json!(3)),
        "trimming should recover all three keys: {out}"
    );
}

/// A wrong column name is a mistake worth naming, not an index-0 fallback that
/// produces a confident but meaningless diagnosis.
#[test]
fn diagnose_join_rejects_an_unknown_column() {
    let dir = fixture_dir();
    let a = dir.path().join("a.csv").to_string_lossy().into_owned();
    let b = dir.path().join("padded.csv").to_string_lossy().into_owned();
    let mut server = Server::start(&dir.path().join("config"), &[]);
    server.handshake();

    let resp = server.request(
        2,
        "tools/call",
        json!({
            "name": "diagnose_join",
            "arguments": {
                "path": a, "left_column": "nope",
                "path_b": b, "right_column": "id"
            }
        }),
    );
    assert_call_failed(&resp, "diagnose_join with an unknown column");
}

/// `harmonise_schemas` writes copies and leaves the originals alone. It is a
/// write tool, so it must also disappear under `--mcp-read-only`.
#[test]
fn harmonise_schemas_writes_copies_and_keeps_originals() {
    let dir = fixture_dir();
    let scan = dir.path().join("harm_in");
    let out = dir.path().join("harm_out");
    std::fs::create_dir(&scan).unwrap();
    std::fs::write(scan.join("a.csv"), "id,name\n1,alice\n").unwrap();
    std::fs::write(scan.join("b.csv"), "id,name\n2,bob\n").unwrap();
    std::fs::write(scan.join("c.csv"), "id\n3\n").unwrap();
    let before = std::fs::read_to_string(scan.join("c.csv")).unwrap();

    let mut server = Server::start(&dir.path().join("config"), &[]);
    server.handshake();
    let payload = call_payload(&server.ok_request(
        2,
        "tools/call",
        json!({
            "name": "harmonise_schemas",
            "arguments": {
                "dir": scan.to_string_lossy(),
                "out_dir": out.to_string_lossy()
            }
        }),
    ));
    assert_eq!(payload["written"], json!(3), "got {payload}");
    assert_eq!(payload["refused"], json!(0), "got {payload}");
    assert!(
        std::fs::read_to_string(out.join("c.csv"))
            .unwrap()
            .starts_with("id,name"),
        "the missing column should have been added"
    );
    assert_eq!(
        std::fs::read_to_string(scan.join("c.csv")).unwrap(),
        before,
        "the original must be byte-identical"
    );
}

/// Writing into the folder being scanned would destroy the originals, which is
/// the one thing this tool promises never to do.
#[test]
fn harmonise_schemas_refuses_to_write_over_its_input() {
    let dir = fixture_dir();
    let scan = dir.path().join("harm_same");
    std::fs::create_dir(&scan).unwrap();
    std::fs::write(scan.join("a.csv"), "id\n1\n").unwrap();

    let mut server = Server::start(&dir.path().join("config"), &[]);
    server.handshake();
    let resp = server.request(
        2,
        "tools/call",
        json!({
            "name": "harmonise_schemas",
            "arguments": {
                "dir": scan.to_string_lossy(),
                "out_dir": scan.to_string_lossy()
            }
        }),
    );
    assert_call_failed(&resp, "harmonise_schemas writing over its own input");
}

/// Two files that disagree come back as two variants with `has_drift` set.
/// The tool is read-only, so it must survive `--mcp-read-only` too.
#[test]
fn schema_drift_tool_reports_drift() {
    let dir = fixture_dir();
    let scan = dir.path().join("parts");
    std::fs::create_dir(&scan).unwrap();
    std::fs::write(scan.join("a.csv"), "id,amount\n1,2\n").unwrap();
    std::fs::write(scan.join("b.csv"), "id,other\n2,x\n").unwrap();
    let scan_arg = scan.to_string_lossy().into_owned();

    let mut server = Server::start(&dir.path().join("config"), &["--mcp-read-only"]);
    server.handshake();

    let out = call_payload(&server.ok_request(
        2,
        "tools/call",
        json!({ "name": "schema_drift", "arguments": { "path": scan_arg } }),
    ));
    assert_eq!(out["has_drift"], json!(true), "unexpected shape: {out}");
    assert_eq!(
        out["variants"].as_array().map(|v| v.len()),
        Some(2),
        "two disagreeing files are two variants: {out}"
    );
    assert_eq!(
        out["drifting_columns"].as_array().map(|v| v.len()),
        Some(2),
        "amount and other each exist in only one variant: {out}"
    );
}

/// The write tool's happy path over the wire: it must actually produce a file,
/// not merely be advertised.
#[test]
fn create_report_writes_a_self_contained_file() {
    let dir = fixture_dir();
    let csv = dir.path().join("a.csv").to_string_lossy().into_owned();
    let out = dir.path().join("report.html");
    let out_arg = out.to_string_lossy().into_owned();

    let mut server = Server::start(&dir.path().join("config"), &[]);
    server.handshake();

    let payload = call_payload(&server.ok_request(
        2,
        "tools/call",
        json!({
            "name": "create_report",
            "arguments": { "path": csv, "out_path": out_arg, "sections": ["stats"] },
        }),
    ));
    assert!(
        payload["bytes"].as_u64().unwrap_or(0) > 0,
        "unexpected shape: {payload}"
    );

    let html = std::fs::read_to_string(&out).expect("report was not written");
    assert!(html.starts_with("<!DOCTYPE html>"));
    for attr in ["src=\"http", "href=\"http", "url(http", "@import"] {
        assert!(!html.contains(attr), "found a remote reference ({attr})");
    }
}

/// The read-only fuzzy join over the wire: it must match a spelling variant
/// and hand back the per-step report alongside the rows.
#[test]
fn fuzzy_join_matches_spelling_variants() {
    let dir = fixture_dir();
    std::fs::write(dir.path().join("crm.csv"), "customer\nMueller GmbH\n").unwrap();
    std::fs::write(dir.path().join("sales.csv"), "account\nMueller Gmbh.\n").unwrap();
    let crm = dir.path().join("crm.csv").to_string_lossy().into_owned();
    let sales = dir.path().join("sales.csv").to_string_lossy().into_owned();

    let mut server = Server::start(&dir.path().join("config"), &["--mcp-read-only"]);
    server.handshake();

    let out = call_payload(&server.ok_request(
        2,
        "tools/call",
        json!({
            "name": "fuzzy_join",
            "arguments": {
                "sources": [{ "path": crm }, { "path": sales }],
                "on": ["customer=account"],
            },
        }),
    ));

    let cols: Vec<&str> = out["schema"]
        .as_array()
        .expect("schema array")
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert!(
        cols.contains(&"match_score_1"),
        "expected the score column: {out}"
    );
    assert_eq!(
        out["steps"][0]["matched"],
        json!(1),
        "the spelling variant should match: {out}"
    );
}

/// The drift tool over the wire: `a.csv` has a missing amount and `b.csv`
/// does not, so the null rate moves and a five percent gate must fail.
#[test]
fn data_drift_reports_a_moved_null_rate() {
    let dir = fixture_dir();
    // A later extract of the same table that lost one amount.
    std::fs::write(
        dir.path().join("later.csv"),
        "id,city,amount\n1,Tokyo,10\n2,Helsinki,20\n3,Tokyo,\n",
    )
    .unwrap();
    let a = dir.path().join("a.csv").to_string_lossy().into_owned();
    let b = dir.path().join("later.csv").to_string_lossy().into_owned();

    let mut server = Server::start(&dir.path().join("config"), &[]);
    server.handshake();

    let payload = call_payload(&server.ok_request(
        2,
        "tools/call",
        json!({
            "name": "data_drift",
            "arguments": { "path_a": a, "path_b": b, "fail_on": "null_rate:0.05" },
        }),
    ));
    assert_eq!(
        payload["failed"], true,
        "a moved null rate must breach the gate: {payload}"
    );
    let rows = payload["drift"].as_array().expect("drift array");
    assert!(
        rows.iter()
            .any(|r| r["metric"] == "null_rate" && r["breached"] == true),
        "no breached null_rate row: {payload}"
    );
}

/// The rules gate over the wire: a duplicated key must fail, and the failing
/// rule must be named with a sample of what broke it.
#[test]
fn check_rules_reports_a_failing_rule() {
    let dir = fixture_dir();
    std::fs::write(dir.path().join("dup.csv"), "order_id\n1\n1\n").unwrap();
    std::fs::write(
        dir.path().join("q.toml"),
        "[[rule]]\ncolumn = \"order_id\"\nkind = \"unique\"\n",
    )
    .unwrap();
    let data = dir.path().join("dup.csv").to_string_lossy().into_owned();
    let rules = dir.path().join("q.toml").to_string_lossy().into_owned();

    let mut server = Server::start(&dir.path().join("config"), &[]);
    server.handshake();

    let payload = call_payload(&server.ok_request(
        2,
        "tools/call",
        json!({ "name": "check_rules", "arguments": { "path": data, "rules_path": rules } }),
    ));
    assert_eq!(payload["passed"], false, "unexpected shape: {payload}");
    let v = payload["violations"].as_array().expect("violations array");
    assert!(
        v.iter()
            .any(|r| r["rule"] == "unique" && r["column"] == "order_id"),
        "no unique failure named: {payload}"
    );
}

/// Large-file streaming over the wire.
///
/// The threshold comes from settings, so the test writes a tiny one into the
/// server's config dir. Without streaming, `count_rows` on a file past the
/// initial-load cap answers short and `unlimited: true` loads the whole thing;
/// with it the count is exact and free, and `run_sql` aggregates every row.
#[test]
fn a_large_file_is_scanned_in_place() {
    let dir = fixture_dir();
    let config = dir.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    // 1 KB threshold, and a 10-row load cap so a non-streamed read would be
    // visibly short.
    std::fs::write(
        config.join("settings.toml"),
        "large_file_min_bytes = 1024\ninitial_load_rows = 10\n",
    )
    .unwrap();

    let mut csv = String::from("id,name\n");
    for i in 0..5_000 {
        csv.push_str(&format!("{i},row{i}\n"));
    }
    std::fs::write(dir.path().join("big.csv"), csv).unwrap();
    let big = dir.path().join("big.csv").to_string_lossy().into_owned();

    let mut server = Server::start(&config, &[]);
    server.handshake();

    let counted = call_payload(&server.ok_request(
        2,
        "tools/call",
        json!({ "name": "count_rows", "arguments": { "path": big } }),
    ));
    assert_eq!(counted["streamed"], true, "unexpected shape: {counted}");
    assert_eq!(
        counted["row_count"], 5000,
        "a scanned count must be exact, not bounded by the load cap: {counted}"
    );

    let queried = call_payload(&server.ok_request(
        3,
        "tools/call",
        json!({
            "name": "run_sql",
            "arguments": { "path": big, "query": "SELECT count(*) AS n FROM data" },
        }),
    ));
    assert_eq!(queried["streamed"], true, "unexpected shape: {queried}");
    let n = queried["result"]["rows"][0][0].clone();
    assert!(
        n == json!(5000) || n == json!("5000"),
        "the aggregate must cover every row, got {n} in {queried}"
    );

    // A small file in the same session must NOT be streamed, or the opt-in is
    // not an opt-in.
    let small = dir.path().join("a.csv").to_string_lossy().into_owned();
    let small_count = call_payload(&server.ok_request(
        4,
        "tools/call",
        json!({ "name": "count_rows", "arguments": { "path": small } }),
    ));
    assert!(
        small_count.get("streamed").is_none(),
        "a small file should read normally: {small_count}"
    );
}

/// Every `.md` under `docs/`, so a tool can be documented on its own page or
/// inside a guide (the live-database tools live in the Database Connections
/// guide, and that is the right place for them).
fn walk_docs(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // The design specs live under docs/ but are working notes, not
            // published pages; a tool named only there is undocumented.
            if path.file_name().is_some_and(|n| n == "superpowers") {
                continue;
            }
            out.extend(walk_docs(&path));
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    out
}

/// `--mcp-tools` on the wire: the advertised list is exactly what was asked
/// for, and a call to something left out is refused rather than silently
/// working.
#[test]
fn a_tool_filter_shapes_the_advertised_list() {
    let dir = fixture_dir();
    let mut server = Server::start(
        &dir.path().join("config"),
        &["--mcp-tools", "core,databases"],
    );
    server.handshake();

    let listed = server.ok_request(2, "tools/list", json!({}));
    let names: Vec<&str> = listed["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();

    for kept in [
        "read_table",
        "run_sql",
        "schema",
        "query_db",
        "list_db_tables",
    ] {
        assert!(
            names.contains(&kept),
            "`{kept}` was asked for but is not advertised: {names:?}"
        );
    }
    for dropped in ["fuzzy_join", "detect_pii", "list_objects", "write_table"] {
        assert!(
            !names.contains(&dropped),
            "`{dropped}` was not asked for but is advertised"
        );
    }

    let refused = server.request(
        3,
        "tools/call",
        json!({ "name": "detect_pii", "arguments": { "path": "x.csv" } }),
    );
    assert_call_failed(&refused, "calling a tool left out by --mcp-tools");
}

/// The server's own `instructions` name the tools, and a client reads them as
/// the truth. They have to follow the filter, or the model is told about tools
/// it cannot call.
#[test]
fn the_instructions_name_only_the_advertised_tools() {
    let dir = fixture_dir();
    let mut server = Server::start(&dir.path().join("config"), &["--mcp-tools", "core"]);
    let init = server.handshake();
    let instructions = init["instructions"].as_str().expect("instructions");

    assert!(instructions.contains("read_table"), "{instructions}");
    for dropped in ["fuzzy_join", "detect_pii", "write_table"] {
        assert!(
            !instructions.contains(dropped),
            "`{dropped}` is not advertised but the instructions mention it"
        );
    }
}

/// `--mcp-without` is the other direction, and the two combine.
#[test]
fn without_removes_from_what_is_left() {
    let dir = fixture_dir();
    let mut server = Server::start(
        &dir.path().join("config"),
        &["--mcp-tools", "core", "--mcp-without", "run_sql"],
    );
    server.handshake();

    let listed = server.ok_request(2, "tools/list", json!({}));
    let names: Vec<&str> = listed["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(names.contains(&"read_table"));
    assert!(!names.contains(&"run_sql"), "explicitly removed");
}

/// A typo stops the server instead of starting one with the wrong surface.
#[test]
fn an_unknown_group_is_refused_at_startup() {
    let dir = fixture_dir();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_octa"))
        .args(["--mcp", "--mcp-tools", "kwality"])
        .env("OCTA_CONFIG_DIR", dir.path().join("config"))
        .output()
        .expect("run octa");
    assert!(!out.status.success(), "a bad group must not start a server");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("kwality"), "{err}");
    assert!(err.contains("quality"), "the valid names are listed: {err}");
}
