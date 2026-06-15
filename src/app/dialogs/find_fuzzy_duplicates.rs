//! Find-near-duplicates dialog (Search -> Find near-duplicates...). Pick the
//! columns to compare, a method, a threshold, normalisation toggles, an
//! optional blocking column, and a row cap; **Find** runs the O(n^2) scan on a
//! background thread (mirroring the multi-search worker) with a **Cancel**
//! button. When the worker finishes, the chosen output is applied once: either
//! orange row highlights on the active table, or a clustered report in a new
//! tab.
//!
//! The scan itself is the pure [`octa::data::fuzzy_duplicates::find_fuzzy_duplicates`].

use std::sync::atomic::Ordering;

use eframe::egui;
use egui::RichText;

use octa::data::fuzzy_duplicates::{
    FuzzyDupConfig, FuzzyResult, SimilarityMethod, find_fuzzy_duplicates,
};
use octa::data::{CellValue, ColumnInfo, DataTable, MarkColor, MarkKey};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{FuzzyDuplicatesState, OctaApp, TabState};

pub(crate) fn render_find_fuzzy_duplicates_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.fuzzy_duplicates_dialog.is_none() {
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
    let mut cancel = false;
    let mut st = app.fuzzy_duplicates_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;
    let is_running = st.running.load(Ordering::Relaxed);

    let dialog_id = egui::Id::new("octa_fuzzy_duplicates_dialog");
    let window = egui::Window::new("octa_fuzzy_duplicates")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(560.0)
            .default_height(520.0)
            .min_width(440.0)
            .min_height(300.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("fuzzy_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("fuzzy_dup.title"))
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

        egui::Panel::bottom("fuzzy_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if is_running {
                        ui.add(egui::Spinner::new());
                        ui.label(octa::i18n::t("fuzzy_dup.running"));
                        if ui.button(octa::i18n::t("fuzzy_dup.cancel")).clicked() {
                            cancel = true;
                        }
                    } else {
                        let can_run = !st.key_cols.is_empty()
                            && (st.out_cluster_col || st.out_highlight || st.out_new_tab);
                        if ui
                            .add_enabled(can_run, egui::Button::new(octa::i18n::t("fuzzy_dup.run")))
                            .clicked()
                        {
                            run = true;
                        }
                        if !can_run {
                            ui.label(
                                RichText::new(octa::i18n::t("fuzzy_dup.select_one"))
                                    .size(10.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("common.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("fuzzy_dup.desc"))
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(6.0);
                    controls(ui, &mut st, &col_names);

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
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if cancel {
        st.cancel.store(true, Ordering::Relaxed);
    }
    if run {
        start_scan(app, &mut st, &col_names);
    }

    // Keep repainting while the worker runs so the spinner animates and we
    // notice completion promptly.
    if st.running.load(Ordering::Relaxed) {
        ctx.request_repaint();
    } else if st.handle.is_some() {
        // Worker just finished: join it, then apply the output once.
        if let Some(h) = st.handle.take() {
            let _ = h.join();
        }
        if !st.applied {
            apply_output(app, &mut st, &col_names);
            st.applied = true;
        }
    }

    if close {
        st.cancel.store(true, Ordering::Relaxed);
        if let Some(h) = st.handle.take() {
            let _ = h.join();
        }
        app.fuzzy_duplicates_dialog = None;
        return;
    }
    app.fuzzy_duplicates_dialog = Some(st);
}

/// The picker controls (column checklist, method, threshold, normalisation,
/// blocking, row cap, output mode).
fn controls(ui: &mut egui::Ui, st: &mut FuzzyDuplicatesState, cols: &[String]) {
    ui.label(RichText::new(octa::i18n::t("fuzzy_dup.key_columns")).strong())
        .on_hover_text(octa::i18n::t("fuzzy_dup.key_columns_help"));
    ui.label(
        RichText::new(octa::i18n::t("fuzzy_dup.key_columns_help"))
            .size(10.0)
            .color(ui.visuals().weak_text_color()),
    );
    egui::ScrollArea::vertical()
        .id_salt("fuzzy_cols")
        .auto_shrink([false, true])
        .max_height(140.0)
        .show(ui, |ui| {
            for (idx, name) in cols.iter().enumerate() {
                let mut on = st.key_cols.contains(&idx);
                if ui.checkbox(&mut on, name).changed() {
                    if on {
                        st.key_cols.insert(idx);
                    } else {
                        st.key_cols.remove(&idx);
                    }
                }
            }
        });
    ui.separator();

    ui.horizontal(|ui| {
        ui.label(octa::i18n::t("fuzzy_dup.method"));
        egui::ComboBox::from_id_salt("fuzzy_method")
            .selected_text(method_label(st.method))
            .show_ui(ui, |ui| {
                for m in [
                    SimilarityMethod::EditRatio,
                    SimilarityMethod::JaroWinkler,
                    SimilarityMethod::TokenSet,
                ] {
                    if ui
                        .selectable_label(st.method == m, method_label(m))
                        .on_hover_text(method_hint(m))
                        .clicked()
                    {
                        st.method = m;
                    }
                }
            })
            .response
            .on_hover_text(method_hint(st.method));
    });

    ui.horizontal(|ui| {
        ui.label(octa::i18n::t("fuzzy_dup.threshold"))
            .on_hover_text(octa::i18n::t("fuzzy_dup.threshold_help"));
        // No DragValue value box (it shows a horizontal-resize cursor); render
        // a plain slider plus a percent label instead.
        ui.add(egui::Slider::new(&mut st.threshold_pct, 50.0..=100.0).show_value(false))
            .on_hover_text(octa::i18n::t("fuzzy_dup.threshold_help"));
        ui.label(format!("{:.0}%", st.threshold_pct));
    });
    ui.label(
        RichText::new(octa::i18n::t("fuzzy_dup.threshold_help"))
            .size(10.0)
            .color(ui.visuals().weak_text_color()),
    );

    ui.label(octa::i18n::t("fuzzy_dup.normalize"));
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut st.normalize.lower,
            octa::i18n::t("fuzzy_dup.norm_lower"),
        );
        ui.checkbox(
            &mut st.normalize.collapse_ws,
            octa::i18n::t("fuzzy_dup.norm_ws"),
        );
        ui.checkbox(
            &mut st.normalize.strip_punct,
            octa::i18n::t("fuzzy_dup.norm_punct"),
        );
    });

    ui.horizontal(|ui| {
        ui.label(octa::i18n::t("fuzzy_dup.block_on"))
            .on_hover_text(octa::i18n::t("fuzzy_dup.block_help"));
        let label = st
            .block_col
            .and_then(|c| cols.get(c).cloned())
            .unwrap_or_else(|| octa::i18n::t("fuzzy_dup.block_none"));
        egui::ComboBox::from_id_salt("fuzzy_block")
            .selected_text(label)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(
                        st.block_col.is_none(),
                        octa::i18n::t("fuzzy_dup.block_none"),
                    )
                    .clicked()
                {
                    st.block_col = None;
                }
                for (c, name) in cols.iter().enumerate() {
                    if ui.selectable_label(st.block_col == Some(c), name).clicked() {
                        st.block_col = Some(c);
                    }
                }
            })
            .response
            .on_hover_text(octa::i18n::t("fuzzy_dup.block_help"));
    });
    ui.label(
        RichText::new(octa::i18n::t("fuzzy_dup.block_help"))
            .size(10.0)
            .color(ui.visuals().weak_text_color()),
    );

    ui.horizontal(|ui| {
        ui.label(octa::i18n::t("fuzzy_dup.max_rows"));
        ui.add(egui::TextEdit::singleline(&mut st.max_rows_text).desired_width(80.0));
    });

    ui.separator();
    ui.label(RichText::new(octa::i18n::t("fuzzy_dup.output")).strong());
    ui.checkbox(
        &mut st.out_cluster_col,
        octa::i18n::t("fuzzy_dup.out_cluster_col"),
    );
    ui.checkbox(&mut st.out_highlight, octa::i18n::t("fuzzy_dup.highlight"));
    ui.checkbox(&mut st.out_new_tab, octa::i18n::t("fuzzy_dup.new_tab"));
}

fn method_label(m: SimilarityMethod) -> String {
    match m {
        SimilarityMethod::EditRatio => octa::i18n::t("similarity_method.edit_ratio"),
        SimilarityMethod::JaroWinkler => octa::i18n::t("similarity_method.jaro_winkler"),
        SimilarityMethod::TokenSet => octa::i18n::t("similarity_method.token_set"),
    }
}

fn method_hint(m: SimilarityMethod) -> String {
    match m {
        SimilarityMethod::EditRatio => octa::i18n::t("fuzzy_dup.method_edit_hint"),
        SimilarityMethod::JaroWinkler => octa::i18n::t("fuzzy_dup.method_jw_hint"),
        SimilarityMethod::TokenSet => octa::i18n::t("fuzzy_dup.method_token_hint"),
    }
}

/// Spawn the worker thread on a clone of the active table.
fn start_scan(app: &mut OctaApp, st: &mut FuzzyDuplicatesState, _cols: &[String]) {
    st.error = None;
    st.applied = false;
    if let Ok(mut slot) = st.result.lock() {
        *slot = None;
    }
    st.cancel.store(false, Ordering::Relaxed);
    st.running.store(true, Ordering::Relaxed);

    let max_rows = st
        .max_rows_text
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse::<usize>()
        .unwrap_or(20_000)
        .max(2);

    let cfg = FuzzyDupConfig {
        key_cols: st.key_cols.iter().copied().collect(),
        method: st.method,
        threshold: st.threshold_pct / 100.0,
        normalize: st.normalize,
        block_col: st.block_col,
        max_rows,
    };
    let table = app.tabs[app.active_tab].table.clone();
    let result = st.result.clone();
    let running = st.running.clone();
    let cancel = st.cancel.clone();

    let handle = std::thread::spawn(move || {
        let res = find_fuzzy_duplicates(&table, &cfg, &cancel);
        if let Ok(mut slot) = result.lock() {
            *slot = Some(res);
        }
        running.store(false, Ordering::Relaxed);
    });
    st.handle = Some(handle);
}

/// Apply the finished scan's outputs (cluster column / highlight / new tab) and
/// report. Any combination of the three output checkboxes may be set.
fn apply_output(app: &mut OctaApp, st: &mut FuzzyDuplicatesState, col_names: &[String]) {
    let res: Option<FuzzyResult> = st.result.lock().ok().and_then(|mut s| s.take());
    let Some(res) = res else {
        return;
    };

    let mut summary = if res.clusters.is_empty() {
        octa::i18n::t("fuzzy_dup.no_results")
    } else {
        octa::i18n::t("fuzzy_dup.found").replace("{n}", &res.clusters.len().to_string())
    };
    if res.capped {
        summary.push(' ');
        summary.push_str(
            &octa::i18n::t("fuzzy_dup.capped_note").replace("{n}", &res.compared_rows.to_string()),
        );
    }

    let active = app.active_tab;

    // Highlight: clear the previous run's rows first (not the user's marks).
    if st.out_highlight {
        let prev = std::mem::take(&mut st.last_highlight_rows);
        for r in prev {
            app.tabs[active].table.marks.remove(&MarkKey::Row(r));
        }
        let mut marked = Vec::new();
        for cluster in &res.clusters {
            for &row in &cluster.rows {
                app.tabs[active]
                    .table
                    .set_mark(MarkKey::Row(row), MarkColor::Orange);
                marked.push(row);
            }
        }
        st.last_highlight_rows = marked;
    }

    // cluster_id (+ score) columns on the active table, one undo step.
    if st.out_cluster_col && !res.clusters.is_empty() {
        add_cluster_columns(&mut app.tabs[active].table, &res);
        app.tabs[active].table_state.widths_initialized = false;
    }

    if st.out_new_tab && !res.clusters.is_empty() {
        let report = build_cluster_report(&app.tabs[active].table, &res, col_names);
        let mut new_tab = TabState::new(app.settings.default_search_mode);
        new_tab.table = report;
        new_tab.filter_dirty = true;
        app.tabs.push(new_tab);
        app.active_tab = app.tabs.len() - 1;
    }

    app.tabs[app.active_tab].filter_dirty = true;
    app.status_message = Some((summary, std::time::Instant::now()));
}

/// Append `cluster_id` (Int) and `cluster_score` (text %) columns mapping each
/// clustered row to its 1-based cluster id; non-clustered rows get Null. One
/// undo step.
fn add_cluster_columns(tbl: &mut DataTable, res: &FuzzyResult) {
    let n = tbl.row_count();
    let mut id_for = vec![None::<i64>; n];
    let mut score_for = vec![None::<String>; n];
    for (i, c) in res.clusters.iter().enumerate() {
        for &r in &c.rows {
            if r < n {
                id_for[r] = Some(i as i64 + 1);
                score_for[r] = Some(format!("{:.0}%", c.score * 100.0));
            }
        }
    }
    let start = tbl.undo_stack.len();
    let id_idx = tbl.col_count();
    tbl.insert_column(id_idx, unique_col(tbl, "cluster_id"), "Int64".into());
    for (r, v) in id_for.into_iter().enumerate() {
        tbl.set(r, id_idx, v.map(CellValue::Int).unwrap_or(CellValue::Null));
    }
    let score_idx = tbl.col_count();
    tbl.insert_column(score_idx, unique_col(tbl, "cluster_score"), "Utf8".into());
    for (r, v) in score_for.into_iter().enumerate() {
        tbl.set(
            r,
            score_idx,
            v.map(CellValue::String).unwrap_or(CellValue::Null),
        );
    }
    tbl.coalesce_undo_since(start);
}

/// A column name not already present, appending `_2`, `_3`, ... if needed.
fn unique_col(tbl: &DataTable, base: &str) -> String {
    let mut name = base.to_string();
    let mut k = 2;
    while tbl.columns.iter().any(|c| c.name == name) {
        name = format!("{base}_{k}");
        k += 1;
    }
    name
}

/// Build the clustered-report table: leading `cluster` + `score` columns, then
/// the original columns, rows ordered by cluster id.
fn build_cluster_report(src: &DataTable, res: &FuzzyResult, col_names: &[String]) -> DataTable {
    let mut columns = Vec::with_capacity(col_names.len() + 2);
    columns.push(ColumnInfo {
        name: octa::i18n::t("fuzzy_dup.col_cluster"),
        data_type: "Int64".to_string(),
    });
    columns.push(ColumnInfo {
        name: octa::i18n::t("fuzzy_dup.col_score"),
        data_type: "Utf8".to_string(),
    });
    for c in &src.columns {
        columns.push(c.clone());
    }

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    for (cid, cluster) in res.clusters.iter().enumerate() {
        let score_pct = format!("{:.0}%", cluster.score * 100.0);
        for &r in &cluster.rows {
            let mut row = Vec::with_capacity(columns.len());
            row.push(CellValue::Int(cid as i64 + 1));
            row.push(CellValue::String(score_pct.clone()));
            for c in 0..src.col_count() {
                row.push(src.get(r, c).cloned().unwrap_or(CellValue::Null));
            }
            rows.push(row);
        }
    }

    DataTable {
        columns,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: None,
        format_name: Some(octa::i18n::t("fuzzy_dup.title")),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    }
}
