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

/// Cap on how many source rows the live preview runs against, so previewing a
/// pivot of a huge table stays instant.
const PIVOT_PREVIEW_SOURCE_ROWS: usize = 1000;
/// Cap on how many result rows the preview shows.
const PIVOT_PREVIEW_RESULT_ROWS: usize = 10;
/// Cap on how many result columns the preview shows (pivots can be very wide).
const PIVOT_PREVIEW_MAX_COLS: usize = 12;

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

    // Refresh the bounded preview only when the inputs changed (never per
    // frame, never against the full table).
    let key = preview_key(&st);
    if st.preview_key != key {
        st.preview = compute_preview(app, &st, &col_names);
        st.preview_key = key;
    }

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_pivot_dialog");
    let window = egui::Window::new("octa_pivot")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(560.0)
            .default_height(560.0)
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

            // Plain-language explanation + bounded live preview, so the user
            // can see what the reshape does without already knowing the concept.
            ui.add_space(6.0);
            ui.separator();
            ui.label(RichText::new(explain_text(&st, &col_names)).italics());
            ui.add_space(4.0);
            match &st.preview {
                Some(Ok(table)) => {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.pv_preview"))
                            .strong()
                            .size(11.0),
                    );
                    render_preview_grid(ui, table);
                }
                Some(Err(e)) => {
                    ui.label(
                        RichText::new(e)
                            .color(ui.visuals().error_fg_color)
                            .size(10.0),
                    );
                }
                None => {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.pv_preview_select"))
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                }
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

/// Build the PIVOT / UNPIVOT SQL from the dialog state. Resolves column
/// indices to names, then defers to the shared `octa::data::pivot` builders
/// (also used by the MCP `pivot` tool).
fn build_sql(st: &PivotState, cols: &[String]) -> Option<String> {
    let name = |i: usize| cols.get(i).cloned();
    match st.kind {
        PivotKind::Pivot => {
            let on = name(st.on_col?)?;
            let value = name(st.value_col?)?;
            let group: Vec<String> = st.group_cols.iter().filter_map(|&i| name(i)).collect();
            Some(octa::data::pivot::pivot_sql(&on, st.agg, &value, &group))
        }
        PivotKind::Unpivot => {
            let melt: Vec<String> = st.unpivot_cols.iter().filter_map(|&i| name(i)).collect();
            octa::data::pivot::unpivot_sql(&melt, &st.name_col, &st.value_name)
        }
    }
}

/// Plain-language sentence describing the configured reshape, resolving the
/// chosen column indices to names and deferring to the pure `octa::data::pivot`
/// explain helpers.
fn explain_text(st: &PivotState, cols: &[String]) -> String {
    match st.kind {
        PivotKind::Pivot => {
            let on = st
                .on_col
                .and_then(|i| cols.get(i))
                .cloned()
                .unwrap_or_default();
            let value = st
                .value_col
                .and_then(|i| cols.get(i))
                .cloned()
                .unwrap_or_default();
            let group: Vec<String> = st
                .group_cols
                .iter()
                .filter_map(|&i| cols.get(i).cloned())
                .collect();
            octa::data::pivot::explain_pivot(&on, st.agg, &value, &group)
        }
        PivotKind::Unpivot => {
            let melt: Vec<String> = st
                .unpivot_cols
                .iter()
                .filter_map(|&i| cols.get(i).cloned())
                .collect();
            octa::data::pivot::explain_unpivot(&melt, &st.name_col, &st.value_name)
        }
    }
}

/// Hash of the inputs the preview depends on, so it is recomputed only when one
/// of them changes.
fn preview_key(st: &PivotState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    matches!(st.kind, PivotKind::Pivot).hash(&mut h);
    st.on_col.hash(&mut h);
    st.value_col.hash(&mut h);
    st.agg.sql_fn().hash(&mut h);
    st.group_cols.hash(&mut h);
    st.unpivot_cols.hash(&mut h);
    st.name_col.hash(&mut h);
    st.value_name.hash(&mut h);
    h.finish()
}

/// Run the reshape on a capped sample of the active table and return the first
/// rows, for the dialog preview. `None` when the inputs aren't sufficient yet
/// (so no SQL runs); `Err` carries the failure text.
fn compute_preview(
    app: &OctaApp,
    st: &PivotState,
    cols: &[String],
) -> Option<Result<octa::data::DataTable, String>> {
    let sql = build_sql(st, cols)?;
    let mut snap = app.tabs[app.active_tab].table.clone();
    snap.apply_edits();
    snap.rows.truncate(PIVOT_PREVIEW_SOURCE_ROWS);
    snap.source_path = None;
    snap.total_rows = None;
    match octa::sql::run_query(&snap, &sql) {
        Ok(outcome) => {
            let mut table = outcome.table;
            table.rows.truncate(PIVOT_PREVIEW_RESULT_ROWS);
            Some(Ok(table))
        }
        Err(e) => Some(Err(e.to_string())),
    }
}

/// Render a small, bounded preview table (header + a few rows, capped columns).
fn render_preview_grid(ui: &mut egui::Ui, table: &octa::data::DataTable) {
    let ncols = table.col_count().min(PIVOT_PREVIEW_MAX_COLS);
    let more_cols = table.col_count() > PIVOT_PREVIEW_MAX_COLS;
    egui::ScrollArea::horizontal()
        .id_salt("pv_preview_scroll")
        .max_height(160.0)
        .show(ui, |ui| {
            egui::Grid::new("pv_preview_grid")
                .striped(true)
                .show(ui, |ui| {
                    for c in 0..ncols {
                        let name = table.columns.get(c).map(|c| c.name.as_str()).unwrap_or("");
                        ui.label(RichText::new(name).strong());
                    }
                    if more_cols {
                        ui.label(RichText::new("...").strong());
                    }
                    ui.end_row();
                    for r in 0..table.row_count() {
                        for c in 0..ncols {
                            let cell = table.get(r, c).map(|v| v.to_string()).unwrap_or_default();
                            let cell: String = cell.chars().take(30).collect();
                            ui.label(cell);
                        }
                        if more_cols {
                            ui.label("...");
                        }
                        ui.end_row();
                    }
                });
        });
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
