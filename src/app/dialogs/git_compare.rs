//! Revision-picker for "Compare with git version" and "Open git version in a
//! new tab". Lists recent commits touching the active file (default HEAD); the
//! chosen revision is read via `octa::git::show_at` into a temp file and either
//! fed to the Compare view or opened as a new tab.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::OctaApp;

pub(crate) fn render_git_compare_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.git_compare_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut do_compare = false;
    let mut do_open_tab = false;
    let mut st = app.git_compare_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_git_compare_dialog");
    let window = egui::Window::new("octa_git_compare")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(260.0)
            .min_width(360.0)
            .min_height(160.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("git_compare_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.gitcmp_title"))
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

        egui::Panel::bottom("git_compare_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("dialog.gitcmp_compare")).clicked() {
                        do_compare = true;
                    }
                    if ui.button(octa::i18n::t("dialog.gitcmp_open_tab")).clicked() {
                        do_open_tab = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("common.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("dialog.gitcmp_intro"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("dialog.gitcmp_against"));
                egui::ComboBox::from_id_salt("gitcmp_rev")
                    .selected_text(st.selected_label.clone())
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        // HEAD default entry.
                        let head_label = octa::i18n::t("dialog.gitcmp_head");
                        if ui
                            .selectable_label(st.selected_rev == "HEAD", &head_label)
                            .clicked()
                        {
                            st.selected_rev = "HEAD".to_string();
                            st.selected_label = head_label.clone();
                        }
                        for c in &st.commits {
                            let label = format!("{} - {}  ({})", c.sha, c.subject, c.rel_time);
                            if ui
                                .selectable_label(st.selected_rev == c.sha, &label)
                                .clicked()
                            {
                                st.selected_rev = c.sha.clone();
                                st.selected_label = label;
                            }
                        }
                    });
            });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if do_compare || do_open_tab {
        match octa::git::show_at(&st.repo_root, &st.selected_rev, &st.relpath) {
            Ok(bytes) => {
                if do_compare {
                    app.begin_compare_with_git_bytes(bytes, &st.ext, st.selected_label.clone());
                } else {
                    app.open_git_bytes_in_new_tab(bytes, &st.ext, &st.relpath, &st.selected_rev);
                }
            }
            Err(e) => {
                app.status_message = Some((format!("git error: {e}"), std::time::Instant::now()));
            }
        }
        return; // dialog consumed
    }
    if !close {
        app.git_compare_dialog = Some(st);
    }
}
