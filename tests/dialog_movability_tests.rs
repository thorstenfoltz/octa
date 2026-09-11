//! Guard: no dialog may pin itself with `Window::anchor`.
//!
//! `egui::Area::anchor` ends by calling `movable(false)`, and `Window` inherits
//! that, so an anchored window silently ignores every drag. Thirty dialogs did
//! it, and the result was windows the user could not push aside to read what
//! was underneath - the Update, Report AI and Repair windows worst of all,
//! since they had no resize either.
//!
//! `octa::ui::settings::center_on_first_show` is the replacement: it centres a
//! window through `default_pos` the first time it is shown and then lets egui
//! remember wherever the user drags it.

use std::path::{Path, PathBuf};

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_dialog_pins_itself_with_anchor() {
    let mut files = Vec::new();
    rust_files(&src_dir(), &mut files);
    assert!(!files.is_empty(), "found no Rust sources to scan");

    let mut offenders: Vec<String> = Vec::new();
    for path in &files {
        // The doc comment on `center_on_first_show` names the call it replaces.
        if path.ends_with("ui/dialog_chrome.rs") {
            continue;
        }
        let text = std::fs::read_to_string(path).expect("read source");
        for (i, line) in text.lines().enumerate() {
            if line.contains(".anchor(egui::Align2::") || line.contains(".anchor(Align2::") {
                offenders.push(format!(
                    "{}:{}: {}",
                    path.strip_prefix(src_dir()).unwrap_or(path).display(),
                    i + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "an anchored egui window cannot be moved (Area::anchor calls movable(false)). \
         Use `center_on_first_show` + `.default_pos(..)` instead:\n  {}",
        offenders.join("\n  ")
    );
}
