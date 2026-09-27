//! Cell history dialog (cell right-click **Cell history...**). Follows one
//! cell through the Git history of the tab's file and lists the commits in
//! which its value changed, newest first.
//!
//! Versions are read on a worker, 50 commits a page, into the tab's
//! `cell_history_cache`, so a second cell of the same tab opens instantly.
//! The pure part is [`octa::data::cell_history`]; the Git part is
//! [`octa::git::history`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::RichText;

use octa::data::cell_history::{
    CellState, CommitInfo, RowRef, Version, cell_history, change_id, first_unique_column,
    key_values,
};
use octa::i18n::t;
use octa::ui::control_row::control_row;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{CellHistoryCache, CellHistoryState, OctaApp, TabState};

/// Commits read per page (first open and each **Load older**).
const PAGE: usize = 50;

impl OctaApp {
    /// Open Cell history for `row` (an index into the table, not the
    /// filtered view) and `col` of the active tab. Does nothing for a tab
    /// whose file is not in a Git repository.
    pub(crate) fn open_cell_history(&mut self, row: usize, col: usize) {
        let Some((root, rel)) = self.active_git_location() else {
            return;
        };
        let cap = if self.settings.max_decompressed_unlimited {
            u64::MAX
        } else {
            self.settings.max_decompressed_bytes
        };
        let tab_idx = self.active_tab;
        let tab = &mut self.tabs[tab_idx];
        let Some(column) = tab.table.columns.get(col).map(|c| c.name.clone()) else {
            return;
        };
        let head = octa::git::head_sha(&root).unwrap_or_default();
        if tab
            .cell_history_cache
            .as_ref()
            .is_some_and(|c| c.head != head)
        {
            tab.cell_history_cache = None;
        }
        let mut current = tab.table.clone();
        current.apply_edits();
        let path = std::path::PathBuf::from(tab.table.source_path.clone().unwrap_or_default());
        let differs =
            tab.is_modified() || octa::git::history::working_copy_differs(&root, &rel, &path);
        let working = differs.then(|| Version {
            commit: CommitInfo {
                subject: t("cellhist.not_committed"),
                ..Default::default()
            },
            table: current.clone(),
        });
        let mut st = CellHistoryState {
            tab: tab_idx,
            row,
            column,
            keys: first_unique_column(&current).into_iter().collect(),
            keys_guessed: true,
            working,
            root,
            path,
            head,
            pending: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(false)),
            history: None,
            error: None,
            size: DialogSize::default(),
        };
        if tab.cell_history_cache.is_none() {
            start_load(&st, 0, cap);
        } else {
            suggest_keys(&mut st, tab);
            recompute(&mut st, tab);
        }
        self.cell_history_dialog = Some(st);
    }
}

/// Read commits `skip..skip + PAGE` on a worker into `st.pending`.
fn start_load(st: &CellHistoryState, skip: usize, cap: u64) {
    let (pending, running, path) = (st.pending.clone(), st.running.clone(), st.path.clone());
    running.store(true, Ordering::Relaxed);
    std::thread::spawn(move || {
        // Clears `running` however the worker ends, so a panic never leaves
        // the spinner on.
        let _running = crate::app::flag_guard::FlagOnDrop::new(running, false);
        let page = octa::git::history::load_versions(&path, skip, PAGE, cap);
        if let Ok(mut slot) = pending.lock() {
            *slot = Some(page);
        }
    });
}

/// The working copy (if any) followed by every cached version, newest first.
fn all_versions(st: &CellHistoryState, cache: &CellHistoryCache) -> Vec<Version> {
    let mut versions: Vec<Version> = st.working.iter().cloned().collect();
    versions.extend(cache.versions.iter().cloned());
    versions
}

/// Replace the first-unique-column guess with a key suggested from the
/// newest and oldest versions, once there is an oldest one to compare with.
fn suggest_keys(st: &mut CellHistoryState, tab: &TabState) {
    let Some(cache) = &tab.cell_history_cache else {
        return;
    };
    if !st.keys_guessed {
        return;
    }
    let versions = all_versions(st, cache);
    let (Some(newest), Some(oldest)) = (versions.first(), versions.last()) else {
        return;
    };
    if versions.len() < 2 {
        return;
    }
    st.keys_guessed = false;
    if let Some(name) = octa::data::merge_versions::suggest_key(&[&newest.table, &oldest.table])
        && let Some(i) = newest.table.columns.iter().position(|c| c.name == name)
    {
        st.keys = vec![i];
    }
}

