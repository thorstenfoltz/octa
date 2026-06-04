//! Local Ollama control surface: detect whether the server is up, list the
//! models the user has actually pulled, and start the server in the
//! background. All three do blocking network / process work, so callers run
//! them on a worker thread (see `chat_panel.rs`), never on the UI thread.

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::Value;

/// A short-timeout agent so a missing server fails fast instead of hanging the
/// worker (and, indirectly, the user waiting on the refresh).
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_millis(800)))
        .timeout_global(Some(Duration::from_secs(4)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// The base URL with any trailing slash removed.
fn root(base: &str) -> &str {
    base.trim_end_matches('/')
}

/// Whether `base` points at a local Ollama (so stopping it by killing the local
/// process makes sense; a remote server can't be stopped from here).
pub fn is_local_url(base: &str) -> bool {
    let b = base.trim();
    b.contains("localhost")
        || b.contains("127.0.0.1")
        || b.contains("0.0.0.0")
        || b.contains("[::1]")
}

/// Best-effort stop of a locally-running `ollama serve` even when Octa does not
/// hold its [`Child`] handle (e.g. the server was already running, or Octa was
/// restarted). Explicit user action via the Stop button, so terminating the
/// local server is the intent. Unix uses `pkill`, Windows `taskkill`.
pub fn stop_local_server() {
    #[cfg(unix)]
    {
        // SIGTERM the server first so ollama can tear down its model runner
        // cleanly, then SIGKILL anything still standing - both the server and
        // the `llama-server` runner (which holds the model in RAM; leaving it
        // alive leaks memory until OOM).
        let _ = Command::new("pkill")
            .args(["-TERM", "-f", "ollama serve"])
            .status();
        std::thread::sleep(Duration::from_millis(800));
        let _ = Command::new("pkill")
            .args(["-KILL", "-f", "ollama serve"])
            .status();
        let _ = Command::new("pkill")
            .args(["-KILL", "-f", "llama-server"])
            .status();
    }
    #[cfg(windows)]
    {
        // /T kills the process tree, so the runner child goes too.
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/IM", "ollama.exe"])
            .status();
    }
}

/// Is an Ollama server answering at `base`? The root path returns a plain
/// "Ollama is running" 200.
pub fn is_running(base: &str) -> bool {
    agent()
        .get(root(base))
        .call()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// The models installed locally (via `ollama pull`), newest-API order, from
/// `GET /api/tags`. Returns an empty list when none are installed.
pub fn list_models(base: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/api/tags", root(base));
    let mut resp = agent()
        .get(&url)
        .call()
        .map_err(|e| format!("could not reach Ollama at {url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Ollama returned HTTP {}", resp.status().as_u16()));
    }
    let v: Value = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("invalid /api/tags response: {e}"))?;
    let models = v["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok(models)
}

/// Start `ollama serve` in the background, returning the spawned [`Child`] so
/// the caller can stop the server it started on exit. Returns an error if the
/// `ollama` binary is not installed or not on `PATH`. Does not wait for the
/// server to become ready - poll [`is_running`] after a moment.
///
/// The child inherits Octa's environment (the `Command` default). Note that a
/// `500 ... llama-server binary not found` on the first chat request is an
/// Ollama install problem (its model-runner binary is missing), not something
/// Octa controls - that error is surfaced verbatim to the user in the panel.
pub fn start_server() -> Result<Child, String> {
    let mut cmd = Command::new("ollama");
    cmd.arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Put the server in its own session / process group so that stopping it
    // later can kill the whole group - the server plus its `llama-server`
    // model-runner child - instead of orphaning the runner (which keeps the
    // model in RAM and grows until OOM).
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: `setsid` is async-signal-safe and only runs in the child
        // between fork and exec.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    cmd.spawn().map_err(|e| {
        format!(
            "could not start Ollama: {e}. Is it installed and on your PATH? See https://ollama.com"
        )
    })
}

/// Stop a server Octa started by killing its whole process group, so the
/// `llama-server` model-runner child dies with it (otherwise it is orphaned and
/// leaks memory). Unix: SIGTERM the group, give ollama a moment to tear the
/// runner down, then SIGKILL. Other platforms kill just the child.
pub fn stop_child_group(child: &mut Child) {
    #[cfg(unix)]
    {
        // Negative pid targets the whole process group (we `setsid`'d at spawn,
        // so pgid == pid).
        let pgid = child.id() as i32;
        // SAFETY: plain libc kill calls.
        unsafe {
            libc::kill(-pgid, libc::SIGTERM);
        }
        std::thread::sleep(Duration::from_millis(800));
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
        let _ = child.wait();
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}
