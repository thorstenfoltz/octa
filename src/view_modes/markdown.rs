use crate::app::state::TabState;

use eframe::egui;
use egui::RichText;

/// Render the Markdown view using commonmark.
pub fn render_markdown_view(ui: &mut egui::Ui, tab: &mut TabState) {
    if let Some(ref content) = tab.raw_content {
        let md_content = content.clone();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        ui.set_max_width(900.0);
                        egui_commonmark::CommonMarkViewer::new().show(
                            ui,
                            &mut tab.commonmark_cache,
                            &md_content,
                        );
                    });
                });
            });
    } else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Markdown content not available")
                    .size(16.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
    }
}
