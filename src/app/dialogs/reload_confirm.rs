//! Two small confirmation dialogs: "Reload from disk and discard edits?" and
//! the "discard aligned edits?" dialog that guards un-aligning the raw view.

use eframe::egui;
use egui::RichText;

use super::super::state::OctaApp;

pub(crate) fn render_unalign_confirm_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_unalign_confirm {
        return;
    }
    let mut confirm = false;
    let mut cancel = false;
    egui::Window::new(octa::i18n::t("dialog.unalign_title"))
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
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
    egui::Window::new(octa::i18n::t("dialog.reload_title"))
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
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
    if confirm {
        app.show_reload_confirm = false;
        app.reload_active_file();
    } else if cancel {
        app.show_reload_confirm = false;
    }
}
