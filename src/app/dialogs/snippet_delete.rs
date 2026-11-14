//! "Delete the snippet?" prompt. Every snippet delete goes through
//! [`OctaApp::request_snippet_delete`]; Settings > SQL turns the question off.

use eframe::egui;
use egui::RichText;

use super::super::state::OctaApp;
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

impl OctaApp {
    /// Asked first unless turned off in the settings.
    pub(crate) fn request_snippet_delete(&mut self, name: String) {
        if self.settings.confirm_snippet_delete {
            self.pending_snippet_delete = Some(name);
        } else {
            self.delete_snippet_now(&name);
        }
    }

    pub(crate) fn delete_snippet_now(&mut self, name: &str) {
        self.sql_snippets.retain(|s| s.name != name);
        crate::app::sql_snippets::save(&self.sql_snippets);
    }
}

pub(crate) fn render_snippet_delete_confirm(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(name) = app.pending_snippet_delete.clone() else {
        return;
    };
    let mut delete = false;
    let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let tick_id = egui::Id::new("octa_snippet_delete_dont_ask");
    let mut dont_ask = ctx.data(|d| d.get_temp::<bool>(tick_id).unwrap_or(false));
    let dialog_id = egui::Id::new("octa_snippet_delete_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(420.0, 160.0));
    let window = egui::Window::new("octa_snippet_delete")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(420.0)
            .default_height(160.0)
            .min_width(300.0)
            .min_height(120.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "snippet_delete_header",
            RichText::new(octa::i18n::t("sql.snippet_delete_yes")),
            &mut size,
        ) {
            chrome_close = true;
        }
        if minimized {
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(octa::i18n::t("sql.snippet_delete_ask").replace("{name}", &name));
            ui.add_space(8.0);
            octa::ui::control_row::control_row(ui, |ui| {
                if ui
                    .button(octa::i18n::t("sql.snippet_delete_yes"))
                    .on_hover_text(octa::i18n::t("sql.snippet_delete_yes_hint"))
                    .clicked()
                {
                    delete = true;
                }
                if ui
                    .button(octa::i18n::t("common.cancel"))
                    .on_hover_text(octa::i18n::t("sql.snippet_delete_cancel_hint"))
                    .clicked()
                {
                    cancel = true;
                }
                ui.checkbox(&mut dont_ask, octa::i18n::t("sql.snippet_delete_dont_ask"))
                    .on_hover_text(octa::i18n::t("sql.snippet_delete_dont_ask_hint"));
            });
        });
    });
    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    let done = chrome_close || cancel || delete;
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if chrome_close {
                DialogSize::Normal
            } else {
                size
            },
        );
        d.insert_temp(tick_id, dont_ask && !done);
    });
    if done {
        app.pending_snippet_delete = None;
    }
    if delete {
        if dont_ask {
            app.settings.confirm_snippet_delete = false;
            app.settings.save();
        }
        app.delete_snippet_now(&name);
    }
}

#[cfg(test)]
#[path = "snippet_delete_tests.rs"]
mod tests;
