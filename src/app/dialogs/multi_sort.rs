//! Multi-column sort dialog. Sorts the active tab by an ordered list of
//! `(column, direction)` keys: the first key is the primary sort, later keys
//! break ties (see `DataTable::sort_rows_by_columns`). App-level state, sorts
//! in place (no new tab).

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{MultiSortState, OctaApp, SortKey};

pub(crate) fn render_multi_sort_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.multi_sort_dialog.is_none() {
        return;
    }

    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    // With no columns there is nothing to sort; close silently.
    if col_names.is_empty() {
        app.multi_sort_dialog = None;
        return;
    }

    let mut close = false;
    let mut run = false;
    let mut st = app.multi_sort_dialog.take().unwrap();
    // Clamp any stale column indices to the current table.
    for key in &mut st.keys {
        if key.col >= col_names.len() {
            key.col = 0;
        }
    }
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_multi_sort_dialog");
    let window = egui::Window::new("octa_multi_sort")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(320.0)
            .min_width(360.0)
            .min_height(180.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("multi_sort_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.msort_title"))
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

        egui::Panel::bottom("multi_sort_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let can_run = !st.keys.is_empty();
                    if ui
                        .add_enabled(can_run, egui::Button::new(octa::i18n::t("common.apply")))
                        .clicked()
                    {
                        run = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("common.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("dialog.msort_intro"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            sort_keys_body(ui, &mut st, &col_names);
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if run {
        let keys: Vec<(usize, bool)> = st.keys.iter().map(|k| (k.col, k.ascending)).collect();
        let tab = &mut app.tabs[app.active_tab];
        tab.table.sort_rows_by_columns(&keys);
        tab.filter_dirty = true;
        return; // dialog dropped
    }
    if !close {
        app.multi_sort_dialog = Some(st);
    }
}

/// The ordered list of sort keys, each a column dropdown + direction toggle,
/// with up/down reorder, remove, and an "add key" button.
fn sort_keys_body(ui: &mut egui::Ui, st: &mut MultiSortState, cols: &[String]) {
    let mut move_up: Option<usize> = None;
    let mut move_down: Option<usize> = None;
    let mut remove: Option<usize> = None;
    let key_count = st.keys.len();

    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .max_height(220.0)
        .show(ui, |ui| {
            for (i, key) in st.keys.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if i == 0 {
                            octa::i18n::t("dialog.msort_by")
                        } else {
                            octa::i18n::t("dialog.msort_then_by")
                        })
                        .size(11.0),
                    );
                    egui::ComboBox::from_id_salt(("msort_col", i))
                        .selected_text(cols.get(key.col).cloned().unwrap_or_default())
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for (c, name) in cols.iter().enumerate() {
                                ui.selectable_value(&mut key.col, c, name);
                            }
                        });
                    // Direction toggle.
                    let dir_label = if key.ascending {
                        octa::i18n::t("dialog.msort_asc")
                    } else {
                        octa::i18n::t("dialog.msort_desc")
                    };
                    if ui.button(dir_label).clicked() {
                        key.ascending = !key.ascending;
                    }
                    // Reorder + remove controls.
                    // ASCII caret glyphs (egui's bundled font has no arrows).
                    if ui
                        .add_enabled(i > 0, egui::Button::new("^"))
                        .on_hover_text(octa::i18n::t("dialog.msort_move_up"))
                        .clicked()
                    {
                        move_up = Some(i);
                    }
                    if ui
                        .add_enabled(i + 1 < key_count, egui::Button::new("v"))
                        .on_hover_text(octa::i18n::t("dialog.msort_move_down"))
                        .clicked()
                    {
                        move_down = Some(i);
                    }
                    if ui
                        .add_enabled(key_count > 1, egui::Button::new("x"))
                        .on_hover_text(octa::i18n::t("dialog.msort_remove"))
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
            }
        });

    if let Some(i) = move_up {
        st.keys.swap(i, i - 1);
    }
    if let Some(i) = move_down {
        st.keys.swap(i, i + 1);
    }
    if let Some(i) = remove {
        st.keys.remove(i);
    }

    ui.add_space(6.0);
    if ui.button(octa::i18n::t("dialog.msort_add")).clicked() {
        // Default the new key to the first column not already used, else col 0.
        let used: Vec<usize> = st.keys.iter().map(|k| k.col).collect();
        let next = (0..cols.len()).find(|c| !used.contains(c)).unwrap_or(0);
        st.keys.push(SortKey {
            col: next,
            ascending: true,
        });
    }
}
