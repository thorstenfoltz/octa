//! Compare folders dialog: the folders to compare (local ones picked here,
//! cloud and local ones added from the sidebars), the subfolder switch, and
//! the running scan. The worker and the result tab live in
//! `crate::app::folder_compare`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::RichText;

use octa::data::DataTable;
use octa::data::folder_compare::FolderListing;
use octa::i18n::t;
use octa::ui::settings::{DialogSize, dialog_header, remember_dialog_rect, size_dialog_window};

use crate::app::folder_compare::GuiFolderRoot;
use crate::app::state::OctaApp;

/// What the worker hands back: the table and the listings it was built from,
/// or the error as text.
pub(crate) type FolderCompareResult = Result<(DataTable, Vec<FolderListing>), String>;

pub(crate) struct FolderCompareState {
    pub(crate) roots: Vec<GuiFolderRoot>,
    /// The roots the running (or last) scan was started with, so editing the
    /// list mid-run cannot mislabel the result.
    pub(crate) run_roots: Vec<GuiFolderRoot>,
    pub(crate) recursive: bool,
    pub(crate) running: Arc<AtomicBool>,
    pub(crate) cancel: Arc<AtomicBool>,
    /// Already-localised progress text the worker writes.
    pub(crate) progress: Arc<Mutex<String>>,
    pub(crate) result: Arc<Mutex<Option<FolderCompareResult>>>,
    pub(crate) size: DialogSize,
}

impl FolderCompareState {
    pub(crate) fn new() -> Self {
        Self {
            roots: Vec::new(),
            run_roots: Vec::new(),
            recursive: true,
            running: Arc::default(),
            cancel: Arc::default(),
            progress: Arc::default(),
            result: Arc::default(),
            size: DialogSize::default(),
        }
    }

    /// Add a folder unless it is already in the list; `true` when added.
    pub(crate) fn add_root(&mut self, root: GuiFolderRoot) -> bool {
        if self.roots.contains(&root) {
            return false;
        }
        self.roots.push(root);
        true
    }
}

pub(crate) fn render_folder_compare_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.folder_compare.take() else {
        return;
    };
    let running = st.running.load(Ordering::Acquire);
    let shown: Vec<String> = st
        .roots
        .iter()
        .map(|r| app.folder_root_display(r))
        .collect();
    let progress = st.progress.lock().map(|p| p.clone()).unwrap_or_default();

    let mut close = false;
    let mut run = false;
    let mut pick_dir = false;
    let mut remove: Option<usize> = None;
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_folder_compare_dialog");
    let window = egui::Window::new("octa_folder_compare")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .default_height(360.0)
            .min_width(360.0)
            .min_height(200.0)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "folder_compare_header",
            RichText::new(t("folder_compare.title")),
            &mut size,
        ) {
            close = true;
        }

        if minimized {
            return;
        }

        egui::Panel::bottom("folder_compare_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                octa::ui::control_row::control_line(ui, |ui| {
                    if running {
                        ui.spinner();
                        ui.label(&progress);
                        if ui
                            .button(t("folder_compare.cancel"))
                            .on_hover_text(t("folder_compare.cancel_hint"))
                            .clicked()
                        {
                            st.cancel.store(true, Ordering::Relaxed);
                        }
                    } else {
                        let compare = ui
                            .add_enabled(
                                st.roots.len() >= 2,
                                egui::Button::new(t("folder_compare.compare")),
                            )
                            .on_hover_text(t("folder_compare.compare_hint"))
                            .on_disabled_hover_text(t("folder_compare.compare_disabled_hint"));
                        if compare.clicked() {
                            run = true;
                        }
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(t("folder_compare.intro"));
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .id_salt("folder_compare_roots")
                .auto_shrink([false, true])
                .max_height(200.0)
                .show(ui, |ui| {
                    if shown.is_empty() {
                        ui.weak(t("folder_compare.none_yet"));
                    }
                    for (i, name) in shown.iter().enumerate() {
                        // Button first: a long path must not push it off the row.
                        octa::ui::control_row::control_line(ui, |ui| {
                            if ui
                                .button(t("folder_compare.remove"))
                                .on_hover_text(t("folder_compare.remove_hint"))
                                .clicked()
                            {
                                remove = Some(i);
                            }
                            ui.add(egui::Label::new(name).selectable(true).truncate());
                        });
                    }
                });
            ui.add_space(6.0);
            octa::ui::control_row::control_line(ui, |ui| {
                if ui
                    .button(t("folder_compare.choose"))
                    .on_hover_text(t("folder_compare.choose_hint"))
                    .clicked()
                {
                    pick_dir = true;
                }
                ui.checkbox(&mut st.recursive, t("folder_compare.recursive"))
                    .on_hover_text(t("folder_compare.recursive_hint"));
            });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if let Some(i) = remove {
        st.roots.remove(i);
    }
    // The folder picker blocks, so it runs after the frame is laid out.
    if pick_dir && let Some(dir) = rfd::FileDialog::new().pick_folder() {
        st.add_root(GuiFolderRoot::Local(dir));
    }

    if close {
        // Closing stops a running scan; its result is dropped with the state.
        st.cancel.store(true, Ordering::Relaxed);
        return;
    }
    app.folder_compare = Some(st);
    if run {
        app.start_folder_compare(ctx);
    }
}
