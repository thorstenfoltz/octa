//! Detect-outliers dialog (Analyse -> Detect outliers...).
//!
//! The user picks numeric columns and a method (IQR fence or Z-score) plus the
//! `k` factor. Apply runs the pure engine [`octa::data::outliers::detect_outliers`]
//! and stores the flagged `(row, col)` cells in the active tab's session-only
//! `outlier_cells` set, which the table renderer paints orange (see
//! `src/ui/table_view/rows.rs`). It does not modify the data.

use eframe::egui;
use egui::RichText;

use octa::data::outliers::{OutlierMethod, detect_outliers};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{OctaApp, OutlierOutput, OutlierState};

pub(crate) fn render_outliers_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.outlier_dialog.is_none() {
        return;
    }

    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();

    if col_names.is_empty() {
        app.outlier_dialog = None;
        return;
    }

    let mut close = false;
    let mut run = false;
    let mut clear = false;
    let mut st = app.outlier_dialog.take().unwrap();

    // Re-seed the selection if the column count drifted while the dialog was open.
    if st.col_selected.len() != col_names.len() {
        st = OutlierState::for_table(&app.tabs[app.active_tab].table);
    }

    let flagged_count = app.tabs[app.active_tab].outlier_cells.len();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_outliers_dialog");
    let window = egui::Window::new("octa_outliers")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(420.0)
            .default_height(380.0)
            .min_width(320.0)
            .min_height(240.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("outliers_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("outliers.title"))
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

        egui::Panel::bottom("outliers_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("outliers.apply")).clicked() {
                        run = true;
                    }
                    if flagged_count > 0 && ui.button(octa::i18n::t("outliers.clear")).clicked() {
                        clear = true;
                    }
                    if flagged_count > 0 {
                        ui.label(
                            RichText::new(
                                octa::i18n::t("outliers.flagged_status")
                                    .replace("{n}", &flagged_count.to_string()),
                            )
                            .size(11.0)
                            .color(ui.visuals().weak_text_color()),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("outliers.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            // Method picker.
            ui.label(
                RichText::new(octa::i18n::t("outliers.method_label"))
                    .strong()
                    .size(13.0),
            );
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut st.method,
                    OutlierMethod::Iqr,
                    octa::i18n::t("outlier_method.iqr"),
                );
                ui.radio_value(
                    &mut st.method,
                    OutlierMethod::ZScore,
                    octa::i18n::t("outlier_method.zscore"),
                );
            });

            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("outliers.k_label"));
                if ui
                    .add(egui::TextEdit::singleline(&mut st.k_buf).desired_width(70.0))
                    .changed()
                {
                    st.error = None;
                }
            });
            ui.label(
                RichText::new(octa::i18n::t("outliers.k_hint"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );

            ui.add_space(6.0);
            ui.separator();

            // Output: highlight cells vs. add an is_outlier column.
            ui.label(
                RichText::new(octa::i18n::t("outliers.output_label"))
                    .strong()
                    .size(13.0),
            );
            ui.radio_value(
                &mut st.output,
                OutlierOutput::Highlight,
                octa::i18n::t("outliers.output_highlight"),
            );
            ui.radio_value(
                &mut st.output,
                OutlierOutput::NewColumn,
                octa::i18n::t("outliers.output_column"),
            );

            ui.add_space(6.0);
            ui.separator();

            // Column picker.
            ui.horizontal(|ui| {
                if ui.small_button(octa::i18n::t("dialog.sel_all")).clicked() {
                    for b in &mut st.col_selected {
                        *b = true;
                    }
                }
                if ui.small_button(octa::i18n::t("dialog.sel_none")).clicked() {
                    for b in &mut st.col_selected {
                        *b = false;
                    }
                }
                let selected_count = st.col_selected.iter().filter(|&&b| b).count();
                ui.label(
                    RichText::new(format!(
                        "{} {}",
                        selected_count,
                        octa::i18n::t("dialog.selected")
                    ))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
                );
            });

            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (idx, name) in col_names.iter().enumerate() {
                        if let Some(on) = st.col_selected.get_mut(idx) {
                            ui.checkbox(on, name);
                        }
                    }
                });

            if let Some(err) = &st.error {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(err)
                        .color(ui.visuals().error_fg_color)
                        .size(11.0),
                );
            }
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if clear {
        app.tabs[app.active_tab].outlier_cells.clear();
        ctx.request_repaint();
        return; // dialog dropped
    }
    if run {
        match apply_outliers(app, &st) {
            Ok(()) => {
                ctx.request_repaint();
                // Adding the column is a one-shot edit: close so the user can
                // see the result and we never append a second is_outlier
                // column. Highlight stays open for re-tuning k / clearing.
                if st.output == OutlierOutput::NewColumn {
                    return;
                }
                st.error = None;
            }
            Err(e) => st.error = Some(e),
        }
    }
    if !close {
        app.outlier_dialog = Some(st);
    }
}

/// Parse `k` (comma-tolerant), run the engine, and store the flagged cells.
fn apply_outliers(app: &mut OctaApp, st: &OutlierState) -> Result<(), String> {
    let k: f64 = st
        .k_buf
        .trim()
        .replace(',', ".")
        .parse()
        .map_err(|_| octa::i18n::t("outliers.bad_k"))?;
    if k.is_nan() || k <= 0.0 {
        return Err(octa::i18n::t("outliers.bad_k"));
    }

    let active = app.active_tab;
    // Merge pending cell edits so the engine sees the visible values.
    app.tabs[active].table.apply_edits();

    let cols: Vec<usize> = st
        .col_selected
        .iter()
        .enumerate()
        .filter_map(|(i, &on)| on.then_some(i))
        .collect();
    if cols.is_empty() {
        return Err(octa::i18n::t("outliers.no_columns"));
    }

    let flagged = detect_outliers(&app.tabs[active].table, &cols, st.method, k);
    let count = flagged.len();

    match st.output {
        OutlierOutput::Highlight => {
            app.tabs[active].outlier_cells = flagged;
            app.status_message = Some((
                octa::i18n::t("outliers.flagged_status").replace("{n}", &count.to_string()),
                std::time::Instant::now(),
            ));
        }
        OutlierOutput::NewColumn => {
            if app.is_readonly() {
                return Err(octa::i18n::t("outliers.title"));
            }
            // One boolean per row: true when the row has >=1 flagged cell.
            let rows = app.tabs[active].table.row_count();
            let flagged_rows: std::collections::HashSet<usize> =
                flagged.iter().map(|(r, _)| *r).collect();

            let tbl = &mut app.tabs[active].table;
            let new_col = tbl.col_count();
            let undo_start = tbl.undo_stack.len();
            tbl.insert_column(new_col, "is_outlier".to_string(), "Boolean".to_string());
            for r in 0..rows {
                tbl.set(
                    r,
                    new_col,
                    octa::data::CellValue::Bool(flagged_rows.contains(&r)),
                );
            }
            tbl.coalesce_undo_since(undo_start);

            // Clear any prior highlight so the two outputs don't stack.
            app.tabs[active].outlier_cells.clear();
            app.tabs[active].filter_dirty = true;
            app.tabs[active].table_state.widths_initialized = false;
            app.status_message = Some((
                octa::i18n::t("outliers.column_status")
                    .replace("{n}", &flagged_rows.len().to_string()),
                std::time::Instant::now(),
            ));
        }
    }
    Ok(())
}
