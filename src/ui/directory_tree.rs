//! Sidebar directory tree: browse a folder (recursively) and open any file
//! into a new tab by clicking it.
//!
//! Each row spans the full panel width so clicking anywhere on the row
//! activates it (like a native file explorer), and the cursor stays as a
//! pointing hand instead of a text-selection I-beam.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use eframe::egui;

/// Persistent state for the directory tree sidebar.
pub struct DirectoryTreeState {
    /// Root path the user opened.
    pub root: PathBuf,
    /// Absolute paths of directories that are currently expanded.
    pub expanded: HashSet<PathBuf>,
    /// Files the user has Ctrl/Shift-selected for a batch action (Union).
    /// A plain click clears this and opens the file, so the selection only
    /// exists while the user is deliberately building one.
    pub selected: HashSet<PathBuf>,
    /// Anchor for Shift-range selection: the last file clicked with Ctrl or
    /// plainly. A Shift-click selects every file between it and the anchor.
    pub select_anchor: Option<PathBuf>,
}

impl DirectoryTreeState {
    pub fn new(root: PathBuf) -> Self {
        let mut expanded = HashSet::new();
        expanded.insert(root.clone());
        Self {
            root,
            expanded,
            selected: HashSet::new(),
            select_anchor: None,
        }
    }
}

/// What happened this frame in the tree UI.
#[derive(Default)]
pub struct TreeAction {
    /// File path the user clicked on and wants opened.
    pub open_file: Option<PathBuf>,
    /// User asked to close the sidebar.
    pub close: bool,
    /// User chose "Union selected files..." from a selected file's context
    /// menu. Carries the selected paths (always 2 or more).
    pub union_files: Option<Vec<PathBuf>>,
}

const INDENT_PER_LEVEL: f32 = 14.0;
const ARROW_WIDTH: f32 = 16.0;
const ROW_PADDING_X: f32 = 4.0;

/// Render the directory tree. Callers wrap this in a `SidePanel`.
///
/// When `allowed_exts` is `Some(set)`, only directories and files whose
/// lowercased extension is in `set` are listed (extensionless files are
/// hidden). `None` lists everything (dotfiles always excluded).
pub fn render_directory_tree(
    ui: &mut egui::Ui,
    state: &mut DirectoryTreeState,
    allowed_exts: Option<&HashSet<String>>,
) -> TreeAction {
    let mut action = TreeAction::default();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Directory").strong());
        if ui
            .small_button("×")
            .on_hover_text("Close the directory sidebar")
            .clicked()
        {
            action.close = true;
        }
    });
    let display_root = state
        .root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| state.root.to_string_lossy().to_string());
    ui.label(
        egui::RichText::new(&display_root)
            .size(11.0)
            .color(ui.visuals().weak_text_color()),
    )
    .on_hover_text(state.root.to_string_lossy().as_ref());

    // Selection bar: only present while the user has files Ctrl/Shift-selected.
    // The Union action also lives in the row context menu, but a context menu
    // is easy to miss, and the count is worth showing while a selection is
    // being built.
    if !state.selected.is_empty() {
        let count = state.selected.len();
        let mut clear = false;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{count} {}",
                    crate::i18n::t("union_tree.n_selected")
                ))
                .size(11.0),
            );
            if ui
                .add_enabled(
                    count >= 2,
                    egui::Button::new(crate::i18n::t("union_tree.union_btn")).small(),
                )
                .on_hover_text(crate::i18n::t("union_tree.selected_hint"))
                .clicked()
            {
                let mut files: Vec<PathBuf> = state.selected.iter().cloned().collect();
                files.sort();
                action.union_files = Some(files);
            }
            if ui
                .small_button("×")
                .on_hover_text(crate::i18n::t("union_tree.clear"))
                .clicked()
            {
                clear = true;
            }
        });
        if clear {
            state.selected.clear();
            state.select_anchor = None;
        }
    }

    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let root = state.root.clone();
            draw_dir(ui, &root, state, &mut action, 0, allowed_exts);
        });
    action
}

