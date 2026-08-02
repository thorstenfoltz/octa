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
    /// While a press-and-drag marquee is active: the selection that existed
    /// when the drag began, so a Ctrl-drag adds to it instead of replacing it.
    /// `None` when not dragging.
    pub marquee: Option<Marquee>,
}

/// In-progress rubber-band selection. The band's geometry lives in egui temp
/// memory (see [`drive_marquee`]), shared with the cloud tree; all this holds
/// is what the band adds to.
pub struct Marquee {
    /// Selection to union the band into (empty unless the drag began with Ctrl).
    pub base: HashSet<PathBuf>,
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
            marquee: None,
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
    /// User chose "Open as dataset..." on a directory: open the folder of
    /// part files as one table (Hive partitioning).
    pub open_dataset: Option<PathBuf>,
}

const INDENT_PER_LEVEL: f32 = 14.0;
const ARROW_WIDTH: f32 = 16.0;
const ROW_PADDING_X: f32 = 4.0;

/// Indices of rows whose vertical centre lies strictly within the band
/// between `a` and `b` (inclusive of the bounds, exclusive of a zero-height
/// band). `a`/`b` need not be ordered, so an upward drag works like a
/// downward one. Shared by the cloud tree's marquee.
pub fn indices_in_band(centers: &[f32], a: f32, b: f32) -> Vec<usize> {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    if lo == hi {
        return Vec::new();
    }
    centers
        .iter()
        .enumerate()
        .filter(|&(_, &c)| c >= lo && c <= hi)
        .map(|(i, _)| i)
        .collect()
}

/// How far the pointer has to move after a press before it counts as a
/// rubber-band drag rather than a click on the row underneath.
const MARQUEE_START_THRESHOLD: f32 = 4.0;

/// Distance from the viewport edge at which a drag starts scrolling the list.
const MARQUEE_EDGE_ZONE: f32 = 24.0;
/// Fastest auto-scroll, in points per frame.
const MARQUEE_MAX_SPEED: f32 = 18.0;

/// One frame of the rubber-band, in the coordinate space that survives
/// scrolling. Shared by the local and cloud trees.
///
/// **Content coordinates, not screen coordinates.** The band anchor has to
/// stay attached to the row it started on while the list scrolls under it;
/// storing a screen position makes the band drift by exactly the scroll
/// distance, so a drag that scrolls ends up selecting the wrong rows.
/// `content_top` (`ui.min_rect().top()`, which moves with the content) is the
/// origin everything is measured from.
#[derive(Clone, Copy)]
pub struct MarqueeFrame {
    /// Band anchor, in content space.
    pub start_y: f32,
    /// Pointer now, in content space.
    pub current_y: f32,
    /// Content-space origin, for converting back when painting.
    pub content_top: f32,
}

impl MarqueeFrame {
    /// Whether a row centred at this screen y is inside the band.
    pub fn contains_row(&self, centers: &[f32]) -> Vec<usize> {
        let content: Vec<f32> = centers.iter().map(|c| c - self.content_top).collect();
        indices_in_band(&content, self.start_y, self.current_y)
    }

    /// The band as a screen-space rect spanning `x_range`.
    pub fn band_rect(&self, x_range: egui::Rangef) -> egui::Rect {
        let (a, b) = (
            self.start_y + self.content_top,
            self.current_y + self.content_top,
        );
        egui::Rect::from_x_y_ranges(x_range, egui::Rangef::new(a.min(b), a.max(b)))
    }
}

