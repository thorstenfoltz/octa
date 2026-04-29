//! Simple "About Octa" dialog showing version, author, and repo link.

use eframe::egui;
use egui::RichText;

use super::super::state::OctaApp;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// Easter egg: clicking the "Octa" title eight times (one per tentacle)
/// reveals a hidden line. Counter is kept in egui's transient memory store
/// keyed by this id, so it survives frames but resets on app restart.
const TENTACLE_CLICK_ID: &str = "about_dialog_tentacle_clicks";

pub(crate) fn render_about_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_about_dialog {
        return;
    }
    let screen_center = ctx.screen_rect().center();
    let default_pos = screen_center - egui::vec2(160.0, 100.0);
    egui::Window::new("About Octa")
        .resizable(false)
        .collapsible(false)
        .default_pos(default_pos)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                let title_id = egui::Id::new(TENTACLE_CLICK_ID);
                let mut clicks: u8 = ui.data(|d| d.get_temp::<u8>(title_id).unwrap_or(0));
                let title = ui
                    .add(
                        egui::Label::new(RichText::new("Octa").strong().size(20.0))
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if title.clicked() {
                    clicks = clicks.saturating_add(1);
                    ui.data_mut(|d| d.insert_temp(title_id, clicks));
                }
                ui.add_space(4.0);
                ui.label(format!("Version {}", VERSION));
                ui.add_space(8.0);
                ui.label(format!("Author: {}", AUTHORS));
                ui.add_space(4.0);
                if ui.hyperlink_to("GitHub Repository", REPOSITORY).clicked() {
                    // egui opens the link automatically
                }
                if clicks >= 8 {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "\u{1f419} Eight tentacles, eight clicks. \
                             You found the kraken's lair.",
                        )
                        .italics()
                        .size(11.0),
                    );
                }
                ui.add_space(12.0);
                if ui.button("Close").clicked() {
                    app.show_about_dialog = false;
                    ui.data_mut(|d| d.remove::<u8>(title_id));
                }
            });
        });
}
