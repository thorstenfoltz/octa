//! "File changed on disk" confirmation. Raised by an in-place Save when the
//! source file's modification time or size no longer matches what the tab
//! recorded when it read (or last wrote) the file.
//!
//! Save As is deliberately not guarded: the user picked that path in a file
//! dialog moments ago, and the picker asks about overwriting itself.

use eframe::egui;

use super::super::state::OctaApp;

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
    egui::Window::new(octa::i18n::t("dialog.file_changed_title"))
        .id(egui::Id::new("file_changed_confirm"))
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
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
}
