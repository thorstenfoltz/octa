//! Crash capture. Two complementary mechanisms (both under `config_dir/logs`):
//! 1. A panic hook that logs the panic and writes `last_crash.txt`.
//! 2. A `running.lock` sentinel: present at next launch => the previous run
//!    ended uncleanly (a hard crash the Rust panic hook could not catch).

use std::path::{Path, PathBuf};

use super::logs_dir;

fn lock_path() -> Option<PathBuf> {
    logs_dir().map(|d| d.join("running.lock"))
}

fn crash_path() -> Option<PathBuf> {
    logs_dir().map(|d| d.join("last_crash.txt"))
}

/// Returns true if the previous run ended uncleanly (the lock was still there),
/// then (re)creates the lock for this session. Call once at startup.
pub fn check_and_mark_running() -> bool {
    match logs_dir() {
        Some(d) => mark_running_in(&d),
        None => false,
    }
}

fn mark_running_in(dir: &Path) -> bool {
    let p = dir.join("running.lock");
    let unclean = p.exists();
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(&p, format!("pid {}", std::process::id()));
    unclean
}

/// Remove the sentinel on a clean shutdown.
pub fn clear_running() {
    if let Some(p) = lock_path() {
        let _ = std::fs::remove_file(p);
    }
}

/// True if a `last_crash.txt` is waiting (does not delete it).
pub fn has_last_crash() -> bool {
    crash_path().map(|p| p.exists()).unwrap_or(false)
}

/// Read and delete `last_crash.txt`.
pub fn take_last_crash() -> Option<String> {
    let p = crash_path()?;
    let s = std::fs::read_to_string(&p).ok()?;
    let _ = std::fs::remove_file(&p);
    Some(s)
}

/// Install a panic hook that logs the panic and writes `last_crash.txt`, then
/// chains to the previously installed hook (so eframe keeps its behaviour).
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let msg = panic_message(info.payload());
        let bt = std::backtrace::Backtrace::force_capture();
        tracing::error!(target: "octa::panic", location = %loc, message = %msg, "panic");
        if let Some(p) = crash_path() {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(
                &p,
                format!(
                    "time: {}\nlocation: {}\nmessage: {}\n\nbacktrace:\n{}\n",
                    now(),
                    loc,
                    msg,
                    bt
                ),
            );
        }
        default(info);
    }));
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_unclean_then_marks() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!mark_running_in(dir.path()), "first run is clean");
        assert!(mark_running_in(dir.path()), "lingering lock => unclean");
    }

    #[test]
    fn panic_message_handles_str_and_string() {
        let s: &str = "boom";
        assert_eq!(panic_message(&s), "boom");
        let owned: String = "kaboom".to_string();
        assert_eq!(panic_message(&owned), "kaboom");
    }
}
