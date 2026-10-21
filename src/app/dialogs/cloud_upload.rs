//! Save the open table to a cloud bucket, an SFTP server or a WebDAV
//! folder: **File > Save to cloud/SFTP...**, and the cloud tree's **Save the
//! active tab here...** on a folder.
//!
//! Picking where is all this dialog does. The writing is a Save As into a
//! private temp folder (`begin_upload`), so the upload gets everything Save
//! As has: the row and CSV questions, background writing, the large-file
//! export. Where that copy lands (`finish_table_save`, the text save, the
//! large export) `upload_saved_copy` hands it to the cloud upload worker,
//! which removes the folder once it is done. The tab itself stays as it was.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui::RichText;

use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_result_message, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::cloud_browser::{bind_bucket, root_prefix};
use super::super::state::OctaApp;

pub(crate) struct CloudUploadState {
    pub(crate) size: DialogSize,
    pub(crate) tab_idx: usize,
    pub(crate) conn_id: String,
    pub(crate) key: String,
    /// The file name the path was built from, kept for a change of connection.
    pub(crate) file_name: String,
    /// The user typed into the path: picking another connection keeps it.
    pub(crate) key_edited: bool,
    pub(crate) error: Option<String>,
}

/// An upload waiting for its copy to be written. Dropping it removes the
/// folder, and the copy with it.
///
/// ponytail: a save that stops at a deferred question (rounding, `.xlsx`
/// formatting) which the user then cancels keeps its entry, and its empty
/// folder, until Octa exits; forget it from those prompts if that matters.
pub(crate) struct PendingUpload {
    pub(crate) path: PathBuf,
    pub(crate) conn_id: String,
    pub(crate) key: String,
    pub(crate) dir: octa::formats::encrypt::ProtectedTempDir,
}

/// Where a file named `name` goes on `conn` by default: its start folder, or
/// `folder` when the upload came from a folder in the tree.
fn default_key(conn: &octa::cloud::CloudConnection, folder: Option<&str>, name: &str) -> String {
    let folder = folder
        .map(str::to_string)
        .unwrap_or_else(|| root_prefix(conn));
    match folder.trim_matches('/') {
        "" => name.to_string(),
        f => format!("{f}/{name}"),
    }
}

/// The address `key` on `conn` stands for, as the transfer dialog shows it.
fn url_of(conn: &octa::cloud::CloudConnection, key: &str) -> String {
    let (bound, key) = bind_bucket(conn, key);
    format!("{}://{}/{key}", bound.kind.scheme(), bound.bucket)
}

impl OctaApp {
    /// Open the upload dialog for the active tab. `at` is a (connection,
    /// folder) picked in the cloud tree.
    pub(crate) fn open_cloud_upload(&mut self, at: Option<(String, String)>) {
        let tab_idx = self.active_tab;
        let file_name = self.upload_file_name(tab_idx);
        let conns = &self.settings.cloud_connections;
        let origin = self.tabs[tab_idx]
            .cloud_origin
            .as_ref()
            .map(|o| o.conn_id.clone());
        // The folder picked in the tree, else the tab's own connection, else
        // the first one that may be written to.
        let conn = at
            .as_ref()
            .and_then(|(id, _)| conns.iter().find(|c| &c.id == id))
            .or_else(|| conns.iter().find(|c| Some(&c.id) == origin.as_ref()))
            .or_else(|| conns.iter().find(|c| c.allow_writes))
            .or_else(|| conns.first());
        let (conn_id, key) = match conn {
            Some(c) => (
                c.id.clone(),
                default_key(c, at.as_ref().map(|(_, f)| f.as_str()), &file_name),
            ),
            None => (String::new(), file_name.clone()),
        };
        self.cloud_upload_dialog = Some(CloudUploadState {
            size: DialogSize::Normal,
            tab_idx,
            conn_id,
            key,
            file_name,
            key_edited: false,
            error: None,
        });
    }

