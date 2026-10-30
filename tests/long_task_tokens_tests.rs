//! Every worker thread the GUI starts either holds a `long_tasks.start(..)`
//! token, so the finish notification sees it, or is listed below with the
//! reason it does not need one. A new `thread::spawn` fails this test until
//! someone decides which; the notification once missed a dozen long jobs
//! because nobody had to.

use std::path::{Path, PathBuf};

/// Spawns that need no token, per file: (path under `src/`, count, why).
const EXEMPT: &[(&str, usize, &str)] = &[
    // Read from their own state in `long_task_label` (or the status bar's
    // busy hint, which it asks first).
    ("app/ask_filter_job.rs", 1, "status bar busy hint"),
    ("app/ask_sql_job.rs", 1, "status bar busy hint"),
    (
        "app/central_panel/interaction.rs",
        3,
        "row and page loading: busy hint",
    ),
    ("app/chat/agent.rs", 1, "assistant session"),
    (
        "app/db_browser.rs",
        3,
        "a sidebar listing; a database load (busy hint); sign-in",
    ),
    ("app/dialogs/batch_convert.rs", 1, "listed"),
    ("app/dialogs/cloud_transfer.rs", 1, "listed"),
    ("app/dialogs/db_copy.rs", 1, "listed"),
    ("app/dialogs/db_write_back.rs", 1, "status bar busy hint"),
    ("app/dialogs/fuzzy_join.rs", 1, "listed"),
    ("app/dialogs/harmonise.rs", 3, "listed"),
    ("app/dialogs/load_all.rs", 1, "a database load (busy hint)"),
    ("app/dialogs/report.rs", 1, "listed"),
    ("app/dialogs/schema_drift.rs", 1, "listed"),
    ("app/dialogs/union.rs", 1, "union progress (busy hint)"),
    ("app/file_io/mod.rs", 1, "file load (busy hint)"),
    ("app/folder_compare.rs", 2, "listed"),
    ("app/full_scan.rs", 2, "listed"),
    ("app/pushdown.rs", 4, "server analyses (busy hint)"),
    ("app/sql_panel/render.rs", 1, "SQL run"),
    ("app/sql_panel/server.rs", 1, "SQL run on the server"),
    (
        "app/sql_panel/workspace.rs",
        1,
        "union progress (busy hint)",
    ),
    (
        "app/cloud_browser.rs",
        4,
        "folder listing, sign-in, union (busy hint) x2",
    ),
    // Not a task anyone waits on.
    (
        "app/chat_panel/controls.rs",
        3,
        "Ollama probe, start and stop",
    ),
    (
        "app/dialogs/binary_viewer.rs",
        2,
        "the viewer's own preview",
    ),
    (
        "app/dialogs/settings.rs",
        1,
        "Settings: test a chat profile",
    ),
    ("app/git_marks.rs", 1, "sidebar Git marks"),
    ("view_modes/map.rs", 1, "one tile, seconds, not a long task"),
    (
        "app/update_check/mod.rs",
        3,
        "update check and install (busy hint)",
    ),
    (
        "ui/settings/dialog/api_section.rs",
        1,
        "Settings: test a connection",
    ),
    (
        "ui/settings/dialog/cloud_section.rs",
        1,
        "Settings: test a connection",
    ),
    (
        "ui/settings/dialog/db_section.rs",
        2,
        "Settings: test a connection",
    ),
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && !path.to_string_lossy().ends_with("_tests.rs")
        {
            out.push(path);
        }
    }
}

#[test]
fn every_gui_worker_thread_is_seen_by_the_finish_notification() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src.join("app"), &mut files);
    rust_files(&src.join("ui"), &mut files);
    rust_files(&src.join("view_modes"), &mut files);
    let mut problems = Vec::new();
    for path in files {
        // Without whitespace, so a call rustfmt broke over lines still counts.
        let text: String = std::fs::read_to_string(&path)
            .unwrap()
            .split_whitespace()
            .collect();
        let spawns =
            text.matches("thread::spawn(").count() + text.matches("thread::Builder::new()").count();
        let rel = path
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let tokens = text.matches("long_tasks.start(").count();
        let exempt = EXEMPT
            .iter()
            .find(|(p, _, _)| *p == rel)
            .map_or(0, |(_, n, _)| *n);
        if spawns != tokens + exempt {
            problems.push(format!(
                "{rel}: {spawns} worker threads, {tokens} tokens + {exempt} exempt"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "give each worker a `long_tasks.start(..)` token or list it in EXEMPT:\n{}",
        problems.join("\n")
    );
}

#[test]
fn every_exempt_file_exists() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for (path, _, _) in EXEMPT {
        assert!(
            src.join(path).is_file(),
            "{path} is gone: drop it from EXEMPT"
        );
    }
}
