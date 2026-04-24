//! "Unsaved changes" confirmation dialogs — one for closing (whole app or
//! single tab) and one for opening a different file.

use eframe::egui;

use super::super::state::OctaApp;

pub(crate) fn render_close_confirm_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_close_confirm {
        return;
    }
    egui::Window::new("Unsaved Changes")
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("You have unsaved changes. What would you like to do?");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    app.show_close_confirm = false;
                    if let Some(tab_idx) = app.pending_close_tab {
                        app.save_tab(tab_idx);
                        app.close_tab(tab_idx);
                        app.pending_close_tab = None;
                    } else {
                        if app.tabs[app.active_tab].table.source_path.is_some() {
                            app.save_file();
                        } else {
                            app.save_file_as();
                        }
                        app.confirmed_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                if ui.button("Don't Save").clicked() {
                    app.show_close_confirm = false;
                    if let Some(tab_idx) = app.pending_close_tab {
                        app.close_tab(tab_idx);
                        app.pending_close_tab = None;
                    } else {
                        app.confirmed_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                if ui.button("Cancel").clicked() {
                    app.show_close_confirm = false;
                    app.pending_close_tab = None;
                }
            });
        });
}

pub(crate) fn render_open_confirm_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_open_confirm {
        return;
    }
    egui::Window::new("Unsaved Changes")
        .id(egui::Id::new("open_confirm"))
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("You have unsaved changes. What would you like to do?");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    app.show_open_confirm = false;
                    if app.tabs[app.active_tab].table.source_path.is_some() {
                        app.save_file();
                    } else {
                        app.save_file_as();
                    }
                    app.do_open_file_dialog();
                }
                if ui.button("Don't Save").clicked() {
                    app.show_open_confirm = false;
                    app.tabs[app.active_tab].table.clear_modified();
                    app.tabs[app.active_tab].raw_content_modified = false;
                    app.do_open_file_dialog();
                }
                if ui.button("Cancel").clicked() {
                    app.show_open_confirm = false;
                    app.pending_open_file = false;
                }
            });
        });
}
