//! "Refresh this tab or open a fresh copy?" Asked by Ctrl+R and the tab
//! menu's Refresh while Settings > Files > On refresh is "Ask each time", and
//! always when the tab holds unsaved changes.
//!
//! "Don't ask again" stores the button the user then clicks as the setting,
//! so the question goes away without a trip to Settings, and Settings is where
//! it comes back.

use eframe::egui;
use egui::RichText;

use super::super::state::OctaApp;
use octa::ui::settings::{
    DialogSize, RefreshBehaviour, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

pub(crate) fn render_refresh_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(pending) = app.pending_refresh else {
        return;
    };
    let Some(tab) = app.tabs.get(pending.tab) else {
        app.pending_refresh = None;
        return;
    };
    let modified = tab.is_modified();
    let title = tab.title_display();
    let mut dont_ask = pending.dont_ask;
    let mut choice: Option<bool> = None; // Some(in_place)
    let mut cancel = false;
    let dialog_id = egui::Id::new("octa_refresh_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(480.0, 220.0));
    let window = egui::Window::new("octa_refresh")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(220.0)
            .min_width(340.0)
            .min_height(150.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("refresh_dialog_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("refresh.title"))
                            .strong()
                            .size(16.0),
                    );
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
            ui.label(RichText::new(&title).strong());
            ui.label(octa::i18n::t("refresh.body"));
            if modified {
                let colors = octa::ui::theme::ThemeColors::for_mode(app.theme_mode);
                ui.label(RichText::new(octa::i18n::t("refresh.body_edits")).color(colors.warning));
            }
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                let in_place_label = if modified {
                    "refresh.in_place_discard"
                } else {
                    "refresh.in_place"
                };
                if ui
                    .button(octa::i18n::t(in_place_label))
                    .on_hover_text(octa::i18n::t("refresh.in_place_hint"))
                    .clicked()
                {
                    choice = Some(true);
                }
                if ui
                    .button(octa::i18n::t("refresh.new_tab"))
                    .on_hover_text(octa::i18n::t("refresh.new_tab_hint"))
                    .clicked()
                {
                    choice = Some(false);
                }
                if ui.button(octa::i18n::t("common.cancel")).clicked() {
                    cancel = true;
                }
            });
            ui.add_space(6.0);
            ui.checkbox(&mut dont_ask, octa::i18n::t("refresh.dont_ask"))
                .on_hover_text(octa::i18n::t("refresh.dont_ask_hint"));
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

    // Mirror the tick box so the next frame shows what the user clicked.
    if let Some(p) = app.pending_refresh.as_mut() {
        p.dont_ask = dont_ask;
    }
    if chrome_close || cancel {
        app.pending_refresh = None;
    } else if let Some(in_place) = choice {
        app.pending_refresh = None;
        if dont_ask {
            app.settings.refresh_behaviour = if in_place {
                RefreshBehaviour::InPlace
            } else {
                RefreshBehaviour::NewTab
            };
            app.settings.save();
        }
        app.refresh_tab(pending.tab, in_place, ctx);
    }
}
