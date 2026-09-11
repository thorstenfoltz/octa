//! Confirmation modal for a DB save that changes the table schema. Mirrors the
//! deferred `round_save` flow: re-enters `do_save_tab_inner` with the decision.

use eframe::egui;

use super::super::state::OctaApp;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

pub(crate) fn render_schema_change_save_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(prompt) = app.pending_schema_change_save.clone() else {
        return;
    };
    let mut proceed = false;
    let mut cancel = false;
    // No close 'x' (no `.open`): forced-choice prompt dismissed by its own
    // Proceed / Cancel buttons, matching the other confirmation dialogs.
    let dialog_id = egui::Id::new("octa_schema_change_save_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(480.0, 300.0));
    let window = egui::Window::new("octa_schema_change_save")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(300.0)
            .min_width(340.0)
            .min_height(180.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("schema_change_save_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(octa::i18n::t("dialog.scs_title"))
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
            ui.label(octa::i18n::t("dialog.scs_intro"));
            ui.add_space(4.0);
            for line in &prompt.changes {
                ui.monospace(line);
            }
            ui.add_space(6.0);
            match &prompt.backup_note {
                Some(p) => {
                    ui.label(octa::i18n::t("dialog.scs_backup"));
                    ui.monospace(p);
                }
                None => {
                    ui.label(octa::i18n::t("dialog.scs_no_backup"));
                }
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(octa::i18n::t("dialog.scs_warn"))
                    .color(ui.visuals().warn_fg_color),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(octa::i18n::t("dialog.scs_proceed")).clicked() {
                    proceed = true;
                }
                if ui.button(octa::i18n::t("common.cancel")).clicked() {
                    cancel = true;
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
        cancel = true;
    }

    if proceed {
        app.pending_schema_change_save = None;
        app.do_save_tab_inner(
            prompt.tab_idx,
            prompt.path,
            prompt.save_filtered_view,
            prompt.round_decision,
            Some(true),
            prompt.style_decision,
        );
    } else if cancel {
        app.pending_schema_change_save = None;
        app.status_message = Some((
            octa::i18n::t("dialog.scs_cancelled"),
            std::time::Instant::now(),
        ));
    }
}
