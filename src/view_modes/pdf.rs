use crate::TabState;
use crate::ui;

use eframe::egui;
use egui::{Color32, RichText, Stroke};
use ui::theme::ThemeMode;

/// Render the PDF page view. Returns early if there are no textures.
pub fn render_pdf_view(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    tab: &mut TabState,
    theme_mode: ThemeMode,
) {
    // Lazily create textures from rendered images
    if tab.pdf_textures.len() != tab.pdf_page_images.len() {
        tab.pdf_textures.clear();
        for (i, image) in tab.pdf_page_images.iter().enumerate() {
            let texture = ctx.load_texture(
                format!("pdf_page_{}", i),
                image.clone(),
                egui::TextureOptions::LINEAR,
            );
            tab.pdf_textures.push(texture);
        }
    }

    if tab.pdf_textures.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("No PDF pages to display")
                    .size(16.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
        return;
    }

    let colors = ui::theme::ThemeColors::for_mode(theme_mode);
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                let page_count = tab.pdf_textures.len();
                for (page_idx, texture) in tab.pdf_textures.iter().enumerate() {
                    let size = texture.size_vec2();
                    let page_text = tab
                        .pdf_page_texts
                        .get(page_idx)
                        .cloned()
                        .unwrap_or_default();
                    // Page header
                    ui.label(
                        RichText::new(format!("Page {} of {}", page_idx + 1, page_count))
                            .size(11.0)
                            .color(colors.text_muted),
                    );
                    ui.add_space(4.0);
                    // Rendered page image
                    egui::Frame::new()
                        .fill(Color32::WHITE)
                        .shadow(egui::epaint::Shadow {
                            offset: [2, 2],
                            blur: 8,
                            spread: 0,
                            color: colors.border.gamma_multiply(0.5),
                        })
                        .show(ui, |ui| {
                            ui.image(egui::load::SizedTexture::new(texture.id(), size));
                        });
                    // Selectable text below the page image
                    if !page_text.is_empty() {
                        ui.add_space(4.0);
                        egui::Frame::new()
                            .fill(colors.bg_secondary)
                            .stroke(Stroke::new(1.0, colors.border_subtle))
                            .corner_radius(4.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&page_text)
                                            .font(egui::FontId::new(
                                                12.0,
                                                egui::FontFamily::Monospace,
                                            ))
                                            .color(colors.text_primary),
                                    )
                                    .selectable(true),
                                );
                            });
                    }
                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(8.0);
                }
            });
        });
}
