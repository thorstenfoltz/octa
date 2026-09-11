//! Pick cloud objects to register as SQL-workspace tables.
//!
//! The workspace could already attach a saved **database** connection, but
//! "Add table" only reaches local files: it opens the operating system's file
//! picker, which knows nothing about a bucket. This dialog is the missing
//! half - it browses one saved cloud connection and registers what you tick.
//!
//! Browsing goes through the sidebar's own listing cache
//! (`OctaApp::ensure_cloud_listing`), so a folder already opened in the
//! sidebar is already listed here and no second network path exists.
//!
//! **Combining is opt-in.** Ticking several objects gives several tables. The
//! "combine into one table" box unions them instead, and it starts off: a
//! union reconciles differing schemas, and doing that to someone's data
//! because they happened to tick two files is not a decision Octa should make
//! for them.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

use super::super::cloud_browser::{ListState, data_objects, root_prefix, sorted_entries};
use super::super::state::OctaApp;

/// One open cloud picker. Session-only, like every other dialog's state.
pub(crate) struct CloudWorkspacePicker {
    pub(crate) conn_id: String,
    pub(crate) conn_name: String,
    /// Prefix being browsed. Starts at the connection's root prefix.
    pub(crate) prefix: String,
    /// Ticked objects as `(key, display name)`, in tick order so the workspace
    /// tables come out in the order the user built them up.
    pub(crate) picked: Vec<(String, String)>,
    /// Union the picked objects into a single table instead of one each.
    pub(crate) combine: bool,
    pub(crate) size: DialogSize,
}

impl OctaApp {
    /// Open the picker for a saved cloud connection (SQL workspace ->
    /// "Attach cloud connection").
    pub(crate) fn open_cloud_workspace_picker(&mut self, conn_id: &str) {
        let Some(conn) = self.find_cloud_conn(conn_id) else {
            return;
        };
        self.cloud_ws_picker = Some(CloudWorkspacePicker {
            conn_id: conn_id.to_string(),
            conn_name: conn.name.clone(),
            prefix: root_prefix(&conn),
            picked: Vec::new(),
            combine: false,
            size: DialogSize::default(),
        });
    }
}