/// Whether a **file row** is actually on screen: not a directory, not hidden,
/// and not filtered out. Shift-range selection uses this so a range can only
/// ever pick up rows the user can see (the raw directory listing still holds
/// dotfiles and filtered-out files, which the draw loop skips).
fn file_row_visible(path: &Path, allowed_exts: Option<&HashSet<String>>) -> bool {
    if path.is_dir() {
        return false;
    }
    let hidden = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(true);
    !hidden && file_is_listed(path, allowed_exts)
}

/// Whether a file is listed under the current filter. Directories are always
/// shown; the filter only applies to files. A file whose extension isn't in
/// the set is hidden unless its filename is recognized by
/// `filename_reader_name` (e.g. `Dockerfile`), which keeps extension-less
/// openable files visible.
fn file_is_listed(path: &Path, allowed_exts: Option<&HashSet<String>>) -> bool {
    let Some(set) = allowed_exts else {
        return true;
    };
    let ext_ok = match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => set.contains(&ext.to_ascii_lowercase()),
        None => false,
    };
    if ext_ok {
        return true;
    }
    // Extension-less / unknown-extension conventions we still open (e.g.
    // `Dockerfile`, `Containerfile`). Reuses the reader's filename matcher.
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| crate::formats::filename_reader_name(n).is_some())
        .unwrap_or(false)
}

