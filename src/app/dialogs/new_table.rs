//! "New table" dialog (File -> New Table...). Asks for a column and row
//! count, then opens a blank editable grid of that shape in a new tab.
//! Driven by `OctaApp.new_table_dialog`.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;
use super::super::state::{NEW_TABLE_COLS, NEW_TABLE_ROWS};

pub(crate) fn render_new_table_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.new_table_dialog.is_none() {
        return;
    }
    let mut state = app.new_table_dialog.take().unwrap();
    let mut close = false;
    let mut create: Option<(usize, usize)> = None;

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
        if dialog_header(
            ui,
            "new_table_header",
            RichText::new(octa::i18n::t("new_table.title")),
            &mut size,
        ) {
            close = true;
        }

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
            // Typed, never dragged: a DragValue shows a left/right resize
            // arrow on hover.
            let bad = egui::Color32::from_rgb(220, 80, 80);
            octa::ui::control_row::control_grid(ui, "new_table_grid", |ui| {
                for (label, hint, buf, range) in [
                    (
                        "new_table.columns",
                        "new_table.columns_hint",
                        &mut state.cols_buf,
                        &NEW_TABLE_COLS,
                    ),
                    (
                        "new_table.rows",
                        "new_table.rows_hint",
                        &mut state.rows_buf,
                        &NEW_TABLE_ROWS,
                    ),
                ] {
                    ui.label(octa::i18n::t(label));
                    let ok = octa::ui::settings::parse_comma_number(buf)
                        .is_ok_and(|n| range.contains(&n));
                    let mut edit = egui::TextEdit::singleline(buf);
                    if !ok {
                        edit = edit.text_color(bad);
                    }
                    octa::ui::control_row::control_text_edit(ui, 90.0, edit)
                        .on_hover_text(octa::i18n::t(hint));
                    ui.end_row();
                }
            });

            ui.add_space(8.0);
            let shape = state.shape();
            octa::ui::control_row::control_line(ui, |ui| {
                if ui
                    .add_enabled(
                        shape.is_some(),
                        egui::Button::new(octa::i18n::t("new_table.create")),
                    )
                    .on_hover_text(octa::i18n::t("new_table.create_hint"))
                    .on_disabled_hover_text(octa::ui::menu_entry::disabled_tip(
                        &octa::i18n::t("new_table.create_hint"),
                        &octa::i18n::t("new_table.invalid_hint"),
                    ))
                    .clicked()
                {
                    create = shape;
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
            if close || create.is_some() {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if let Some((cols, rows)) = create {
        app.open_new_table_tab(cols, rows);
        return; // state consumed
    }
    if !close {
        state.size = size;
        app.new_table_dialog = Some(state);
    }
}
