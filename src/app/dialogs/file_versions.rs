//! File -> Earlier versions: the versions `octa::file_versions` kept of the
//! active tab's file, newest first, with Open (a copy in a new tab), Compare
//! (with this tab) and Restore (put it back as the file, keeping what the file
//! holds now as a version first). It works whether or not keeping is still
//! switched on, so versions kept before it was switched off stay reachable.

use std::path::PathBuf;

use eframe::egui;
use egui::RichText;

use octa::file_versions::Version;
use octa::i18n::t;
use octa::ui::settings::{DialogSize, dialog_header, remember_dialog_rect, size_dialog_window};

use crate::app::state::OctaApp;

pub(crate) struct FileVersionsState {
    pub(crate) size: DialogSize,
    /// The tab the dialog belongs to, and the file the versions are of (a
    /// decompressed tab's `.gz` original, not its temp).
    pub(crate) tab_idx: usize,
    pub(crate) file: PathBuf,
    pub(crate) versions: Vec<Version>,
    pub(crate) selected: Option<usize>,
    /// Set on a Delta / Iceberg tab: the table's own history replaces the
    /// kept copies, and the footer and list come from `file_versions_lake`.
    pub(crate) lake: Option<super::file_versions_lake::LakeVersions>,
}

/// What the footer asked for this frame.
enum Act {
    Open(Version),
    Compare(Version),
    Restore(Version),
    Settings,
}

impl OctaApp {
    /// Entry from the File menu and its shortcut.
    pub(crate) fn open_file_versions_dialog(&mut self) {
        let dir = self.tabs[self.active_tab]
            .table
            .source_path
            .as_ref()
            .map(PathBuf::from)
            .filter(|p| p.is_dir());
        if let Some(dir) = dir
            && self.open_lake_versions(&dir)
        {
            return;
        }
        let tab = &self.tabs[self.active_tab];
        let file = match (&tab.cloud_origin, &tab.compressed_origin) {
            (Some(_), _) => None,
            (None, Some(origin)) => Some(origin.original.clone()),
            (None, None) => tab.table.source_path.as_ref().map(PathBuf::from),
        };
        let Some(file) = file else {
            self.status_message = Some((t("dialog.versions_no_file"), std::time::Instant::now()));
            return;
        };
        let versions = octa::file_versions::list_all(&self.settings.version_stores(), &file);
        self.file_versions_dialog = Some(FileVersionsState {
            size: DialogSize::default(),
            tab_idx: self.active_tab,
            selected: (!versions.is_empty()).then_some(0),
            file,
            versions,
            lake: None,
        });
    }

    /// Put `version` back as the dialog's file and read the tab again.
    fn restore_version(&mut self, st: &FileVersionsState, version: &Version) {
        let Some(store) = self.settings.version_store() else {
            return;
        };
        let limits = self.settings.version_limits();
        if let Err(e) = octa::file_versions::restore(&store, &version.path, &st.file, limits) {
            self.status_message = Some((format!("{e:#}"), std::time::Instant::now()));
            return;
        }
        // A decompressed tab reads its temp copy: refresh that from the
        // restored original before reading it again.
        if let Some(origin) = self.tabs[st.tab_idx].compressed_origin.clone() {
            let cap = self.settings.max_decompressed();
            let refreshed =
                octa::formats::compression::decompress_to_temp(&origin.original, origin.codec, cap)
                    .and_then(|tmp| Ok(std::fs::copy(tmp.path(), &origin.temp)?));
            if let Err(e) = refreshed {
                self.status_message = Some((format!("{e:#}"), std::time::Instant::now()));
                return;
            }
        }
        self.active_tab = st.tab_idx;
        self.reload_active_file();
        self.status_message = Some((
            t("dialog.versions_restored").replace("{time}", &version.when),
            std::time::Instant::now(),
        ));
    }
}

