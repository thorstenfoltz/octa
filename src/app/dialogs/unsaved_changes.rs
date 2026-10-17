//! "Unsaved changes" confirmation dialogs - one for closing (whole app or
//! single tab) and one for opening a different file.

use eframe::egui;

use super::super::state::OctaApp;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

pub(crate) fn render_close_confirm_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_close_confirm {
        return;
    }
    let dialog_id = egui::Id::new("octa_close_confirm_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(440.0, 220.0));
    let window = egui::Window::new("octa_close_confirm")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(440.0)
            .default_height(220.0)
            .min_width(320.0)
            .min_height(150.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("close_confirm_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                octa::ui::control_row::control_line(ui, |ui| {
                    ui.label(
                        egui::RichText::new(octa::i18n::t("dialog.unsaved_title"))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            chrome_close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(octa::i18n::t("dialog.unsaved_body"));
            ui.add_space(12.0);
            octa::ui::control_row::control_line(ui, |ui| {
                if ui.button(octa::i18n::t("common.save")).clicked() {
                    app.show_close_confirm = false;
                    if let Some(tab_idx) = app.pending_close_tab {
                        // A tab with no file of its own, or a cloud object
                        // that may not be written, can only be saved as a
                        // new file: Save would refuse and close nothing.
                        // A save written in the background closes the tab
                        // once it worked (`AfterSave`); one written at once is
                        // checked below.
                        let running = app.tabs[tab_idx].saving.is_some();
                        if app.tabs[tab_idx].saves_in_place() && app.cloud_tab_writable(tab_idx) {
                            app.save_tab(tab_idx);
                        } else {
                            app.save_tab_as(tab_idx);
                        }
                        app.pending_close_tab = None;
                        if !running && let Some(job) = app.tabs[tab_idx].saving.as_mut() {
                            job.then = Some(crate::app::file_io::save_job::AfterSave::CloseTab);
                        }
                        // A live-database tab's Save raised the write-back
                        // confirmation instead of saving; closing now would
                        // discard the very edits being confirmed.
                        // Also wait on an in-flight write: `drain_db_write_back_job`
                        // re-tags `tabs[tab_idx]`, and closing a tab now would
                        // shift that index onto a different tab.
                        // And on the save having worked: a failed one, or one
                        // still waiting on a question, leaves the tab unsaved,
                        // and closing it would throw the edits away (for a
                        // protected zip, with the plain copy holding them).
                        // The status line says what happened.
                        if app.pending_db_write_back.is_none()
                            && app.db_write_back_job.is_none()
                            && !app.tabs.get(tab_idx).is_some_and(|t| t.is_modified())
                            && app.tabs.get(tab_idx).is_some_and(|t| t.saving.is_none())
                        {
                            app.close_tab(tab_idx);
                        }
                    } else {
                        let active = app.active_tab;
                        let running = app.tabs[active].saving.is_some();
                        if app.tabs[active].saves_in_place() && app.cloud_tab_writable(active) {
                            app.save_file();
                        } else {
                            app.save_file_as();
                        }
                        if !running && let Some(job) = app.tabs[active].saving.as_mut() {
                            job.then = Some(crate::app::file_io::save_job::AfterSave::Exit);
                        }
                        // Exit only once the save has worked, as for one tab.
                        if app.pending_db_write_back.is_none()
                            && !app.tabs[active].is_modified()
                            && app.tabs[active].saving.is_none()
                        {
                            app.confirmed_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
                if ui.button(octa::i18n::t("dialog.dont_save")).clicked() {
                    app.show_close_confirm = false;
                    if let Some(tab_idx) = app.pending_close_tab {
                        app.close_tab(tab_idx);
                        app.pending_close_tab = None;
                    } else {
                        app.confirmed_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                if ui.button(octa::i18n::t("common.cancel")).clicked() {
                    app.show_close_confirm = false;
                    app.pending_close_tab = None;
                }
            });
        });
    });
    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if chrome_close {
                DialogSize::Normal
            } else {
                size
            },
        )
    });
    if chrome_close {
        app.show_close_confirm = false;
        app.pending_close_tab = None;
    }
}

pub(crate) fn render_open_confirm_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_open_confirm {
        return;
    }
    let dialog_id = egui::Id::new("octa_open_confirm_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(440.0, 220.0));
    let window = egui::Window::new("octa_open_confirm")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(440.0)
            .default_height(220.0)
            .min_width(320.0)
            .min_height(150.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("open_confirm_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                octa::ui::control_row::control_line(ui, |ui| {
                    ui.label(
                        egui::RichText::new(octa::i18n::t("dialog.unsaved_title"))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            chrome_close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(octa::i18n::t("dialog.unsaved_body"));
            ui.add_space(12.0);
            octa::ui::control_row::control_line(ui, |ui| {
                if ui.button(octa::i18n::t("common.save")).clicked() {
                    app.show_open_confirm = false;
                    if app.tabs[app.active_tab].saves_in_place() {
                        app.save_file();
                    } else {
                        app.save_file_as();
                    }
                    app.do_open_file_dialog();
                }
                if ui.button(octa::i18n::t("dialog.dont_save")).clicked() {
                    app.show_open_confirm = false;
                    app.tabs[app.active_tab].table.clear_modified();
                    app.tabs[app.active_tab].raw_content_modified = false;
                    app.do_open_file_dialog();
                }
                if ui.button(octa::i18n::t("common.cancel")).clicked() {
                    app.show_open_confirm = false;
                    app.pending_open_file = false;
                }
            });
        });
    });
    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if chrome_close {
                DialogSize::Normal
            } else {
                size
            },
        )
    });
    if chrome_close {
        app.show_open_confirm = false;
        app.pending_open_file = false;
    }
}
