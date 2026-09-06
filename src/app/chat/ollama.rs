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
/// local server is the intent. Unix uses `pkill`, Windows `taskkill`. The
/// patterns are anchored to the full command line so a process that merely
/// mentions "ollama serve" in its arguments is never matched.
pub fn stop_local_server() {
    #[cfg(unix)]
    {
        // SIGTERM the server first so ollama can tear down its model runner
        // cleanly, then SIGKILL anything still standing - both the server and
        // the `llama-server` runner (which holds the model in RAM; leaving it
        // alive leaks memory until OOM). `-x` with `-f` matches the whole
        // command line exactly; the optional path prefix still catches a
        // server started as e.g. /usr/local/bin/ollama serve.
        let _ = Command::new("pkill")
            .args(["-TERM", "-fx", "([^ ]*/)?ollama serve"])
            .status();
        std::thread::sleep(Duration::from_millis(800));
        let _ = Command::new("pkill")
            .args(["-KILL", "-fx", "([^ ]*/)?ollama serve"])
            .status();
        let _ = Command::new("pkill")
            .args(["-KILL", "-f", "^([^ ]*/)?llama-server( |$)"])
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

/// The models installed locally (via `ollama pull`), from `GET /api/tags`,
/// **most recently updated first**. Returns an empty list when none are
/// installed.
///
/// Every other model list in the assistant is newest first, and this one is
/// no exception - but "newest" can only mean what Ollama knows, which is
/// `modified_at`, the time the local copy was pulled or refreshed. That is
/// the model you most likely just installed and want to pick, whereas the
/// order `/api/tags` happens to return is not an order at all. A tag without
/// a usable timestamp sorts last rather than being dropped.
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
    Ok(names_newest_first(&v))
}

/// Pull the model names out of an `/api/tags` body, most recently updated
/// first. Split from the request so the ordering can be tested without a
/// running Ollama.
fn names_newest_first(v: &Value) -> Vec<String> {
    let mut models: Vec<(String, String)> = v["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let name = m["name"].as_str()?.to_string();
                    // RFC 3339, so a plain string comparison orders it, and a
                    // missing timestamp becomes the empty string, which sorts
                    // last under the reversed comparison below.
                    let modified = m["modified_at"].as_str().unwrap_or_default().to_string();
                    Some((name, modified))
                })
                .collect()
        })
        .unwrap_or_default();
    // Newest first, then by name so two models pulled in the same second do
    // not swap places between refreshes.
    models.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    models.into_iter().map(|(name, _)| name).collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_listed_newest_first() {
        let body = serde_json::json!({"models": [
            {"name": "llama3.2", "modified_at": "2026-01-04T10:00:00Z"},
            {"name": "qwen3", "modified_at": "2026-08-30T09:00:00Z"},
            {"name": "mistral", "modified_at": "2026-03-12T22:00:00Z"},
        ]});
        assert_eq!(names_newest_first(&body), ["qwen3", "mistral", "llama3.2"]);
    }

    #[test]
    fn a_tag_without_a_timestamp_sorts_last_but_is_kept() {
        // Better a model at the bottom of the list than one the user pulled
        // and cannot find in the dropdown at all.
        let body = serde_json::json!({"models": [
            {"name": "no-date"},
            {"name": "dated", "modified_at": "2026-08-30T09:00:00Z"},
        ]});
        assert_eq!(names_newest_first(&body), ["dated", "no-date"]);
    }

    #[test]
    fn an_empty_or_odd_body_is_an_empty_list() {
        for body in [serde_json::json!({}), serde_json::json!({"models": []})] {
            assert!(names_newest_first(&body).is_empty());
        }
    }
}
