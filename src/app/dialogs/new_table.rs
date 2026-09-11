//! "New table" dialog (File -> New Table...). Asks for a column and row
//! count, then opens a blank editable grid of that shape in a new tab.
//! Driven by `OctaApp.new_table_dialog`.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

use super::super::state::OctaApp;

pub(crate) fn render_new_table_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.new_table_dialog.is_none() {
        return;
    }
    let mut state = app.new_table_dialog.take().unwrap();
    let mut close = false;
    let mut create = false;

    let dialog_id = egui::Id::new("octa_new_table_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(state.size));
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(340.0, 200.0));
    let window = egui::Window::new("octa_new_table")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true).default_width(340.0).default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("new_table_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("new_table.title"))
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

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("new_table.hint"))
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            egui::Grid::new("new_table_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(octa::i18n::t("new_table.columns"));
                    ui.add(egui::DragValue::new(&mut state.cols).range(1..=1000))
                        .on_hover_text(octa::i18n::t("new_table.columns_hint"));
                    ui.end_row();
                    ui.label(octa::i18n::t("new_table.rows"));
                    ui.add(egui::DragValue::new(&mut state.rows).range(0..=100_000))
                        .on_hover_text(octa::i18n::t("new_table.rows_hint"));
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .button(octa::i18n::t("new_table.create"))
                    .on_hover_text(octa::i18n::t("new_table.create_hint"))
                    .clicked()
                {
                    create = true;
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
            if close || create {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if create {
        app.open_new_table_tab(state.cols, state.rows);
        return; // state consumed
    }
    if !close {
        state.size = size;
        app.new_table_dialog = Some(state);
    }
}
