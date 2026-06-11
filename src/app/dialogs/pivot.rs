//! Pivot / Unpivot dialog. Reshapes the active table between long and wide
//! form via DuckDB's `PIVOT` / `UNPIVOT`, dropping the result into a fresh
//! detached tab (same pattern as the Summary tab in `tabs.rs`).
//!
//! - **Pivot** (long -> wide): spread one column's distinct values into new
//!   columns, aggregating a value column, grouped by the chosen identity
//!   columns. `PIVOT data ON on USING agg(value) GROUP BY g...`.
//! - **Unpivot** (wide -> long): melt several columns into a name/value pair.
//!   `UNPIVOT data ON c... INTO NAME name VALUE value`.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{OctaApp, PivotAgg, PivotKind, PivotState, TabState};

pub(crate) fn render_pivot_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.pivot_dialog.is_none() {
        return;
    }

    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();

    let mut close = false;
    let mut run = false;
    // Work on a clone so the closure doesn't borrow `app` twice; written back
    // after the window closes.
    let mut st = app.pivot_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_pivot_dialog");
    let window = egui::Window::new("octa_pivot")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(440.0)
            .min_width(360.0)
            .min_height(200.0)
    });

    let inner = window.show(ctx, |ui| {
        // Header: title + window controls (minimize / maximize / close).
        egui::Panel::top("pivot_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.pv_title"))
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

        // Footer: Run / Cancel, pinned to the bottom so resizing grows the body.
        egui::Panel::bottom("pivot_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let can_run = match st.kind {
                        PivotKind::Pivot => st.on_col.is_some() && st.value_col.is_some(),
                        PivotKind::Unpivot => st.unpivot_cols.len() >= 2,
                    };
                    if ui
                        .add_enabled(can_run, egui::Button::new(octa::i18n::t("dialog.pv_run")))
                        .clicked()
                    {
                        run = true;
                    }
                    if !can_run {
                        ui.label(
                            RichText::new(octa::i18n::t("dialog.pv_need"))
                                .size(10.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("common.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        // Body in the central area. No outer scroll area: the only growable
        // part is the column picker, which has its own bounded scroll, so the
        // dialog never stretches to fill the screen.
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut st.kind,
                    PivotKind::Pivot,
                    octa::i18n::t("dialog.pv_pivot"),
                );
                ui.selectable_value(
                    &mut st.kind,
                    PivotKind::Unpivot,
                    octa::i18n::t("dialog.pv_unpivot"),
                );
            });
            ui.separator();
            match st.kind {
                PivotKind::Pivot => pivot_body(ui, &mut st, &col_names),
                PivotKind::Unpivot => unpivot_body(ui, &mut st, &col_names),
            }
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if run {
        execute_pivot(app, &st, &col_names);
        return; // dialog dropped (st not written back)
    }
    if !close {
        // Keep the dialog open with the edited state.
        app.pivot_dialog = Some(st);
    }
}

fn pivot_body(ui: &mut egui::Ui, st: &mut PivotState, cols: &[String]) {
    ui.label(
        RichText::new(octa::i18n::t("dialog.pv_pivot_desc"))
            .size(10.0)
            .color(ui.visuals().weak_text_color()),
    );
    ui.add_space(4.0);
    egui::Grid::new("pivot_grid").num_columns(2).show(ui, |ui| {
        ui.label(octa::i18n::t("dialog.pv_on"));
        col_combo(ui, "pv_on", &mut st.on_col, cols);
        ui.end_row();

        ui.label(octa::i18n::t("dialog.pv_aggregate"));
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("pv_agg")
                .selected_text(st.agg.sql_fn())
                .show_ui(ui, |ui| {
                    for &a in PivotAgg::ALL {
                        ui.selectable_value(&mut st.agg, a, a.sql_fn());
                    }
                });
            ui.label(octa::i18n::t("dialog.pv_of"));
            col_combo(ui, "pv_value", &mut st.value_col, cols);
        });
        ui.end_row();
    });

    ui.add_space(6.0);
    ui.label(RichText::new(octa::i18n::t("dialog.pv_group_by")).strong());
    ui.label(
        RichText::new(octa::i18n::t("dialog.pv_group_desc"))
            .size(10.0)
            .color(ui.visuals().weak_text_color()),
    );
    multi_col_picker(ui, "pv_group", &mut st.group_cols, cols);
}

fn unpivot_body(ui: &mut egui::Ui, st: &mut PivotState, cols: &[String]) {
    ui.label(
        RichText::new(octa::i18n::t("dialog.pv_unpivot_desc"))
            .size(10.0)
            .color(ui.visuals().weak_text_color()),
    );
    ui.add_space(4.0);
    ui.label(RichText::new(octa::i18n::t("dialog.pv_unpivot_cols")).strong());
    multi_col_picker(ui, "pv_unpivot", &mut st.unpivot_cols, cols);

    ui.add_space(6.0);
    egui::Grid::new("unpivot_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(octa::i18n::t("dialog.pv_name_col"));
            ui.text_edit_singleline(&mut st.name_col);
            ui.end_row();
            ui.label(octa::i18n::t("dialog.pv_value_col"));
            ui.text_edit_singleline(&mut st.value_name);
            ui.end_row();
        });
}

