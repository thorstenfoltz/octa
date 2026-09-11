//! Two small confirmation dialogs: "Reload from disk and discard edits?" and
//! the "discard aligned edits?" dialog that guards un-aligning the raw view.

use eframe::egui;
use egui::RichText;

use super::super::state::OctaApp;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

pub(crate) fn render_unalign_confirm_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_unalign_confirm {
        return;
    }
    let mut confirm = false;
    let mut cancel = false;
    let dialog_id = egui::Id::new("octa_unalign_confirm_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(460.0, 240.0));
    let window = egui::Window::new("octa_unalign_confirm")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(240.0)
            .min_width(320.0)
            .min_height(150.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("unalign_confirm_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(octa::i18n::t("dialog.unalign_title"))
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
            ui.label(octa::i18n::t("dialog.unalign_body"));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .button(octa::i18n::t("dialog.reload_and_discard"))
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button(octa::i18n::t("dialog.keep_aligned")).clicked() {
                    cancel = true;
                }
                ui.add_space(12.0);
                ui.label(
                    RichText::new(octa::i18n::t("dialog.unalign_hint"))
                        .weak()
                        .size(11.0),
                );
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
    if confirm {
        let tab = &mut app.tabs[app.active_tab];
        if let (Some(original), Some(content)) =
            (tab.raw_content_original.clone(), tab.raw_content.as_mut())
        {
            *content = original;
            tab.raw_content_modified = false;
            tab.raw_view_formatted = false;
        }
        app.show_unalign_confirm = false;
    } else if cancel {
        app.show_unalign_confirm = false;
    }
}

pub(crate) fn render_reload_confirm_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_reload_confirm {
        return;
    }
    let mut confirm = false;
    let mut cancel = false;
    let dialog_id = egui::Id::new("octa_reload_confirm_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(460.0, 240.0));
    let window = egui::Window::new("octa_reload_confirm")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(240.0)
            .min_width(320.0)
            .min_height(150.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("reload_confirm_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(octa::i18n::t("dialog.reload_title"))
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
            ui.label(octa::i18n::t("dialog.reload_body"));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .button(octa::i18n::t("dialog.reload_and_discard"))
                    .clicked()
                {
                    confirm = true;
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
    if confirm {
        app.show_reload_confirm = false;
        app.reload_active_file();
    } else if cancel {
        app.show_reload_confirm = false;
    }
}
