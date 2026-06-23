//! Drop-duplicate-rows dialog (Edit -> Drop duplicate rows...).
//!
//! The user picks which columns form the duplicate key (default: all columns
//! ticked, meaning whole-row comparison) and whether to keep the First or Last
//! occurrence of each key. Applying the dialog replaces the active tab's rows
//! with the deduplicated result as a single undoable structural edit.
//!
//! The dedupe engine lives in `octa::data::dedupe`; this file is only the
//! picker and dispatch.

use eframe::egui;
use egui::RichText;

use octa::data::dedupe::{KeepWhich, dedupe_dropped_indices};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{DedupeState, OctaApp};

pub(crate) fn render_dedupe_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.dedupe_dialog.is_none() {
        return;
    }

    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();

    // Nothing to do on an empty table; close silently.
    if col_names.is_empty() {
        app.dedupe_dialog = None;
        return;
    }

    let mut close = false;
    let mut run = false;
    let mut st = app.dedupe_dialog.take().unwrap();

    // If the column count changed since the dialog was opened (e.g. the user
    // added a column while the dialog was minimised), re-seed the selection so
    // we never index out of bounds.
    if st.col_selected.len() != col_names.len() {
        let col_count = col_names.len();
        st.col_selected = vec![true; col_count];
        st.key_cols = (0..col_count).collect();
    }

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_dedupe_dialog");
    let window = egui::Window::new("octa_dedupe")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(420.0)
            .default_height(360.0)
            .min_width(320.0)
            .min_height(220.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("dedupe_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dedupe.title"))
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

        egui::Panel::bottom("dedupe_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("dedupe.apply")).clicked() {
                        run = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("dedupe.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            // Key column picker.
            ui.label(
                RichText::new(octa::i18n::t("dedupe.keys_label"))
                    .strong()
                    .size(13.0),
            );

            ui.horizontal(|ui| {
                if ui.small_button(octa::i18n::t("dialog.sel_all")).clicked() {
                    for b in &mut st.col_selected {
                        *b = true;
                    }
                    st.key_cols = (0..col_names.len()).collect();
                }
                if ui.small_button(octa::i18n::t("dialog.sel_none")).clicked() {
                    for b in &mut st.col_selected {
                        *b = false;
                    }
                    st.key_cols.clear();
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
            ui.separator();

            egui::ScrollArea::vertical()
                .max_height(180.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (idx, name) in col_names.iter().enumerate() {
                        let mut on = st.col_selected.get(idx).copied().unwrap_or(false);
                        if ui.checkbox(&mut on, name).changed() {
                            if idx < st.col_selected.len() {
                                st.col_selected[idx] = on;
                            }
                            if on {
                                if !st.key_cols.contains(&idx) {
                                    st.key_cols.push(idx);
                                    st.key_cols.sort_unstable();
                                }
                            } else {
                                st.key_cols.retain(|&c| c != idx);
                            }
                        }
                    }
                });

            ui.add_space(6.0);
            ui.separator();

            // Keep which occurrence.
            ui.label(
                RichText::new(octa::i18n::t("dedupe.keep_label"))
                    .strong()
                    .size(13.0),
            );
            ui.radio_value(
                &mut st.keep,
                KeepWhich::First,
                octa::i18n::t("dedupe.keep_first"),
            );
            ui.radio_value(
                &mut st.keep,
                KeepWhich::Last,
                octa::i18n::t("dedupe.keep_last"),
            );
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if run {
        apply_dedupe(app, st);
        return; // dialog dropped
    }
    if !close {
        app.dedupe_dialog = Some(st);
    }
}

/// Run `dedupe_rows`, compute how many rows were removed, replace the active
/// tab's data with the result, and record one coalesced undo step.
fn apply_dedupe(app: &mut OctaApp, st: DedupeState) {
    let active = app.active_tab;

    // Merge pending cell edits so dedupe sees the visible values.
    app.tabs[active].table.apply_edits();

    // The engine tells us exactly which original rows to drop, in descending
    // order. Delete each (highest first, so indices don't shift) and coalesce
    // the per-row DeleteRow undo actions into one Batch, so a single Ctrl+Z
    // restores the full table.
    let dropped = dedupe_dropped_indices(&app.tabs[active].table, &st.key_cols, st.keep);
    let removed = dropped.len();

    if removed == 0 {
        app.status_message = Some((
            octa::i18n::t("dedupe.removed_status"),
            std::time::Instant::now(),
        ));
        return;
    }

    let undo_start = app.tabs[active].table.undo_stack.len();
    for row_idx in dropped {
        app.tabs[active].table.delete_row(row_idx);
    }
    app.tabs[active].table.coalesce_undo_since(undo_start);
    app.tabs[active].table.structural_changes = true;

    app.tabs[active].filter_dirty = true;
    app.tabs[active].table_state.widths_initialized = false;

    app.status_message = Some((
        format!("{} {}", removed, octa::i18n::t("dedupe.removed_status")),
        std::time::Instant::now(),
    ));
}
