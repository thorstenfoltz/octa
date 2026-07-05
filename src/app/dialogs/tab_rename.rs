//! "Rename tab" dialog. Sets a display-only name for a tab (the file path and
//! on-disk name are unchanged); clearing the field reverts to the file name.
//! Driven by `OctaApp.tab_rename_draft`.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;

pub(crate) fn render_tab_rename_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.tab_rename_draft.is_none() {
        return;
    }
    let mut draft = app.tab_rename_draft.take().unwrap();
    // If the target tab vanished (closed while the dialog was open), drop it.
    if draft.tab_index >= app.tabs.len() {
        return;
    }
    let mut close = false;
    let mut save = false;

    let dialog_id = egui::Id::new("octa_tab_rename_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(draft.size));
    let minimized = size == DialogSize::Minimized;

    let window = egui::Window::new("octa_tab_rename")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(false).default_width(360.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("tab_rename_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.tab_rename_title"))
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
            ui.label(octa::i18n::t("dialog.tab_rename_name"));
            let resp = ui.add(
                egui::TextEdit::singleline(&mut draft.name_buf)
                    .desired_width(320.0)
                    .hint_text(octa::i18n::t("dialog.tab_rename_hint")),
            );
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.label(
                RichText::new(octa::i18n::t("dialog.tab_rename_revert_hint"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(octa::i18n::t("common.save")).clicked() || enter {
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
        let idx = draft.tab_index;
        app.commit_tab_rename(idx, draft.name_buf.clone());
        // Reflect the change in the OS window title when the active tab was
        // renamed.
        if idx == app.active_tab {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(app.tabs[idx].title_display()));
        }
        return; // draft consumed
    }
    if !close {
        draft.size = size;
        app.tab_rename_draft = Some(draft);
    }
}
