//! "Delete Columns" modal dialog: checkbox list with All/None buttons.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;

pub(crate) fn render_delete_columns_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.tabs[app.active_tab].show_delete_columns_dialog {
        return;
    }
    let mut should_delete = false;
    let mut close = false;
    // Keep the selection vec in sync when the table shape changes while the
    // dialog is open (rare, but possible via SQL mutations).
    let tab = &mut app.tabs[app.active_tab];
    if tab.delete_col_selection.len() != tab.table.col_count() {
        tab.delete_col_selection = vec![false; tab.table.col_count()];
    }

    let dialog_id = egui::Id::new("octa_delete_columns_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;

    let window = egui::Window::new("octa_delete_columns")
        .title_bar(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true).default_width(320.0).min_width(280.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("delete_columns_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.delete_columns_title"))
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

        let tab = &mut app.tabs[app.active_tab];
        let selected_count = tab.delete_col_selection.iter().filter(|&&v| v).count();

        egui::Panel::bottom("delete_columns_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let delete_btn = ui.add_enabled(
                        selected_count > 0,
                        egui::Button::new(format!(
                            "{} ({} {})",
                            octa::i18n::t("common.delete"),
                            selected_count,
                            octa::i18n::t("dialog.selected")
                        )),
                    );
                    if delete_btn.clicked() {
                        should_delete = true;
                    }
                    if ui.button(octa::i18n::t("common.cancel")).clicked() {
                        close = true;
                    }
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(octa::i18n::t("dialog.delete_columns_prompt"));
            ui.add_space(6.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (idx, col) in tab.table.columns.iter().enumerate() {
                    let mut checked = tab.delete_col_selection[idx];
                    let label = format!("{} [{}]", col.name, col.data_type);
                    if ui.checkbox(&mut checked, label).changed() {
                        tab.delete_col_selection[idx] = checked;
                    }
                }
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.small_button(octa::i18n::t("dialog.sel_all")).clicked() {
                    for v in &mut tab.delete_col_selection {
                        *v = true;
                    }
                }
                if ui.small_button(octa::i18n::t("dialog.sel_none")).clicked() {
                    for v in &mut tab.delete_col_selection {
                        *v = false;
                    }
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
            if close || should_delete {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if should_delete {
        let tab = &mut app.tabs[app.active_tab];
        // Delete in reverse order to keep indices valid
        let to_delete: Vec<usize> = tab
            .delete_col_selection
            .iter()
            .enumerate()
            .filter_map(|(i, &sel)| if sel { Some(i) } else { None })
            .rev()
            .collect();

        for col_idx in to_delete {
            tab.table.delete_column(col_idx);
        }

        tab.table_state.editing_cell = None;
        if tab.table.col_count() == 0 {
            tab.table_state.selected_cell = None;
        } else if let Some((row, col)) = tab.table_state.selected_cell {
            let new_col = col.min(tab.table.col_count() - 1);
            tab.table_state.selected_cell = Some((row, new_col));
        }
        tab.table_state.widths_initialized = false;
        tab.filter_dirty = true;
        tab.show_delete_columns_dialog = false;
    }

    if close {
        app.tabs[app.active_tab].show_delete_columns_dialog = false;
    }
}
