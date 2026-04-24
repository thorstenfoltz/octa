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
    egui::Window::new("Discard aligned edits?")
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(
                "Turning off 'Align Columns' reloads the file from disk.\n\
                 Unsaved changes in the raw view will be lost.",
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Reload and discard").clicked() {
                    confirm = true;
                }
                if ui.button("Keep aligned").clicked() {
                    cancel = true;
                }
                ui.add_space(12.0);
                ui.label(
                    RichText::new("(You can disable this warning in Settings → File-Specific.)")
                        .weak()
                        .size(11.0),
                );
            });
        });
    if confirm {
        let tab = &mut app.tabs[app.active_tab];
        if let (Some(content), Some(path)) =
            (tab.raw_content.as_mut(), tab.table.source_path.clone())
        {
            if let Ok(original) = std::fs::read_to_string(&path) {
                *content = original;
                tab.raw_content_modified = false;
                tab.raw_view_formatted = false;
            }
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
    egui::Window::new("Discard unsaved changes?")
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("Reloading will replace your current edits with the contents on disk.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Reload and discard").clicked() {
                    confirm = true;
                }
                if ui.button("Cancel").clicked() {
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