/// A single-column dropdown writing into `sel`.
fn col_combo(ui: &mut egui::Ui, id: &str, sel: &mut Option<usize>, cols: &[String]) {
    let text = sel
        .and_then(|i| cols.get(i).cloned())
        .unwrap_or_else(|| octa::i18n::t("dialog.pv_pick"));
    egui::ComboBox::from_id_salt(id)
        .selected_text(text)
        .width(180.0)
        .show_ui(ui, |ui| {
            for (i, name) in cols.iter().enumerate() {
                if ui.selectable_label(*sel == Some(i), name).clicked() {
                    *sel = Some(i);
                }
            }
        });
}

/// A checkbox picker writing into `sel` (preserving pick order). Checkboxes
/// wrap across the full available width (using the empty space beside a single
/// narrow column) and the area has a bounded height: it grows with the number
/// of columns up to a cap, then scrolls, so the dialog never stretches to the
/// bottom of the screen.
fn multi_col_picker(ui: &mut egui::Ui, id: &str, sel: &mut Vec<usize>, cols: &[String]) {
    // Frame the picker so the wrap region reads as one panel.
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id)
            // `auto_shrink([false, true])`: take the full width, but shrink to
            // the content height up to `max_height` (then scroll). This is the
            // bound that stops the list running to the end of the screen.
            .auto_shrink([false, true])
            .max_height(220.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, name) in cols.iter().enumerate() {
                        let mut on = sel.contains(&i);
                        if ui.checkbox(&mut on, name).changed() {
                            if on {
                                if !sel.contains(&i) {
                                    sel.push(i);
                                }
                            } else {
                                sel.retain(|c| *c != i);
                            }
                        }
                    }
                });
            });
    });
}

/// Quote a DuckDB identifier (double quotes, internal quotes doubled).
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Build the PIVOT / UNPIVOT SQL from the dialog state.
fn build_sql(st: &PivotState, cols: &[String]) -> Option<String> {
    let name = |i: usize| cols.get(i).map(|s| quote_ident(s));
    match st.kind {
        PivotKind::Pivot => {
            let on = name(st.on_col?)?;
            let value = name(st.value_col?)?;
            let agg = st.agg.sql_fn();
            let mut sql = format!("PIVOT data ON {on} USING {agg}({value})");
            if !st.group_cols.is_empty() {
                let groups: Vec<String> = st.group_cols.iter().filter_map(|&i| name(i)).collect();
                sql.push_str(&format!(" GROUP BY {}", groups.join(", ")));
            }
            Some(sql)
        }
        PivotKind::Unpivot => {
            if st.unpivot_cols.len() < 2 {
                return None;
            }
            let melt: Vec<String> = st.unpivot_cols.iter().filter_map(|&i| name(i)).collect();
            let name_col = quote_ident(st.name_col.trim());
            let value_col = quote_ident(st.value_name.trim());
            Some(format!(
                "UNPIVOT data ON {} INTO NAME {name_col} VALUE {value_col}",
                melt.join(", ")
            ))
        }
    }
}

fn execute_pivot(app: &mut OctaApp, st: &PivotState, cols: &[String]) {
    let Some(sql) = build_sql(st, cols) else {
        return;
    };
    let mut snap = app.tabs[app.active_tab].table.clone();
    snap.apply_edits();
    let source_label = app.tabs[app.active_tab]
        .table
        .source_path
        .as_ref()
        .and_then(|p| {
            std::path::Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| app.tabs[app.active_tab].title_display());

    match octa::sql::run_query(&snap, &sql) {
        Ok(outcome) => {
            let mut new_tab = TabState::new(app.settings.default_search_mode);
            new_tab.table = outcome.table;
            new_tab.table.source_path = None;
            new_tab.table.format_name = None;
            let verb = match st.kind {
                PivotKind::Pivot => octa::i18n::t("dialog.pv_pivot"),
                PivotKind::Unpivot => octa::i18n::t("dialog.pv_unpivot"),
            };
            new_tab.custom_tab_label = Some(format!("{verb} - {source_label}"));
            new_tab.filter_dirty = true;
            if new_tab.table.row_count() > 0 && new_tab.table.col_count() > 0 {
                new_tab.table_state.selected_cell = Some((0, 0));
            }
            app.tabs.push(new_tab);
            app.active_tab = app.tabs.len() - 1;
        }
        Err(e) => {
            app.status_message = Some((
                format!("{}: {e}", octa::i18n::t("dialog.pv_failed")),
                std::time::Instant::now(),
            ));
        }
    }
}