/// Start / continue / end a rubber-band selection, and scroll the list when the
/// pointer is dragged past its edge. Returns the current frame while a band is
/// active, `None` otherwise.
///
/// Deliberately driven from raw pointer state rather than a background
/// `Response`, because the background is only the *empty* part of the panel:
/// in a narrow sidebar full of filenames there is barely any, so a drag that
/// happened to start on a row silently did nothing. Rows sense clicks, not
/// drags, so a press that turns into a drag is unambiguous and safe to claim
/// here; a press that does not move still reaches the row as a click.
///
/// `ctrl_extends` reports whether the drag started with Ctrl/Cmd held, so the
/// caller can union with its existing selection.
pub fn drive_marquee(
    ui: &egui::Ui,
    mem_id: egui::Id,
    viewport: egui::Rect,
    ctrl_extends: &mut bool,
) -> Option<MarqueeFrame> {
    let content_top = ui.min_rect().top();
    let (down, press_origin, pos) = ui.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.press_origin(),
            i.pointer.interact_pos().or(i.pointer.latest_pos()),
        )
    });

    #[derive(Clone, Copy)]
    struct State {
        start_content: f32,
        last_content: f32,
        ctrl: bool,
    }

    let mut state = ui.memory(|m| m.data.get_temp::<State>(mem_id));

    if state.is_none()
        && down
        && let (Some(origin), Some(now)) = (press_origin, pos)
        // Started inside this list...
        && viewport.contains(origin)
        // ...moved far enough to be a drag and not a click...
        && (now - origin).length() > MARQUEE_START_THRESHOLD
        // ...and no other widget (a scrollbar, a splitter) already owns it.
        && ui.ctx().dragged_id().is_none()
    {
        state = Some(State {
            start_content: origin.y - content_top,
            last_content: now.y - content_top,
            ctrl: ui.input(|i| i.modifiers.ctrl || i.modifiers.command),
        });
    }

    let mut st = state?;
    if !down {
        ui.memory_mut(|m| m.data.remove::<State>(mem_id));
        return None;
    }

    // A pointer dragged outside the window reports no position; hold the last
    // one rather than collapsing the band (which would clear the selection).
    if let Some(now) = pos {
        st.last_content = now.y - content_top;

        // Auto-scroll when held near or past an edge, so a selection can run
        // past the visible rows. Speed ramps with how far outside it is.
        let past_bottom = now.y - (viewport.bottom() - MARQUEE_EDGE_ZONE);
        let past_top = (viewport.top() + MARQUEE_EDGE_ZONE) - now.y;
        let dy = if past_bottom > 0.0 {
            -past_bottom.min(MARQUEE_MAX_SPEED)
        } else if past_top > 0.0 {
            past_top.min(MARQUEE_MAX_SPEED)
        } else {
            0.0
        };
        if dy != 0.0 {
            // `scroll_with_delta` is inverted: negative y scrolls down.
            ui.scroll_with_delta(egui::vec2(0.0, dy));
            // The pointer may not move again, so ask for the next frame here or
            // the scroll would stall as soon as the user holds still.
            ui.ctx().request_repaint();
        }
    }
    ui.memory_mut(|m| m.data.insert_temp(mem_id, st));
    *ctrl_extends = st.ctrl;
    Some(MarqueeFrame {
        start_y: st.start_content,
        current_y: st.last_content,
        content_top,
    })
}

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
            // The visible strip of the list. `clip_rect` (not `max_rect`) is
            // the viewport: it is what the pointer has to be inside for a drag
            // to belong to this list, and what the auto-scroll measures its
            // edges from.
            let viewport = ui.clip_rect();

            let root = state.root.clone();
            let mut rows: Vec<(egui::Rect, PathBuf)> = Vec::new();
            draw_dir(ui, &root, state, &mut action, 0, allowed_exts, &mut rows);

            apply_marquee(ui, state, viewport, &rows);
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
    rows: &mut Vec<(egui::Rect, PathBuf)>,
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
        if !is_dir {
            rows.push((resp.rect, entry.clone()));
        }

        let copy_name = name.clone();
        let selection_len = state.selected.len();
        let mut clear_selection = false;
        let mut union_now = false;
        let mut open_dataset = false;
        resp.context_menu(|ui| {
            if ui
                .button(crate::i18n::t("context_menu.copy_name"))
                .clicked()
            {
                ui.ctx().copy_text(copy_name.clone());
                ui.close();
            }
            if is_dir {
                ui.separator();
                if ui
                    .button(crate::i18n::t("dataset.open_as_dataset"))
                    .on_hover_text(crate::i18n::t("dataset.open_as_dataset_hint"))
                    .clicked()
                {
                    open_dataset = true;
                    ui.close();
                }
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
        if open_dataset {
            action.open_dataset = Some(entry.clone());
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
            draw_dir(ui, &entry, state, action, depth + 1, allowed_exts, rows);
        }
    }
}

/// Drive the rubber-band selection for one frame: start/continue/end the drag,
/// paint the translucent band, and set `state.selected` to `base ∪ rows-in-band`.
fn apply_marquee(
    ui: &egui::Ui,
    state: &mut DirectoryTreeState,
    viewport: egui::Rect,
    rows: &[(egui::Rect, PathBuf)],
) {
    let mem_id = ui.id().with("dir_marquee_state");
    let mut ctrl = false;
    let Some(frame) = drive_marquee(ui, mem_id, viewport, &mut ctrl) else {
        // The band ended (or never started); the selection it produced stays.
        state.marquee = None;
        return;
    };
    // Remember the base selection once, on the frame the band appears, so a
    // Ctrl-drag adds to what was already selected instead of to itself.
    if state.marquee.is_none() {
        state.marquee = Some(Marquee {
            base: if ctrl {
                state.selected.clone()
            } else {
                HashSet::new()
            },
        });
    }
    let base = state
        .marquee
        .as_ref()
        .map(|m| m.base.clone())
        .unwrap_or_default();

    // Recompute selection = base ∪ file rows whose centre is in the band.
    let centers: Vec<f32> = rows.iter().map(|(r, _)| r.center().y).collect();
    let mut selected = base;
    for i in frame.contains_row(&centers) {
        selected.insert(rows[i].1.clone());
    }
    state.selected = selected;
    let cutoff = frame.current_y + frame.content_top;
    if let Some(last) = rows.iter().rev().find(|(r, _)| r.center().y <= cutoff) {
        state.select_anchor = Some(last.1.clone());
    }

    // Paint the band across the panel width (feedback only).
    let band = frame.band_rect(viewport.x_range());
    let fill = ui.visuals().selection.bg_fill.linear_multiply(0.25);
    ui.painter().rect_filled(band, 2.0, fill);
    ui.painter().rect_stroke(
        band,
        2.0,
        egui::Stroke::new(1.0_f32, ui.visuals().selection.stroke.color),
        egui::StrokeKind::Inside,
    );
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