pub(crate) fn render_file_versions_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.file_versions_dialog.take() else {
        return;
    };
    // The tab was closed underneath the dialog.
    if st.tab_idx >= app.tabs.len() {
        if let Some(lake) = st.lake.as_mut() {
            lake.cancel();
        }
        return;
    }
    if let Some(lake) = st.lake.as_mut()
        && let Some((outcome, older, newer)) = lake.take_finished()
    {
        match outcome {
            Ok(diff) => app.open_lake_diff_tab(&st.file, diff, &older, &newer),
            Err(e) => app.status_message = Some((e, std::time::Instant::now())),
        }
    }
    let mut close = false;
    let mut act: Option<Act> = None;
    let mut lake_act = None;
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;
    let keeping = app.settings.keep_versions;
    let modified = app.tabs[st.tab_idx].is_modified();
    let name = st
        .file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let dialog_id = egui::Id::new("octa_file_versions_dialog");
    let window = egui::Window::new("octa_file_versions")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(320.0)
            .min_width(340.0)
            .min_height(180.0)
    });

    let inner = window.show(ctx, |ui| {
        close |= dialog_header(
            ui,
            "file_versions_header",
            RichText::new(t("dialog.versions_title").replace("{name}", &name)),
            &mut size,
        );
        if minimized {
            return;
        }

        egui::Panel::bottom("file_versions_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                if let Some(lake) = st.lake.as_ref() {
                    lake_act = super::file_versions_lake::render_lake_footer(
                        ui,
                        lake,
                        st.selected,
                        &mut close,
                    );
                    return;
                }
                let picked = st.selected.and_then(|i| st.versions.get(i)).cloned();
                let pick_first = t("dialog.versions_pick");
                octa::ui::control_row::control_line(ui, |ui| {
                    if ui
                        .add_enabled(picked.is_some(), egui::Button::new(t("common.open")))
                        .on_hover_text(t("dialog.versions_open_hint"))
                        .on_disabled_hover_text(&pick_first)
                        .clicked()
                    {
                        act = picked.clone().map(Act::Open);
                    }
                    if ui
                        .add_enabled(
                            picked.is_some(),
                            egui::Button::new(t("dialog.versions_compare")),
                        )
                        .on_hover_text(t("dialog.versions_compare_hint"))
                        .on_disabled_hover_text(&pick_first)
                        .clicked()
                    {
                        act = picked.clone().map(Act::Compare);
                    }
                    let restore_off = if modified {
                        t("dialog.versions_restore_modified")
                    } else {
                        pick_first.clone()
                    };
                    if ui
                        .add_enabled(
                            picked.is_some() && !modified,
                            egui::Button::new(t("dialog.versions_restore")),
                        )
                        .on_hover_text(t("dialog.versions_restore_hint"))
                        .on_disabled_hover_text(&restore_off)
                        .clicked()
                    {
                        act = picked.clone().map(Act::Restore);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("common.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(lake) = st.lake.as_ref() {
                super::file_versions_lake::render_lake_list(ui, lake, &mut st.selected);
                return;
            }
            if st.versions.is_empty() {
                let text = if keeping {
                    t("dialog.versions_none")
                } else {
                    t("dialog.versions_off")
                };
                ui.label(text);
                if !keeping {
                    ui.add_space(6.0);
                    if ui
                        .button(t("dialog.versions_open_settings"))
                        .on_hover_text(t("dialog.versions_open_settings_hint"))
                        .clicked()
                    {
                        act = Some(Act::Settings);
                    }
                }
                return;
            }
            let intro = if keeping {
                t("dialog.versions_intro")
            } else {
                t("dialog.versions_off_note")
            };
            ui.label(RichText::new(intro).small().weak());
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (i, v) in st.versions.iter().enumerate() {
                        let label =
                            format!("{}    {}", v.when, octa::ui::status_bar::human_size(v.size));
                        let resp = ui
                            .selectable_label(st.selected == Some(i), label)
                            .on_hover_text(t("dialog.versions_row_hint"));
                        if resp.clicked() {
                            st.selected = Some(i);
                        }
                        if resp.double_clicked() {
                            act = Some(Act::Compare(v.clone()));
                        }
                    }
                });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if let Some(lake_act) = lake_act
        && let Some(lake) = st.lake.as_mut()
    {
        match lake_act {
            super::file_versions_lake::LakeAct::Open(v) => {
                app.open_lake_version(&st.file, lake.kind, &v);
                return;
            }
            super::file_versions_lake::LakeAct::Diff {
                older,
                newer,
                older_label,
                newer_label,
            } => app.start_lake_diff(
                ctx,
                lake,
                st.file.clone(),
                (older, older_label),
                (newer, newer_label),
            ),
            super::file_versions_lake::LakeAct::Cancel => lake.cancel(),
        }
    }

    match act {
        Some(Act::Open(v)) => {
            match std::fs::read(&v.path) {
                Ok(bytes) => {
                    let ext = st
                        .file
                        .extension()
                        .map(|e| e.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    app.open_git_bytes_in_new_tab(bytes, &ext, &name, &v.when);
                }
                Err(e) => {
                    app.status_message = Some((format!("{e:#}"), std::time::Instant::now()));
                }
            }
            return;
        }
        Some(Act::Compare(v)) => {
            app.active_tab = st.tab_idx;
            app.begin_compare_with_path(v.path);
            app.status_message = Some((
                t("dialog.versions_comparing").replace("{time}", &v.when),
                std::time::Instant::now(),
            ));
            return;
        }
        Some(Act::Restore(v)) => {
            app.restore_version(&st, &v);
            return;
        }
        Some(Act::Settings) => {
            app.settings_dialog.open(&app.settings);
            app.settings_dialog.focus_saving_section = true;
            return;
        }
        None => {}
    }
    if close {
        // A comparison nobody will see should not keep DuckDB busy.
        if let Some(lake) = st.lake.as_mut() {
            lake.cancel();
        }
    } else {
        app.file_versions_dialog = Some(st);
    }
}
