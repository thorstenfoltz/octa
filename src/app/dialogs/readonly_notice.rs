//! Modal that announces a read-only-mode toggle. Contains a "Don't show
//! this again" checkbox that writes through to `AppSettings.show_readonly_notice`,
//! disabling future notices globally.

use eframe::egui;
use egui::RichText;

use super::super::state::OctaApp;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

pub(crate) fn render_readonly_notice_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(notice) = app.pending_readonly_notice.as_ref() else {
        return;
    };
    let is_active = notice.is_active;

    // Pull the persisted checkbox state out of the notice so the dialog
    // can mutate it across frames. Without this round-trip the box would
    // flicker - re-deriving the initial value from settings on every frame
    // overwrites the user's click in the same frame they made it.
    let mut suppress_future = notice.suppress_future;
    let mut close = false;

    let title = if is_active {
        octa::i18n::t("dialog.readonly_on_title")
    } else {
        octa::i18n::t("dialog.readonly_off_title")
    };
    let body = if is_active {
        octa::i18n::t("dialog.readonly_on_body")
    } else {
        octa::i18n::t("dialog.readonly_off_body")
    };

    let dialog_id = egui::Id::new("octa_readonly_notice_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(440.0, 260.0));
    let window = egui::Window::new("octa_readonly_notice")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(440.0)
            .default_height(260.0)
            .min_width(320.0)
            .min_height(160.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("readonly_notice_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(title.clone()).strong().size(16.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            chrome_close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.set_min_width(360.0);
            ui.label(body);
            ui.add_space(8.0);
            ui.checkbox(
                &mut suppress_future,
                octa::i18n::t("dialog.dont_show_again"),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(octa::i18n::t("dialog.readonly_hint"))
                    .weak()
                    .size(11.0),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(octa::i18n::t("common.ok")).clicked() {
                    close = true;
                }
            });
        });
    });
    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if chrome_close {
                DialogSize::Normal
            } else {
                size
            },
        )
    });
    if chrome_close {
        close = true;
    }

    // Mirror the checkbox change back into the notice so the next frame
    // shows the same checked/unchecked state the user clicked.
    if let Some(n) = app.pending_readonly_notice.as_mut() {
        n.suppress_future = suppress_future;
    }

    if close {
        app.settings.show_readonly_notice = !suppress_future;
        app.settings.save();
        app.pending_readonly_notice = None;
    }
}