// ponytail: clones every version per recompute; only runs on a key change or
// a page load. Switch `cell_history` to `&[&Version]` if large files make key
// changes slow.
fn recompute(st: &mut CellHistoryState, tab: &TabState) {
    let Some(cache) = &tab.cell_history_cache else {
        return;
    };
    let versions = all_versions(st, cache);
    let Some(newest) = versions.first() else {
        st.history = None;
        return;
    };
    let names: Vec<String> = st
        .keys
        .iter()
        .filter_map(|&i| newest.table.columns.get(i).map(|c| c.name.clone()))
        .collect();
    let row = match key_values(&newest.table, &names, st.row) {
        Some(values) if !names.is_empty() => RowRef::Key {
            columns: names,
            values,
            position: st.row,
        },
        _ => RowRef::Position(st.row),
    };
    st.history = Some(cell_history(&versions, &row, &st.column));
}

fn state_text(state: &CellState) -> String {
    match state {
        CellState::Value(v) => v.clone(),
        CellState::RowAbsent => t("cellhist.row_absent"),
        CellState::ColumnAbsent => t("cellhist.column_absent"),
    }
}

/// `s` cut to `max` characters with a trailing `...`.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max).collect::<String>())
    }
}

pub(crate) fn render_cell_history_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.cell_history_dialog.take() else {
        return;
    };
    if st.tab >= app.tabs.len() {
        return;
    }
    let cap = if app.settings.max_decompressed_unlimited {
        u64::MAX
    } else {
        app.settings.max_decompressed_bytes
    };

    // Move a finished page into the tab's cache.
    let is_running = st.running.load(Ordering::Relaxed);
    if !is_running && let Some(page) = st.pending.lock().ok().and_then(|mut s| s.take()) {
        let tab = &mut app.tabs[st.tab];
        match page {
            Ok(page) => {
                match &mut tab.cell_history_cache {
                    Some(cache) => {
                        cache.versions.extend(page.versions);
                        cache.unreadable.extend(page.unreadable);
                        cache.more = page.more;
                    }
                    None => {
                        tab.cell_history_cache = Some(CellHistoryCache {
                            head: st.head.clone(),
                            versions: page.versions,
                            unreadable: page.unreadable,
                            more: page.more,
                        });
                    }
                }
                st.error = None;
            }
            Err(e) => st.error = Some(format!("{e:#}")),
        }
        let tab = &app.tabs[st.tab];
        suggest_keys(&mut st, tab);
        recompute(&mut st, tab);
    }

    let tab = &app.tabs[st.tab];
    let col_names: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();
    let (more, loaded, unreadable) = match &tab.cell_history_cache {
        Some(c) => (
            c.more,
            c.versions.len() + c.unreadable.len(),
            c.unreadable.clone(),
        ),
        None => (false, 0, Vec::new()),
    };

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;
    let mut close = false;
    let mut load_older = false;
    let mut keys_changed = false;
    let mut open_version: Option<CommitInfo> = None;

    let dialog_id = egui::Id::new("octa_cell_history_dialog");
    let window = egui::Window::new("octa_cell_history")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(640.0)
            .default_height(460.0)
            .min_width(420.0)
            .min_height(240.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("cell_history_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} - {}", t("cellhist.title"), st.column))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }

        egui::Panel::bottom("cell_history_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let older = ui
                        .add_enabled(
                            more && !is_running,
                            egui::Button::new(t("cellhist.load_older")),
                        )
                        .on_hover_text(t("cellhist.load_older_hint"));
                    let older = if is_running {
                        older.on_disabled_hover_text(t("cellhist.loading"))
                    } else {
                        older.on_disabled_hover_text(t("cellhist.no_older"))
                    };
                    if older.clicked() {
                        load_older = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("cellhist.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            // The key picker on top, then a hand-drawn splitter to drag its
            // height, then the history. Hand-drawn like the table picker's,
            // because a resizable `egui::Panel` here never showed a handle:
            // the scroll areas on both sides took the pointer first.
            let split_id = egui::Id::new("octa_cell_history_key_height");
            let mut key_h: f32 = ui
                .ctx()
                .data_mut(|d| d.get_persisted(split_id))
                .unwrap_or(90.0);
            let max_key_h = (ui.available_height() - 120.0).max(40.0);
            key_h = key_h.clamp(40.0, max_key_h);
            ui.label(t("cellhist.key"))
                .on_hover_text(t("cellhist.key_hint"));
            let before = st.keys.clone();
            let picker = ui
                .scope(|ui| {
                    crate::app::dialogs::widgets::multi_col_picker_sized(
                        ui,
                        "cellhist_keys",
                        &mut st.keys,
                        &col_names,
                        Some(key_h),
                    );
                })
                .response;
            picker.on_hover_text(t("cellhist.key_hint"));
            if st.keys != before {
                st.keys_guessed = false;
                keys_changed = true;
            }
            let splitter = ui.allocate_response(
                egui::vec2(ui.available_width(), 8.0),
                egui::Sense::click_and_drag(),
            );
            if splitter.hovered() || splitter.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            }
            if splitter.dragged() {
                key_h = (key_h + splitter.drag_delta().y).clamp(40.0, max_key_h);
            }
            let stroke = if splitter.hovered() || splitter.dragged() {
                ui.visuals().widgets.hovered.bg_stroke
            } else {
                ui.visuals().widgets.noninteractive.bg_stroke
            };
            let mid_y = splitter.rect.center().y;
            ui.painter().hline(splitter.rect.x_range(), mid_y, stroke);
            ui.ctx().data_mut(|d| d.insert_persisted(split_id, key_h));

            let warn = ui.visuals().warn_fg_color;
            if let Some(e) = &st.error {
                octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
            }
            if let Some(h) = &st.history
                && !h.positional.is_empty()
            {
                octa::ui::message::selectable_message(
                    ui,
                    warn,
                    &t("cellhist.positional_banner")
                        .replace("{count}", &h.positional.len().to_string()),
                );
            }
            for (commit, error) in &unreadable {
                octa::ui::message::selectable_message(
                    ui,
                    warn,
                    &t("cellhist.unreadable")
                        .replace("{commit}", &commit.sha)
                        .replace("{error}", error),
                );
            }
            if is_running {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(t("cellhist.loading"));
                });
            }
            let Some(history) = &st.history else {
                return;
            };
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for entry in &history.entries {
                        control_row(ui, |ui| {
                            let c = &entry.commit;
                            if c.is_working_copy() {
                                ui.label(RichText::new(&c.subject).italics());
                            } else {
                                ui.label(RichText::new(&c.date).monospace());
                                ui.label(&c.author);
                                ui.label(clip(&c.subject, 60)).on_hover_text(&c.subject);
                            }
                            ui.label(RichText::new(state_text(&entry.state)).strong());
                            ui.label(
                                RichText::new(t(&format!("cellhist.change_{}", change_id(entry))))
                                    .color(ui.visuals().weak_text_color()),
                            );
                            if !c.is_working_copy()
                                && ui
                                    .button(t("cellhist.open_version"))
                                    .on_hover_text(t("cellhist.open_version_hint"))
                                    .clicked()
                            {
                                open_version = Some(c.clone());
                            }
                        });
                    }
                });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if keys_changed {
        let tab = st.tab;
        recompute(&mut st, &app.tabs[tab]);
    }
    if load_older {
        start_load(&st, loaded, cap);
    }
    if st.running.load(Ordering::Relaxed) {
        ctx.request_repaint();
    }
    if let Some(commit) = open_version {
        // `commit.path`, not today's name: an old sha under a renamed
        // file's current path does not exist.
        match octa::git::show_at(&st.root, &commit.sha, &commit.path) {
            Ok(bytes) => {
                let ext = std::path::Path::new(&commit.path)
                    .extension()
                    .map(|e| e.to_string_lossy().into_owned())
                    .unwrap_or_default();
                app.open_git_bytes_in_new_tab(bytes, &ext, &commit.path, &commit.sha);
            }
            Err(e) => st.error = Some(format!("{e:#}")),
        }
    }
    if !close {
        app.cell_history_dialog = Some(st);
    }
}