pub(crate) fn render_cloud_workspace_picker(app: &mut OctaApp, ctx: &egui::Context) {
    // Everything the window needs, copied out before `app` is borrowed
    // mutably for the listing below.
    let Some((conn_id, prefix, title, mut size, mut combine, picked)) =
        app.cloud_ws_picker.as_ref().map(|s| {
            (
                s.conn_id.clone(),
                s.prefix.clone(),
                format!(
                    "{} - {}",
                    octa::i18n::t("sql.cloud_pick_title"),
                    s.conn_name
                ),
                s.size,
                s.combine,
                s.picked.clone(),
            )
        })
    else {
        return;
    };
    let minimized = size == DialogSize::Minimized;

    // The listing for the folder on screen, off the sidebar's shared cache.
    app.ensure_cloud_listing(ctx, conn_id.clone(), prefix.clone());
    let listing = app
        .cloud_browser
        .listings
        .lock()
        .ok()
        .and_then(|m| match m.get(&(conn_id.clone(), prefix.clone())) {
            Some(ListState::Loading) => Some(Listing::Loading),
            Some(ListState::Ready(entries)) => Some(Listing::Ready(entries.clone())),
            Some(ListState::Error(e)) => Some(Listing::Failed(e.clone())),
            None => None,
        })
        .unwrap_or(Listing::Loading);
    let sort = app.cloud_browser.sort;
    // Extension allowlist, from the reader registry on `self`: ticking a .png
    // would only fail at download time, so it is not offered.
    let allowed_exts: std::collections::HashSet<String> = app
        .registry
        .all_extensions()
        .into_iter()
        .map(|e| e.to_ascii_lowercase())
        .collect();

    let mut close = false;
    let mut add = false;
    let mut descend: Option<String> = None;
    let mut go_up = false;
    let mut toggled: Vec<(String, String)> = Vec::new();

    let dialog_id = egui::Id::new("octa_cloud_workspace_picker");
    let center = center_on_first_show(ctx, egui::vec2(520.0, 460.0));
    let window = egui::Window::new("octa_cloud_workspace_picker")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .default_height(460.0)
            .min_width(360.0)
            .min_height(240.0)
            .default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("cloud_ws_picker_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&title).strong().size(16.0));
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

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !prefix.is_empty(),
                        egui::Button::new(octa::i18n::t("sql.cloud_pick_up")),
                    )
                    .on_hover_text(octa::i18n::t("sql.cloud_pick_up_hint"))
                    .on_disabled_hover_text(octa::i18n::t("sql.cloud_pick_at_root"))
                    .clicked()
                {
                    go_up = true;
                }
                ui.label(
                    RichText::new(if prefix.is_empty() {
                        "/"
                    } else {
                        prefix.as_str()
                    })
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
                );
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt("cloud_ws_picker_scroll")
                .auto_shrink([false, false])
                .max_height(260.0)
                .show(ui, |ui| match &listing {
                    Listing::Loading => {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().size(14.0));
                            ui.label(octa::i18n::t("cloud.loading"));
                        });
                        ui.ctx().request_repaint();
                    }
                    Listing::Failed(e) => {
                        octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
                    }
                    Listing::Ready(entries) => {
                        let sorted = sorted_entries(entries, sort);
                        let mut any = false;
                        for entry in &sorted {
                            if entry.is_prefix {
                                any = true;
                                if ui
                                    .button(format!("[ ] {}", entry.name))
                                    .on_hover_text(octa::i18n::t("sql.cloud_pick_open_folder"))
                                    .clicked()
                                {
                                    descend = Some(entry.key.clone());
                                }
                            }
                        }
                        // Only files a reader can actually open are offered;
                        // ticking a .png would fail at download time.
                        let owned: Vec<octa::cloud::ObjectEntry> =
                            sorted.iter().map(|e| (*e).clone()).collect();
                        for (key, name) in data_objects(&owned, &allowed_exts) {
                            any = true;
                            let mut on = picked.iter().any(|(k, _)| k == &key);
                            if ui.checkbox(&mut on, &name).changed() {
                                toggled.push((key.clone(), name.clone()));
                            }
                        }
                        if !any {
                            ui.label(
                                RichText::new(octa::i18n::t("sql.cloud_pick_empty"))
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                    }
                });

            ui.separator();
            ui.label(
                RichText::new(
                    octa::i18n::t("sql.cloud_pick_selected")
                        .replace("{n}", &picked.len().to_string()),
                )
                .size(11.0)
                .color(ui.visuals().weak_text_color()),
            );
            ui.add_enabled_ui(picked.len() > 1, |ui| {
                ui.checkbox(&mut combine, octa::i18n::t("sql.cloud_pick_combine"))
                    .on_hover_text(octa::i18n::t("sql.cloud_pick_combine_hint"))
                    .on_disabled_hover_text(octa::i18n::t("sql.cloud_pick_combine_needs_two"));
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !picked.is_empty(),
                        egui::Button::new(octa::i18n::t("sql.cloud_pick_add")),
                    )
                    .on_hover_text(octa::i18n::t("sql.cloud_pick_add_hint"))
                    .on_disabled_hover_text(octa::i18n::t("sql.cloud_pick_none"))
                    .clicked()
                {
                    add = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(octa::i18n::t("common.cancel")).clicked() {
                        close = true;
                    }
                });
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }

    let Some(state) = app.cloud_ws_picker.as_mut() else {
        return;
    };
    state.size = if close || add {
        DialogSize::Normal
    } else {
        size
    };
    state.combine = combine;
    for (key, name) in toggled {
        match state.picked.iter().position(|(k, _)| k == &key) {
            Some(i) => {
                state.picked.remove(i);
            }
            None => state.picked.push((key, name)),
        }
    }
    if let Some(key) = descend {
        state.prefix = key;
    }
    if go_up {
        state.prefix = parent_prefix(&state.prefix);
    }
    if add {
        let picked = std::mem::take(&mut state.picked);
        let combine = state.combine;
        let conn_id = state.conn_id.clone();
        app.cloud_ws_picker = None;
        app.start_cloud_workspace_fetch(ctx, &conn_id, picked, combine);
    } else if close {
        app.cloud_ws_picker = None;
    }
}

/// The listing of the folder on screen. A local mirror of the sidebar's
/// `ListState` so the lock is released before the window is drawn.
enum Listing {
    Loading,
    Ready(Vec<octa::cloud::ObjectEntry>),
    Failed(String),
}

/// One level up from an object-store prefix. Prefixes end in `/`, so the last
/// separator has to be ignored to find the parent's.
fn parent_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(i) => trimmed[..=i].to_string(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::parent_prefix;

    #[test]
    fn parent_prefix_walks_up_one_level_at_a_time() {
        assert_eq!(parent_prefix("a/b/c/"), "a/b/");
        assert_eq!(parent_prefix("a/b/"), "a/");
        // The last level up lands at the bucket root, not at "a".
        assert_eq!(parent_prefix("a/"), "");
        assert_eq!(parent_prefix(""), "");
    }
}