    /// The name a tab's file gets in the cloud: the object it came from, the
    /// file it was read from, or its label with the format it remembers.
    fn upload_file_name(&self, tab_idx: usize) -> String {
        let tab = &self.tabs[tab_idx];
        // A cloud tab's source is a temp copy with a made-up name.
        if let Some(origin) = &tab.cloud_origin
            && let Some(name) = origin.key.rsplit('/').next().filter(|n| !n.is_empty())
        {
            return name.to_string();
        }
        if let Some(name) = tab
            .table
            .source_path
            .as_deref()
            .and_then(|p| Path::new(p).file_name())
        {
            return name.to_string_lossy().into_owned();
        }
        if let Some(name) = self.result_tab_file_name(tab_idx) {
            return name;
        }
        if tab.saves_text() {
            "untitled.txt".to_string()
        } else {
            "untitled.csv".to_string()
        }
    }

    /// What stops `st` from uploading, in the user's words.
    fn upload_problem(&self, st: &CloudUploadState) -> Option<String> {
        let Some(conn) = self.find_cloud_conn(&st.conn_id) else {
            return Some(t("upload.pick_conn"));
        };
        if !conn.allow_writes {
            return Some(t("cloud.conn_writes_off").replace("{name}", &conn.name));
        }
        let key = st.key.trim();
        let name = key.rsplit('/').next().unwrap_or_default();
        if name.is_empty() {
            return Some(t("upload.need_name"));
        }
        let tab = self.tabs.get(st.tab_idx)?;
        // Raw text is written as typed, under any name.
        if tab.saves_text() {
            return None;
        }
        let writable = if tab.large.is_some() {
            octa::formats::large::ExportFormat::for_path(
                Path::new(name),
                None,
                &self.settings.write_options.parquet,
            )
            .is_some()
        } else {
            self.registry
                .reader_for_path(Path::new(name))
                .is_some_and(|r| r.supports_write())
        };
        (!writable).then(|| t("upload.no_writer").replace("{name}", name))
    }

    /// Write the tab into a private temp folder through Save As, and upload
    /// it once it is written (`upload_saved_copy`).
    fn start_upload(&mut self, st: &mut CloudUploadState) -> bool {
        if let Some(problem) = self.upload_problem(st) {
            st.error = Some(problem);
            return false;
        }
        let Some(conn) = self.find_cloud_conn(&st.conn_id) else {
            return false;
        };
        let key = st.key.trim().to_string();
        let name = key.rsplit('/').next().unwrap_or_default().to_string();
        let dir = match octa::formats::encrypt::ProtectedTempDir::new() {
            Ok(dir) => dir,
            Err(e) => {
                st.error = Some(format!("{e:#}"));
                return false;
            }
        };
        let path = dir.path().join(&name);
        let url = url_of(&conn, &key);
        self.pending_uploads.push(PendingUpload {
            path: path.clone(),
            conn_id: conn.id.clone(),
            key,
            dir,
        });
        self.begin_upload(st.tab_idx, path, url);
        true
    }

    /// A copy was written to `path`: upload it if an upload waits for it.
    pub(crate) fn upload_saved_copy(&mut self, path: &Path) {
        if let Some(i) = self.pending_uploads.iter().position(|u| u.path == path) {
            let u = self.pending_uploads.swap_remove(i);
            self.start_cloud_upload(u.conn_id, u.key, u.path, Some(u.dir));
        }
    }

    /// The copy for `path` will not be written: drop its upload and folder.
    pub(crate) fn forget_upload(&mut self, path: &Path) {
        self.pending_uploads.retain(|u| u.path != path);
    }
}