/// Render a single row that spans the full panel width and is clickable as a
/// whole. Returns the `Response` (already wired for hover cursor + tooltip).
fn draw_row(
    ui: &mut egui::Ui,
    depth: usize,
    is_dir: bool,
    is_open: bool,
    name: &str,
    selected: bool,
) -> egui::Response {
    let text_style = egui::TextStyle::Body;
    let font_id = text_style.resolve(ui.style());
    let row_height = ui.text_style_height(&text_style) + 6.0;
    let full_width = ui.available_width();

    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(full_width, row_height), egui::Sense::click());

    // Ctrl/Shift-selected rows (staged for a Union) keep a persistent tint;
    // hover is the lighter, transient highlight on top.
    if selected {
        ui.painter()
            .rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
    }
    // Hover highlight + pointer cursor.
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 2.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }

    let painter = ui.painter();
    let text_color = ui.visuals().text_color();

    // Draw caret (for directories) and name.
    let mut x = rect.left() + ROW_PADDING_X + depth as f32 * INDENT_PER_LEVEL;
    if is_dir {
        let caret = if is_open { "▼" } else { "▶" };
        painter.text(
            egui::pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            caret,
            font_id.clone(),
            text_color,
        );
    }
    x += ARROW_WIDTH;

    // Name: truncate if it would exceed the row.
    let max_name_width = (rect.right() - x - ROW_PADDING_X).max(0.0);
    let mut galley = painter.layout_no_wrap(name.to_string(), font_id.clone(), text_color);
    if galley.size().x > max_name_width {
        let ellipsis = "...";
        // Cheap character-based truncation (not perfect for variable-width fonts
        // but good enough for a sidebar).
        let mut truncated = name.to_string();
        while !truncated.is_empty() {
            truncated.pop();
            let candidate = format!("{truncated}{ellipsis}");
            galley = painter.layout_no_wrap(candidate, font_id.clone(), text_color);
            if galley.size().x <= max_name_width {
                break;
            }
        }
    }
    painter.galley(
        egui::pos2(x, rect.center().y - galley.size().y * 0.5),
        galley,
        text_color,
    );

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn draw_dir(
    ui: &mut egui::Ui,
    dir: &Path,
    state: &mut DirectoryTreeState,
    action: &mut TreeAction,
    depth: usize,
    allowed_exts: Option<&HashSet<String>>,
) {
    let entries = match read_sorted_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            ui.colored_label(
                egui::Color32::from_rgb(200, 80, 80),
                format!("<error: {err}>"),
            );
            return;
        }
    };

    for (idx, entry) in entries.iter().enumerate() {
        let entry = entry.clone();
        let is_dir = entry.is_dir();
        let name = entry
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        // Hide files Octa can't open when the filter is on. Directories are
        // always shown so the user can still navigate into them.
        if !is_dir && !file_is_listed(&entry, allowed_exts) {
            continue;
        }
        let is_open = is_dir && state.expanded.contains(&entry);
        let is_selected = !is_dir && state.selected.contains(&entry);
        let resp = draw_row(ui, depth, is_dir, is_open, &name, is_selected)
            .on_hover_text(entry.to_string_lossy().as_ref());

        let copy_name = name.clone();
        let selection_len = state.selected.len();
        let mut clear_selection = false;
        let mut union_now = false;
        resp.context_menu(|ui| {
            if ui
                .button(crate::i18n::t("context_menu.copy_name"))
                .clicked()
            {
                ui.ctx().copy_text(copy_name.clone());
                ui.close();
            }
            // Union is offered on a row that is part of a 2+ selection. On any
            // other row we explain how to build one rather than silently
            // omitting the entry.
            if !is_dir {
                ui.separator();
                if is_selected && selection_len >= 2 {
                    if ui
                        .button(format!(
                            "{} ({selection_len})",
                            crate::i18n::t("union_tree.selected")
                        ))
                        .on_hover_text(crate::i18n::t("union_tree.selected_hint"))
                        .clicked()
                    {
                        union_now = true;
                        ui.close();
                    }
                } else {
                    ui.add_enabled(
                        false,
                        egui::Button::new(crate::i18n::t("union_tree.need_two")),
                    );
                }
            }
            if selection_len > 0 && ui.button(crate::i18n::t("union_tree.clear")).clicked() {
                clear_selection = true;
                ui.close();
            }
        });
        if union_now {
            let mut files: Vec<PathBuf> = state.selected.iter().cloned().collect();
            files.sort();
            action.union_files = Some(files);
        }
        if clear_selection {
            state.selected.clear();
            state.select_anchor = None;
        }

        if resp.clicked() {
            if is_dir {
                // Directories ignore modifiers: they expand/collapse as always.
                if state.expanded.contains(&entry) {
                    state.expanded.remove(&entry);
                } else {
                    state.expanded.insert(entry.clone());
                }
            } else {
                let mods = ui.input(|i| i.modifiers);
                if mods.ctrl || mods.command {
                    // Toggle into the selection set, do not open.
                    if !state.selected.remove(&entry) {
                        state.selected.insert(entry.clone());
                    }
                    state.select_anchor = Some(entry.clone());
                } else if mods.shift {
                    // Range-select every listed file between the anchor and
                    // this row, within this directory's listing. An anchor in
                    // another directory falls back to selecting just this file.
                    let anchor_idx = state
                        .select_anchor
                        .as_ref()
                        .and_then(|a| entries.iter().position(|e| e == a));
                    match anchor_idx {
                        Some(from) => {
                            let (lo, hi) = if from <= idx {
                                (from, idx)
                            } else {
                                (idx, from)
                            };
                            for e in &entries[lo..=hi] {
                                if file_row_visible(e, allowed_exts) {
                                    state.selected.insert(e.clone());
                                }
                            }
                        }
                        None => {
                            state.selected.insert(entry.clone());
                            state.select_anchor = Some(entry.clone());
                        }
                    }
                } else {
                    // Plain click: drop any selection and open the file.
                    state.selected.clear();
                    state.select_anchor = Some(entry.clone());
                    action.open_file = Some(entry.clone());
                }
            }
        }

        if is_dir && state.expanded.contains(&entry) {
            draw_dir(ui, &entry, state, action, depth + 1, allowed_exts);
        }
    }
}

/// Read one directory's direct entries, sorted: directories first (alphabetical),
/// then files (alphabetical). Symlinks to files are treated as files.
pub fn read_sorted_dir(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    for ent in std::fs::read_dir(dir)? {
        let ent = ent?;
        let p = ent.path();
        if p.is_dir() {
            dirs.push(p);
        } else {
            files.push(p);
        }
    }
    dirs.sort_by(|a, b| {
        a.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
            .to_lowercase()
            .cmp(
                &b.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
                    .to_lowercase(),
            )
    });
    files.sort_by(|a, b| {
        a.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
            .to_lowercase()
            .cmp(
                &b.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
                    .to_lowercase(),
            )
    });
    dirs.extend(files);
    Ok(dirs)
}

#[cfg(test)]
#[path = "directory_tree_tests.rs"]
mod tests;
