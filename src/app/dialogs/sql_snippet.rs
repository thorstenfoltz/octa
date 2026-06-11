//! "Save SQL snippet" dialog. Captures a name + description for the current
//! SQL query and appends it to the persistent snippet library
//! (`src/app/sql_snippets.rs`). Driven by `OctaApp.sql_snippet_save`.

use eframe::egui;
use egui::RichText;

use super::super::sql_snippets::SqlSnippet;
use super::super::state::OctaApp;

pub(crate) fn render_sql_snippet_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.sql_snippet_save.is_none() {
        return;
    }
    let mut draft = app.sql_snippet_save.take().unwrap();
    let mut close = false;
    let mut save = false;

    egui::Window::new(octa::i18n::t("dialog.snip_title"))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .resizable(false)
        .collapsible(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            egui::Grid::new("snip_grid")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    ui.label(octa::i18n::t("dialog.snip_name"));
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.name)
                            .desired_width(280.0)
                            .hint_text(octa::i18n::t("dialog.snip_name_hint")),
                    );
                    ui.end_row();
                    ui.label(octa::i18n::t("dialog.snip_desc"));
                    ui.add(
                        egui::TextEdit::multiline(&mut draft.description)
                            .desired_width(280.0)
                            .desired_rows(2),
                    );
                    ui.end_row();
                });

            ui.add_space(4.0);
            ui.label(
                RichText::new(octa::i18n::t("dialog.snip_query"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            egui::ScrollArea::vertical()
                .max_height(120.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut draft.query.as_str())
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace),
                    );
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can_save = !draft.name.trim().is_empty();
                if ui
                    .add_enabled(can_save, egui::Button::new(octa::i18n::t("common.save")))
                    .clicked()
                {
                    save = true;
                }
                if !can_save {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.snip_need_name"))
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(octa::i18n::t("common.cancel")).clicked() {
                        close = true;
                    }
                });
            });
        });

    if save {
        let name = draft.name.trim().to_string();
        // Replace an existing snippet with the same name, else append.
        app.sql_snippets.retain(|s| s.name != name);
        app.sql_snippets.push(SqlSnippet {
            name,
            description: draft.description.trim().to_string(),
            query: draft.query.clone(),
        });
        super::super::sql_snippets::save(&app.sql_snippets);
        app.status_message = Some((
            octa::i18n::t("sql.snippet_saved"),
            std::time::Instant::now(),
        ));
        return; // draft consumed
    }
    if !close {
        // Keep the dialog open with the edited draft.
        app.sql_snippet_save = Some(draft);
    }
}
