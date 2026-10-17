//! Guard: a row of controls is an `octa::ui::control_row` row, never a bare
//! `ui.horizontal`.
//!
//! A bare row starts 18px tall while Octa's themed buttons are 26-30px, so a
//! label or checkbox placed before the first button centres 4-5px above it and
//! a ComboBox placed after one sinks by as much. "Fields not in line" was
//! reported dialog after dialog, title bars included, because each fix was
//! local; over 300 rows were converted at once, and this keeps new ones from
//! slipping back. `src/ui/control_row_tests.rs` measures the alignment itself.
//!
//! The scan relies on `cargo fmt`: a row's closing brace sits at the indent of
//! the line that opened it.

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

/// Hand-tuned strips that are not form rows: the toolbar pins its own band,
/// the tab bar and the sidebar trees are lists of small controls, the status
/// bar's text is smaller than a button. Tests build bare rows on purpose.
const EXEMPT_FILES: &[&str] = &[
    "ui/toolbar/",
    "app/tabs/tab_bar.rs",
    "ui/directory_tree.rs",
    "ui/tree_filter.rs",
    "app/cloud_tree.rs",
    "app/db_tree.rs",
    "ui/status_bar.rs",
    "_tests.rs",
    "/tests.rs",
];

/// Widgets as tall as a themed button.
const TALL: &[&str] = &[
    ".button(",
    "Button::new",
    "Button::selectable",
    "ComboBox::",
    "TextEdit::",
    "control_text_edit",
    "draw_window_controls",
    "selectable_value(",
    "selectable_label(",
    ".menu_button(",
    ".toggle_value(",
];

/// Rows that are layouts rather than form lines (columns, lists, sliders,
/// whose handle would grow with a pinned row), left as they are.
const LAYOUT: &[&str] = &[
    "ui.vertical",
    "ScrollArea",
    "Grid::new",
    "control_grid",
    "ProgressBar",
    "ui.group(",
    ".columns(",
    "CollapsingHeader",
    ".collapsing(",
    "Slider::",
    "color_edit",
    "ui.horizontal",
    "control_row(",
    "allocate_ui",
];

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Every scanned source as (path relative to `src/`, text).
fn sources() -> Vec<(String, String)> {
    let mut files = Vec::new();
    rust_files(&src_dir(), &mut files);
    assert!(!files.is_empty(), "found no Rust sources to scan");
    files
        .iter()
        .map(|path| {
            let rel = path.strip_prefix(src_dir()).unwrap_or(path);
            let rel = rel.to_string_lossy().replace('\\', "/");
            (rel, std::fs::read_to_string(path).expect("read source"))
        })
        .filter(|(rel, _)| !EXEMPT_FILES.iter().any(|e| rel.contains(e)))
        .collect()
}

#[test]
fn rows_of_controls_go_through_control_row() {
    let mut offenders: Vec<String> = Vec::new();
    for (rel_s, text) in sources() {
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if !(code.contains("ui.horizontal(") || code.contains("ui.horizontal_wrapped(")) {
                continue;
            }
            let end = if code.trim_end().ends_with('{') {
                (i + 1..lines.len())
                    .find(|&j| {
                        indent(lines[j]) == indent(line) && lines[j].trim_start().starts_with('}')
                    })
                    .unwrap_or(lines.len() - 1)
            } else {
                i
            };
            let first = code.find("ui.horizontal").unwrap_or(0);
            let body = std::iter::once(&code[first + "ui.horizontal".len()..])
                .chain(lines[i + 1..=end].iter().copied())
                .collect::<Vec<_>>()
                .join("\n");
            let tall: usize = TALL.iter().map(|t| body.matches(t).count()).sum();
            let small = body.matches(".small()").count();
            if tall > small && !LAYOUT.iter().any(|l| body.contains(l)) {
                offenders.push(format!("{rel_s}:{}: {}", i + 1, line.trim()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "a bare `ui.horizontal` holding buttons, combos or text fields comes out \
         ragged under Octa's themes. Use `octa::ui::control_row::control_line` \
         (or `control_row` for a wrapping row):\n  {}",
        offenders.join("\n  ")
    );
}

/// A ComboBox wraps itself in a `ui.horizontal` whose band is
/// `interact_size.y`, so in a grid it sinks below the buttons and text fields
/// beside it unless the grid pins that height. Conditional formatting's rule
/// rows were the report.
#[test]
fn grids_of_combos_pin_the_control_height() {
    let mut offenders: Vec<String> = Vec::new();
    for (rel_s, text) in sources() {
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.split("//").next().unwrap_or("").contains("Grid::new(") {
                continue;
            }
            // The grid statement ends at the first line back at its indent.
            let end = (i + 1..lines.len())
                .find(|&j| !lines[j].trim().is_empty() && indent(lines[j]) <= indent(line))
                .unwrap_or(lines.len() - 1);
            let body = lines[i..=end].join("\n");
            if body.contains("ComboBox") && !body.contains("pin_control_height(") {
                offenders.push(format!("{rel_s}:{}: {}", i + 1, line.trim()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "a grid holding a ComboBox must call \
         `octa::ui::control_row::pin_control_height(ui)` first in its `show` \
         closure (or be a `control_grid`):\n  {}",
        offenders.join("\n  ")
    );
}
