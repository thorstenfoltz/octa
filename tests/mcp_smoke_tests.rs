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
        "copy_object",
        "move_object",
        "delete_object",
    ] {
        assert!(
            !names.contains(&write_tool),
            "`{write_tool}` must not be advertised under --mcp-read-only"
        );
    }
    for read_tool in ["read_table", "schema", "run_sql", "pivot", "grep_files"] {
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
