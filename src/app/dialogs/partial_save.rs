//! "This file was loaded in part" confirmation, raised by a Save of a table
//! that holds only a window of its source.
//!
//! Octa stops reading at `initial_load_rows`, and Save writes `table.rows`
//! back over the source path. For a file that was capped at load, that means
//! writing the loaded rows over a file that had more - silently, with a
//! success message. The rows that were never read are simply gone.
//!
//! So the one save that can destroy data asks first, and offers the fix
//! (load the rest, then save) rather than only a way past the warning.

use eframe::egui;

use super::super::state::OctaApp;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

pub(crate) fn render_partial_save_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(tab_idx) = app.pending_partial_save_confirm else {
        return;
    };
    // The tab could have been closed while this was up; without the guard the
    // buttons below would act on whatever tab slid into that index.
    let Some(((loaded, known_total), path)) = app.tabs.get(tab_idx).and_then(|t| {
        Some((
            t.table.partial_note()?,
            t.table.source_path.clone().unwrap_or_default(),
        ))
    }) else {
        app.pending_partial_save_confirm = None;
        return;
    };
    let dialog_id = egui::Id::new("octa_partial_save_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(480.0, 260.0));
    let window = egui::Window::new("octa_partial_save")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(260.0)
            .min_width(320.0)
            .min_height(160.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("partial_save_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(octa::i18n::t("dialog.partial_save_title"))
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
            ui.label(octa::i18n::t("dialog.partial_save_body"));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(octa::ui::message::partial_note_text(loaded, known_total))
                    .color(octa::ui::message::PARTIAL_NOTE_COLOR),
            );
            ui.add_space(4.0);
            let colour = ui.visuals().weak_text_color();
            octa::ui::message::selectable_message(ui, colour, &path);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .button(octa::i18n::t("dialog.partial_save_load_all"))
                    .on_hover_text(octa::i18n::t("dialog.partial_save_load_all_hint"))
                    .clicked()
                {
                    app.pending_partial_save_confirm = None;
                    app.active_tab = tab_idx;
                    app.reload_without_row_cap();
                }
                if ui
                    .button(octa::i18n::t("dialog.partial_save_anyway"))
                    .on_hover_text(octa::i18n::t("dialog.partial_save_anyway_hint"))
                    .clicked()
                {
                    app.pending_partial_save_confirm = None;
                    app.partial_save_acknowledged = true;
                    app.active_tab = tab_idx;
                    app.save_file();
                }
                if ui
                    .button(octa::i18n::t("common.cancel"))
                    .on_hover_text(octa::i18n::t("dialog.file_changed_cancel_hint"))
                    .clicked()
                {
                    app.pending_partial_save_confirm = None;
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
        app.pending_partial_save_confirm = None;
    }
}
