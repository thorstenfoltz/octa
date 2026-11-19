//! Crash capture. Two complementary mechanisms (both under `config_dir/logs`):
//! 1. A panic hook that logs the panic and writes `last_crash.txt`.
//! 2. A `running.lock` sentinel: left non-empty at next launch => the previous
//!    run ended uncleanly (a hard crash the Rust panic hook could not catch).
//!
//! The running session holds an OS lock on the sentinel, so a second Octa
//! window sees a live owner instead of a crash, and a crashed one leaves its
//! lock behind for the system to release.

use std::cell::Cell;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::logs_dir;

/// The sentinel this process owns, locked, until [`clear_running`].
static RUNNING: Mutex<Option<File>> = Mutex::new(None);

thread_local! {
    /// True while [`caught`] runs on this thread.
    static CATCHING: Cell<bool> = const { Cell::new(false) };
}

/// Run third-party code that may panic on a damaged file, and turn the panic
/// into `Err(message)`. The panic is still logged, but it writes no
/// `last_crash.txt`: Octa carries on, so the next start must not report a
/// crash that never happened.
pub fn caught<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    let outer = CATCHING.with(|c| c.replace(true));
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    CATCHING.with(|c| c.set(outer));
    run.map_err(|payload| panic_message(payload.as_ref()))
}

/// Whether a panic on this thread now would be caught by [`caught`].
fn panic_is_caught() -> bool {
    CATCHING.with(Cell::get)
}

fn crash_path() -> Option<PathBuf> {
    logs_dir().map(|d| d.join("last_crash.txt"))
}

/// Returns true if the previous run ended uncleanly, then marks this session
/// running. Call once, from the GUI start only: tests that build an
/// `OctaApp` must not touch the real logs folder.
pub fn check_and_mark_running() -> bool {
    let Some(dir) = logs_dir() else {
        return false;
    };
    let (unclean, held) = mark_running_in(&dir);
    *RUNNING.lock().unwrap_or_else(|e| e.into_inner()) = held;
    unclean
}

/// Takes the sentinel in `dir` when no other live Octa holds it. A sentinel
/// that is free but not empty was left by a session that never reached
/// [`clear_running`]. Another window holding it is not a crash.
///
/// ponytail: only the first of several concurrent windows is crash-watched;
/// per-session sentinel files if that ever matters.
fn mark_running_in(dir: &Path) -> (bool, Option<File>) {
    let _ = std::fs::create_dir_all(dir);
    let Ok(mut file) = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("running.lock"))
    else {
        return (false, None);
    };
    if file.try_lock().is_err() {
        return (false, None);
    }
    let unclean = file.metadata().is_ok_and(|m| m.len() > 0);
    let marked = file.set_len(0).is_ok() && write!(file, "pid {}", std::process::id()).is_ok();
    (unclean, marked.then_some(file))
}

/// Empty the sentinel on a clean shutdown, still holding its lock, then let
/// the lock go. The file stays, so no other window races on its path.
pub fn clear_running() {
    if let Some(file) = RUNNING.lock().unwrap_or_else(|e| e.into_inner()).take() {
        clear_held(&file);
    }
}

fn clear_held(file: &File) {
    let _ = file.set_len(0);
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
        let caught = panic_is_caught();
        tracing::error!(target: "octa::panic", location = %loc, message = %msg, caught, "panic");
        if !caught && let Some(p) = crash_path() {
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
#[path = "crash_tests.rs"]
mod tests;
