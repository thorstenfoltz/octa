//! Simple "About Octa" dialog showing version, author, and repo link.

use eframe::egui;
use egui::RichText;

use super::super::state::OctaApp;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

pub(crate) fn render_about_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_about_dialog {
        return;
    }
    egui::Window::new("About Octa")
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("Octa").strong().size(20.0));
                ui.add_space(4.0);
                ui.label(format!("Version {}", VERSION));
                ui.add_space(8.0);
                ui.label(format!("Author: {}", AUTHORS));
                ui.add_space(4.0);
                if ui.hyperlink_to("GitHub Repository", REPOSITORY).clicked() {
                    // egui opens the link automatically
                }
                ui.add_space(12.0);
                if ui.button("Close").clicked() {
                    app.show_about_dialog = false;
                }
            });
        });
}
