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
