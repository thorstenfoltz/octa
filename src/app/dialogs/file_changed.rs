//! "File changed on disk" confirmation. Raised by an in-place Save when the
//! source file's modification time or size no longer matches what the tab
//! recorded when it read (or last wrote) the file.
//!
//! Save As is deliberately not guarded: the user picked that path in a file
//! dialog moments ago, and the picker asks about overwriting itself.

use eframe::egui;

use super::super::state::OctaApp;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

pub(crate) fn render_file_changed_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(tab_idx) = app.pending_overwrite_confirm else {
        return;
    };
    // The tab could have been closed while this was up; without the guard the
    // buttons below would act on whatever tab slid into that index.
    let Some(path) = app
        .tabs
        .get(tab_idx)
        .and_then(|t| t.table.source_path.clone())
    else {
        app.pending_overwrite_confirm = None;
        return;
    };
    let dialog_id = egui::Id::new("octa_file_changed_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(460.0, 260.0));
    let window = egui::Window::new("octa_file_changed")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(260.0)
            .min_width(320.0)
            .min_height(160.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("file_changed_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(octa::i18n::t("dialog.file_changed_title"))
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
            ui.label(octa::i18n::t("dialog.file_changed_body"));
            ui.add_space(4.0);
            // Selectable: the path is the first thing a user wants to paste
            // into a terminal to see what happened to the file.
            let colour = ui.visuals().weak_text_color();
            octa::ui::message::selectable_message(ui, colour, &path);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .button(octa::i18n::t("dialog.file_changed_overwrite"))
                    .on_hover_text(octa::i18n::t("dialog.file_changed_overwrite_hint"))
                    .clicked()
                {
                    app.pending_overwrite_confirm = None;
                    // Accept what is on disk as the new baseline, so the save
                    // this triggers does not walk straight back into here.
                    if let Some(tab) = app.tabs.get_mut(tab_idx)
                        && let Some(p) = tab.table.source_path.clone()
                    {
                        tab.file_stamp = crate::app::file_io::file_stamp(std::path::Path::new(&p));
                    }
                    app.active_tab = tab_idx;
                    app.save_file();
                }
                if ui
                    .button(octa::i18n::t("dialog.file_changed_reload"))
                    .on_hover_text(octa::i18n::t("dialog.file_changed_reload_hint"))
                    .clicked()
                {
                    app.pending_overwrite_confirm = None;
                    app.active_tab = tab_idx;
                    app.reload_active_file();
                }
                if ui
                    .button(octa::i18n::t("common.cancel"))
                    .on_hover_text(octa::i18n::t("dialog.file_changed_cancel_hint"))
                    .clicked()
                {
                    app.pending_overwrite_confirm = None;
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
        app.pending_overwrite_confirm = None;
    }
}
