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
}

impl DirectoryTreeState {
    pub fn new(root: PathBuf) -> Self {
        let mut expanded = HashSet::new();
        expanded.insert(root.clone());
        Self { root, expanded }
    }
}

/// What happened this frame in the tree UI.
#[derive(Default)]
pub struct TreeAction {
    /// File path the user clicked on and wants opened.
    pub open_file: Option<PathBuf>,
    /// User asked to close the sidebar.
    pub close: bool,
}

const INDENT_PER_LEVEL: f32 = 14.0;
const ARROW_WIDTH: f32 = 16.0;
const ROW_PADDING_X: f32 = 4.0;

/// Render the directory tree. Callers wrap this in a `SidePanel`.
pub fn render_directory_tree(ui: &mut egui::Ui, state: &mut DirectoryTreeState) -> TreeAction {
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
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let root = state.root.clone();
            draw_dir(ui, &root, state, &mut action, 0);
        });
    action
}

/// Render a single row that spans the full panel width and is clickable as a
/// whole. Returns the `Response` (already wired for hover cursor + tooltip).
fn draw_row(
    ui: &mut egui::Ui,
    depth: usize,
    is_dir: bool,
    is_open: bool,
    name: &str,
) -> egui::Response {
    let text_style = egui::TextStyle::Body;
    let font_id = text_style.resolve(ui.style());
    let row_height = ui.text_style_height(&text_style) + 6.0;
    let full_width = ui.available_width();

    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(full_width, row_height), egui::Sense::click());

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
        let ellipsis = "…";
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

    for entry in entries {
        let is_dir = entry.is_dir();
        let name = entry
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        let is_open = is_dir && state.expanded.contains(&entry);
        let resp = draw_row(ui, depth, is_dir, is_open, &name)
            .on_hover_text(entry.to_string_lossy().as_ref());
        if resp.clicked() {
            if is_dir {
                if state.expanded.contains(&entry) {
                    state.expanded.remove(&entry);
                } else {
                    state.expanded.insert(entry.clone());
                }
            } else {
                action.open_file = Some(entry.clone());
            }
        }

        if is_dir && state.expanded.contains(&entry) {
            draw_dir(ui, &entry, state, action, depth + 1);
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
mod tests {
    use super::*;

    #[test]
    fn sort_puts_directories_first() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("zdir")).unwrap();
        std::fs::write(tmp.path().join("afile.txt"), "").unwrap();
        std::fs::write(tmp.path().join("bfile.txt"), "").unwrap();
        let out = read_sorted_dir(tmp.path()).unwrap();
        let names: Vec<String> = out
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["zdir", "afile.txt", "bfile.txt"]);
    }

    #[test]
    fn state_has_root_expanded_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let s = DirectoryTreeState::new(tmp.path().to_path_buf());
        assert!(s.expanded.contains(&tmp.path().to_path_buf()));
    }
}
