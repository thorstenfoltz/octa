//! SQL snippets manager window (standard min/max/close chrome): save the
//! active tab's query as a named snippet, insert a saved one into the editor,
//! or delete one. App-level because the snippet library is shared across tabs.
//! Replaces the old Snippets dropdown (which jumped around on click). Mirrors
//! the chat Prompts window in `src/app/chat_panel.rs`.

use eframe::egui;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;

pub(crate) fn render_sql_snippets_window(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.sql_snippets_window_open {
        return;
    }
    let mut size = app.sql_snippets_window_size;
    let mut close_requested = false;
    let mut save_snippet = false;
    let mut insert_snippet: Option<String> = None;
    let mut delete_snippet: Option<String> = None;
    let can_save = !app.tabs[app.active_tab].sql_query.trim().is_empty();

    let dialog_id = egui::Id::new("octa_sql_snippets_window_v1");
    let window = egui::Window::new(octa::i18n::t("sql.snippets"))
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(440.0)
            .default_height(440.0)
            .min_width(320.0)
            .min_height(180.0)
    });
    let minimized = size == DialogSize::Minimized;

    let inner = window.show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(octa::i18n::t("sql.snippets"))
                    .strong()
                    .size(15.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if draw_window_controls(ui, &mut size) {
                    close_requested = true;
                }
            });
        });
        if minimized {
            return;
        }
        ui.separator();

        if ui
            .add_enabled(
                can_save,
                egui::Button::new(octa::i18n::t("sql.snippet_save")),
            )
            .on_hover_text(octa::i18n::t("sql.snippets_hint"))
            .clicked()
        {
            save_snippet = true;
        }
        ui.separator();

        if app.sql_snippets.is_empty() {
            ui.label(
                egui::RichText::new(octa::i18n::t("sql.snippet_none"))
                    .weak()
                    .size(12.0),
            );
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for snip in &app.sql_snippets {
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("x")
                                .on_hover_text(octa::i18n::t("sql.snippet_delete"))
                                .clicked()
                            {
                                delete_snippet = Some(snip.name.clone());
                            }
                            if ui.button(octa::i18n::t("sql.snippet_insert")).clicked() {
                                insert_snippet = Some(snip.query.clone());
                            }
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    let hover = if snip.description.is_empty() {
                                        snip.query.clone()
                                    } else {
                                        format!("{}\n\n{}", snip.description, snip.query)
                                    };
                                    ui.add(
                                        egui::Label::new(egui::RichText::new(&snip.name).strong())
                                            .truncate(),
                                    )
                                    .on_hover_text(hover);
                                },
                            );
                        });
                    });
                    if !snip.description.is_empty() {
                        ui.label(egui::RichText::new(&snip.description).weak().size(11.0));
                    }
                    ui.separator();
                }
            });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    app.sql_snippets_window_size = size;

    if save_snippet {
        let query = app.tabs[app.active_tab].sql_query.trim().to_string();
        if !query.is_empty() {
            app.sql_snippet_save = Some(super::super::state::SqlSnippetDraft {
                name: String::new(),
                description: String::new(),
                query,
            });
        }
    }
    if let Some(q) = insert_snippet {
        app.tabs[app.active_tab].sql_query = q;
        app.tabs[app.active_tab].sql_editor_focus_pending = true;
    }
    if let Some(name) = delete_snippet {
        app.sql_snippets.retain(|s| s.name != name);
        super::super::sql_snippets::save(&app.sql_snippets);
    }
    if close_requested {
        app.sql_snippets_window_open = false;
    }
}
