//! Fill-missing-values (impute) dialog (Edit -> Fill missing values...).
//!
//! The user picks a column and a strategy (Mean / Median / Mode / Constant /
//! Forward fill / Backward fill). Applying calls the pure engine function
//! [`octa::data::impute::impute_column`], then writes the returned values back
//! into the column via [`DataTable::set`], which pushes individual cell-edit
//! entries onto the undo stack; those are coalesced into one
//! [`octa::data::UndoAction::Batch`] so a single Ctrl+Z reverts every
//! replaced cell.
//!
//! Error handling (e.g. Mean on a text column) stores the message in
//! [`ImputeState::error`] and keeps the dialog open.

use eframe::egui;
use egui::RichText;

use octa::data::impute::{ImputeStrategy, impute_column};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{ImputeState, OctaApp};

/// The six strategies in display order; indices match `ImputeState.strategy_idx`.
const STRATEGIES: &[(&str, &str)] = &[
    ("impute.strat_mean", "Mean"),
    ("impute.strat_median", "Median"),
    ("impute.strat_mode", "Mode"),
    ("impute.strat_constant", "Constant"),
    ("impute.strat_ffill", "Forward fill"),
    ("impute.strat_bfill", "Backward fill"),
];

fn strategy_from_state(st: &ImputeState) -> ImputeStrategy {
    match st.strategy_idx {
        0 => ImputeStrategy::Mean,
        1 => ImputeStrategy::Median,
        2 => ImputeStrategy::Mode,
        3 => ImputeStrategy::Constant(st.constant.clone()),
        4 => ImputeStrategy::ForwardFill,
        _ => ImputeStrategy::BackwardFill,
    }
}

pub(crate) fn render_impute_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.impute_dialog.is_none() {
        return;
    }

    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();

    // Close silently if the table has no columns.
    if col_names.is_empty() {
        app.impute_dialog = None;
        return;
    }

    let mut close = false;
    let mut apply = false;
    let mut st = app.impute_dialog.take().unwrap();

    // Guard against the column index becoming stale (e.g. user deleted a
    // column while the dialog was open).
    if st.col >= col_names.len() {
        st.col = 0;
    }

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_impute_dialog");
    let window = egui::Window::new("octa_impute")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(420.0)
            .default_height(260.0)
            .min_width(320.0)
            .min_height(180.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("impute_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("impute.title"))
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

        egui::Panel::bottom("impute_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("impute.apply")).clicked() {
                        apply = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("impute.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            // Column picker.
            ui.horizontal(|ui| {
                ui.label(RichText::new(octa::i18n::t("impute.column_label")).strong());
                let col_text = col_names.get(st.col).cloned().unwrap_or_default();
                egui::ComboBox::from_id_salt("impute_col")
                    .selected_text(col_text)
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in col_names.iter().enumerate() {
                            if ui.selectable_label(st.col == i, name).clicked() {
                                st.col = i;
                                st.error = None;
                            }
                        }
                    });
            });

            ui.add_space(6.0);

            // Strategy picker.
            ui.horizontal(|ui| {
                ui.label(RichText::new(octa::i18n::t("impute.strategy_label")).strong());
                let strat_text = octa::i18n::t(STRATEGIES[st.strategy_idx].0);
                egui::ComboBox::from_id_salt("impute_strategy")
                    .selected_text(strat_text)
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for (idx, &(key, _)) in STRATEGIES.iter().enumerate() {
                            if ui
                                .selectable_label(st.strategy_idx == idx, octa::i18n::t(key))
                                .clicked()
                            {
                                st.strategy_idx = idx;
                                st.error = None;
                            }
                        }
                    });
            });

            // Constant value field (only shown when Constant is selected).
            if st.strategy_idx == 3 {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(octa::i18n::t("impute.constant_label"));
                    ui.add(egui::TextEdit::singleline(&mut st.constant).desired_width(200.0));
                });
            }

            // Inline error (shown when the last Apply failed).
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

    if apply {
        match apply_impute(app, &st) {
            Ok(()) => {
                // Success: drop the dialog.
                return;
            }
            Err(e) => st.error = Some(e),
        }
    }
    if !close {
        app.impute_dialog = Some(st);
    }
}

/// Run the impute engine, write the result back into the active tab's column,
/// and coalesce all cell-level undo entries into one batch step.
fn apply_impute(app: &mut OctaApp, st: &ImputeState) -> Result<(), String> {
    if app.is_readonly() {
        return Err(octa::i18n::t("impute.title"));
    }

    let active = app.active_tab;
    let col = st.col;

    // Merge any pending cell edits so the engine sees the visible values.
    app.tabs[active].table.apply_edits();

    let strategy = strategy_from_state(st);
    let values =
        impute_column(&app.tabs[active].table, col, &strategy).map_err(|e| e.to_string())?;

    // Record the start of the undo stack so we can coalesce all `set` calls
    // into one batch action, giving a single Ctrl+Z to undo all fills.
    let undo_start = app.tabs[active].table.undo_stack.len();

    let tbl = &mut app.tabs[active].table;
    for (r, v) in values.into_iter().enumerate() {
        tbl.set(r, col, v);
    }

    // Coalesce individual cell-edit undo entries into one Batch so Ctrl+Z
    // reverts the whole fill in one step (mirrors how transform.rs handles
    // in-place column edits like FillDown / Replace).
    app.tabs[active].table.coalesce_undo_since(undo_start);

    app.tabs[active].filter_dirty = true;
    app.tabs[active].table_state.widths_initialized = false;

    let col_name = app.tabs[active]
        .table
        .columns
        .get(col)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    app.status_message = Some((
        format!("{} \"{}\"", octa::i18n::t("impute.title"), col_name),
        std::time::Instant::now(),
    ));

    Ok(())
}
