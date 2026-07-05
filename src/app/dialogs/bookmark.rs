//! "Name this bookmark" dialog. Captures a name for a session bookmark at the
//! selection recorded when the dialog opened. Driven by `OctaApp.bookmark_draft`.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;

pub(crate) fn render_bookmark_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.bookmark_draft.is_none() {
        return;
    }
    let mut draft = app.bookmark_draft.take().unwrap();
    let mut close = false;
    let mut save = false;

    let dialog_id = egui::Id::new("octa_bookmark_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(draft.size));
    let minimized = size == DialogSize::Minimized;

    let position = match draft.col {
        Some(c) => format!("R{}:C{}", draft.row + 1, c + 1),
        None => format!("R{}", draft.row + 1),
    };

    let window = egui::Window::new("octa_bookmark")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(false).default_width(360.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("bookmark_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.bookmark_title"))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close = true;
                        }
                    });
                });
            });

        if minimized {
            return;
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(
                RichText::new(format!(
                    "{} {position}",
                    octa::i18n::t("dialog.bookmark_at")
                ))
                .size(11.0)
                .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            ui.label(octa::i18n::t("dialog.bookmark_name"));
            let resp = ui.add(
                egui::TextEdit::singleline(&mut draft.name_buf)
                    .desired_width(320.0)
                    .hint_text(octa::i18n::t("dialog.bookmark_name_hint")),
            );
            // Enter in the name box saves.
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can_save = !draft.name_buf.trim().is_empty();
                if ui
                    .add_enabled(can_save, egui::Button::new(octa::i18n::t("common.save")))
                    .clicked()
                    || (enter && can_save)
                {
                    save = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(octa::i18n::t("common.cancel")).clicked() {
                        close = true;
                    }
                });
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if close || save {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if save {
        let name = draft.name_buf.trim().to_string();
        app.commit_bookmark_draft(name, draft.row, draft.col);
        app.status_message = Some((octa::i18n::t("bookmarks.saved"), std::time::Instant::now()));
        return; // draft consumed
    }
    if !close {
        draft.size = size;
        app.bookmark_draft = Some(draft);
    }
}
