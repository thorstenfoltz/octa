//! "Save chat prompt" dialog. Captures a name + description for the current
//! chat input and appends it to the persistent prompt library
//! (`src/app/chat_prompts.rs`). Driven by `OctaApp.chat_prompt_save`.
//! Mirrors `src/app/dialogs/sql_snippet.rs`.

use eframe::egui;
use egui::RichText;

use super::super::chat_prompts::ChatPrompt;
use super::super::state::OctaApp;

pub(crate) fn render_chat_prompt_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.chat_prompt_save.is_none() {
        return;
    }
    let mut draft = app.chat_prompt_save.take().unwrap();
    let mut close = false;
    let mut save = false;

    egui::Window::new(octa::i18n::t("dialog.prompt_title"))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .resizable(false)
        .collapsible(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            egui::Grid::new("chat_prompt_grid")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    ui.label(octa::i18n::t("dialog.prompt_name"));
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.name)
                            .desired_width(280.0)
                            .hint_text(octa::i18n::t("dialog.prompt_name_hint")),
                    );
                    ui.end_row();
                    ui.label(octa::i18n::t("dialog.prompt_desc"));
                    ui.add(
                        egui::TextEdit::multiline(&mut draft.description)
                            .desired_width(280.0)
                            .desired_rows(2),
                    );
                    ui.end_row();
                });

            ui.add_space(4.0);
            ui.label(
                RichText::new(octa::i18n::t("dialog.prompt_body"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut draft.text)
                            .desired_width(f32::INFINITY)
                            .desired_rows(4),
                    );
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can_save = !draft.name.trim().is_empty() && !draft.text.trim().is_empty();
                if ui
                    .add_enabled(can_save, egui::Button::new(octa::i18n::t("common.save")))
                    .clicked()
                {
                    save = true;
                }
                if !can_save {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.prompt_need_name"))
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
        // Replace an existing prompt with the same name, else append.
        app.chat_prompts.retain(|p| p.name != name);
        app.chat_prompts.push(ChatPrompt {
            name,
            description: draft.description.trim().to_string(),
            text: draft.text.clone(),
        });
        super::super::chat_prompts::save(&app.chat_prompts);
        app.status_message = Some((
            octa::i18n::t("chat.prompt_saved"),
            std::time::Instant::now(),
        ));
        return; // draft consumed
    }
    if !close {
        // Keep the dialog open with the edited draft.
        app.chat_prompt_save = Some(draft);
    }
}