pub(crate) fn render_cloud_upload_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.cloud_upload_dialog.take() else {
        return;
    };
    // The tab closed while the dialog was open.
    if st.tab_idx >= app.tabs.len() {
        return;
    }
    let mut close = false;
    let mut upload = false;
    let mut open_settings = false;
    let mut load_all = false;
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;
    let partial = app.tabs[st.tab_idx]
        .table
        .partial_note()
        .filter(|_| app.tabs[st.tab_idx].large.is_none());

    let dialog_id = egui::Id::new("octa_cloud_upload_dialog");
    let window = egui::Window::new("octa_cloud_upload")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(230.0)
            .min_width(360.0)
            .min_height(160.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("cloud_upload_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                octa::ui::control_row::control_line(ui, |ui| {
                    ui.label(RichText::new(t("upload.title")).strong().size(16.0));
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
        let no_conns = app.settings.cloud_connections.is_empty();
        egui::Panel::bottom("cloud_upload_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                if let Some(msg) = &st.error {
                    draw_result_message(ui, false, msg);
                    ui.add_space(4.0);
                }
                octa::ui::control_row::control_line(ui, |ui| {
                    let go = ui
                        .add_enabled(!no_conns, egui::Button::new(t("common.save")))
                        .on_hover_text(t("upload.go_hint"))
                        .on_disabled_hover_text(t("upload.no_conns"));
                    if go.clicked() {
                        upload = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("common.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            if no_conns {
                ui.label(t("upload.no_conns"));
                ui.add_space(6.0);
                if ui
                    .button(t("dialog.versions_open_settings"))
                    .on_hover_text(t("upload.open_settings_hint"))
                    .clicked()
                {
                    open_settings = true;
                }
                return;
            }
            egui::Grid::new("cloud_upload_grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    octa::ui::control_row::pin_control_height(ui);
                    ui.label(t("upload.conn"))
                        .on_hover_text(t("upload.conn_hint"));
                    let selected = app
                        .settings
                        .cloud_connections
                        .iter()
                        .find(|c| c.id == st.conn_id)
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| t("dialog.dbc_pick"));
                    egui::ComboBox::from_id_salt("cloud_upload_conn")
                        .selected_text(selected)
                        .width(240.0)
                        .show_ui(ui, |ui| {
                            for c in app.settings.cloud_connections.iter() {
                                let mut label = format!("{} ({})", c.name, c.kind.scheme());
                                if !c.allow_writes {
                                    label.push_str(&format!("  [{}]", t("db.copy_writes_off")));
                                }
                                if ui.selectable_label(st.conn_id == c.id, label).clicked()
                                    && st.conn_id != c.id
                                {
                                    st.conn_id = c.id.clone();
                                    st.error = None;
                                    if !st.key_edited {
                                        st.key = default_key(c, None, &st.file_name);
                                    }
                                }
                            }
                        })
                        .response
                        .on_hover_text(t("upload.conn_hint"));
                    ui.end_row();

                    ui.label(t("upload.path"))
                        .on_hover_text(t("upload.path_hint"));
                    ui.vertical(|ui| {
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut st.key)
                                    .desired_width(280.0)
                                    .font(egui::TextStyle::Monospace),
                            )
                            .on_hover_text(t("upload.path_hint"))
                            .changed()
                        {
                            st.key_edited = true;
                            st.error = None;
                        }
                        // Where it lands, resolved as the upload resolves it.
                        if let Some(c) = app
                            .settings
                            .cloud_connections
                            .iter()
                            .find(|c| c.id == st.conn_id)
                        {
                            ui.label(
                                RichText::new(url_of(c, st.key.trim()))
                                    .monospace()
                                    .size(10.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                    });
                    ui.end_row();
                });
            // A table holding a window of its file uploads only that window.
            if let Some((loaded, known_total)) = partial {
                ui.add_space(8.0);
                ui.label(t("dialog.partial_save_as_body"));
                ui.label(
                    RichText::new(octa::ui::message::partial_note_text(loaded, known_total))
                        .color(octa::ui::message::PARTIAL_NOTE_COLOR),
                );
                if ui
                    .button(t("dialog.partial_save_load_all"))
                    .on_hover_text(t("dialog.partial_save_load_all_hint"))
                    .clicked()
                {
                    load_all = true;
                }
            }
        });
    });

    if let Some(r) = &inner {
        remember_dialog_rect(ctx, dialog_id, size, r.response.rect);
    }
    st.size = size;

    if open_settings {
        app.settings_dialog.open(&app.settings);
        app.settings_dialog.focus_cloud_section = true;
        app.settings_dialog.focus_cloud_form = true;
        close = true;
    }
    if load_all {
        app.active_tab = st.tab_idx;
        app.reload_without_row_cap();
        close = true;
    }
    if upload && app.start_upload(&mut st) {
        close = true;
    }
    if !close {
        app.cloud_upload_dialog = Some(st);
    }
}

#[cfg(test)]
#[path = "cloud_upload_tests.rs"]
mod tests;
