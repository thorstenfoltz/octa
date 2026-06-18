//! Join-tables dialog (Analyse -> Join tables...).
//!
//! The user picks a **left** tab and a **right** tab, then one or more join
//! conditions. Each condition pairs any column of the left table with any
//! column of the right table via a comparison operator (`=`, `<`, `<=`, `>`,
//! `>=`). Column names and types need not match - both sides are cast to a
//! common type before comparing (numeric when both are numeric, else text).
//! Multiple conditions are ANDed.
//!
//! Applying calls [`octa::data::join::join_two`] and opens the result in a new
//! tab (same pattern as the Union dialog).

use eframe::egui;
use egui::RichText;

use octa::data::join::{JoinCond, JoinOp, JoinType, join_two};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{JoinCondDraft, JoinState, OctaApp, TabState};

impl OctaApp {
    /// Build the default join state: left = active tab, right = the first other
    /// tab, one `=` condition on the first column of each.
    pub(crate) fn default_join_state(&self) -> JoinState {
        let active = self.active_tab;
        let right = (0..self.tabs.len())
            .find(|&i| i != active)
            .unwrap_or(active);
        JoinState {
            left_tab: active,
            right_tab: right,
            conds: vec![JoinCondDraft {
                left_col: 0,
                op: JoinOp::Eq,
                right_col: 0,
            }],
            join_type: JoinType::Left,
            error: None,
            size: DialogSize::default(),
        }
    }
}

fn tab_label(tab: &TabState, idx: usize) -> String {
    tab.table
        .source_path
        .as_ref()
        .and_then(|p| {
            std::path::Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .or_else(|| tab.custom_tab_label.clone())
        .unwrap_or_else(|| format!("Untitled {}", idx + 1))
}

fn op_label(op: JoinOp) -> &'static str {
    match op {
        JoinOp::Eq => "=",
        JoinOp::Lt => "<",
        JoinOp::Le => "<=",
        JoinOp::Gt => ">",
        JoinOp::Ge => ">=",
    }
}

fn col_name(app: &OctaApp, tab: usize, col: usize) -> String {
    app.tabs
        .get(tab)
        .and_then(|t| t.table.columns.get(col))
        .map(|c| c.name.clone())
        .unwrap_or_default()
}

pub(crate) fn render_join_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.join_dialog.is_none() {
        return;
    }
    if app.tabs.len() < 2 {
        app.join_dialog = None;
        return;
    }

    let mut close = false;
    let mut apply = false;
    let mut st = app.join_dialog.take().unwrap();

    // Clamp tab indices and condition column indices against the current tabs
    // (the user may have closed a tab while the dialog was open).
    let n_tabs = app.tabs.len();
    if st.left_tab >= n_tabs {
        st.left_tab = 0;
    }
    if st.right_tab >= n_tabs {
        st.right_tab = (0..n_tabs).find(|&i| i != st.left_tab).unwrap_or(0);
    }
    let left_cols: Vec<String> = app.tabs[st.left_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let right_cols: Vec<String> = app.tabs[st.right_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    for cond in &mut st.conds {
        if cond.left_col >= left_cols.len() {
            cond.left_col = 0;
        }
        if cond.right_col >= right_cols.len() {
            cond.right_col = 0;
        }
    }

    let tab_labels: Vec<String> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| tab_label(t, i))
        .collect();

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;
    let mut remove_cond: Option<usize> = None;
    let mut add_cond = false;
    let more_than_one = st.conds.len() > 1;

    let dialog_id = egui::Id::new("octa_join_dialog");
    let window = egui::Window::new("octa_join")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(540.0)
            .default_height(440.0)
            .min_width(420.0)
            .min_height(300.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("join_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("join.title"))
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

        egui::Panel::bottom("join_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("join.apply")).clicked() {
                        apply = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("join.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            // --- Left / right tab pickers ---
            ui.horizontal(|ui| {
                ui.label(RichText::new(octa::i18n::t("join.left_label")).strong());
                egui::ComboBox::from_id_salt("join_left_tab")
                    .selected_text(tab_labels.get(st.left_tab).cloned().unwrap_or_default())
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for (i, label) in tab_labels.iter().enumerate() {
                            ui.selectable_value(&mut st.left_tab, i, label);
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(octa::i18n::t("join.right_label")).strong());
                egui::ComboBox::from_id_salt("join_right_tab")
                    .selected_text(tab_labels.get(st.right_tab).cloned().unwrap_or_default())
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for (i, label) in tab_labels.iter().enumerate() {
                            ui.selectable_value(&mut st.right_tab, i, label);
                        }
                    });
            });

            ui.add_space(8.0);
            ui.separator();

            // --- Conditions ---
            ui.label(
                RichText::new(octa::i18n::t("join.conditions_label"))
                    .strong()
                    .size(13.0),
            );
            ui.label(
                RichText::new(octa::i18n::t("join.conditions_hint"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .id_salt("join_conds")
                .max_height(160.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (ci, cond) in st.conds.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            // Left column.
                            egui::ComboBox::from_id_salt(("join_lcol", ci))
                                .selected_text(
                                    left_cols.get(cond.left_col).cloned().unwrap_or_default(),
                                )
                                .width(140.0)
                                .show_ui(ui, |ui| {
                                    for (i, name) in left_cols.iter().enumerate() {
                                        ui.selectable_value(&mut cond.left_col, i, name);
                                    }
                                });
                            // Operator.
                            egui::ComboBox::from_id_salt(("join_op", ci))
                                .selected_text(op_label(cond.op))
                                .width(60.0)
                                .show_ui(ui, |ui| {
                                    for op in
                                        [JoinOp::Eq, JoinOp::Lt, JoinOp::Le, JoinOp::Gt, JoinOp::Ge]
                                    {
                                        ui.selectable_value(&mut cond.op, op, op_label(op));
                                    }
                                });
                            // Right column.
                            egui::ComboBox::from_id_salt(("join_rcol", ci))
                                .selected_text(
                                    right_cols.get(cond.right_col).cloned().unwrap_or_default(),
                                )
                                .width(140.0)
                                .show_ui(ui, |ui| {
                                    for (i, name) in right_cols.iter().enumerate() {
                                        ui.selectable_value(&mut cond.right_col, i, name);
                                    }
                                });
                            // Remove (only when more than one condition).
                            if more_than_one && ui.small_button("X").clicked() {
                                remove_cond = Some(ci);
                            }
                        });
                    }
                });

            if ui.button(octa::i18n::t("join.add_condition")).clicked() {
                add_cond = true;
            }

            ui.add_space(8.0);
            ui.separator();

            // --- Join type ---
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(octa::i18n::t("join.type_label"))
                        .strong()
                        .size(13.0),
                );
                let selected_text = match st.join_type {
                    JoinType::Inner => octa::i18n::t("join.type_inner"),
                    JoinType::Left => octa::i18n::t("join.type_left"),
                    JoinType::Right => octa::i18n::t("join.type_right"),
                    JoinType::Full => octa::i18n::t("join.type_full"),
                };
                egui::ComboBox::from_id_salt("join_type_combo")
                    .selected_text(selected_text)
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut st.join_type,
                            JoinType::Inner,
                            octa::i18n::t("join.type_inner"),
                        );
                        ui.selectable_value(
                            &mut st.join_type,
                            JoinType::Left,
                            octa::i18n::t("join.type_left"),
                        );
                        ui.selectable_value(
                            &mut st.join_type,
                            JoinType::Right,
                            octa::i18n::t("join.type_right"),
                        );
                        ui.selectable_value(
                            &mut st.join_type,
                            JoinType::Full,
                            octa::i18n::t("join.type_full"),
                        );
                    });
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

    if let Some(ci) = remove_cond
        && st.conds.len() > 1
    {
        st.conds.remove(ci);
    }
    if add_cond {
        st.conds.push(JoinCondDraft {
            left_col: 0,
            op: JoinOp::Eq,
            right_col: 0,
        });
    }

    if apply {
        match apply_join(app, &st) {
            Ok(()) => return,
            Err(e) => st.error = Some(e),
        }
    }
    if !close {
        app.join_dialog = Some(st);
    }
}

/// Snapshot the two chosen tabs and run the two-table join, opening the result
/// in a new tab.
fn apply_join(app: &mut OctaApp, st: &JoinState) -> Result<(), String> {
    if st.left_tab == st.right_tab {
        return Err(octa::i18n::t("join.same_tab"));
    }
    if st.conds.is_empty() {
        return Err(octa::i18n::t("join.need_key"));
    }

    // Resolve condition column indices to names against the current schemas.
    let conds: Vec<JoinCond> = st
        .conds
        .iter()
        .map(|c| JoinCond {
            left_col: col_name(app, st.left_tab, c.left_col),
            op: c.op,
            right_col: col_name(app, st.right_tab, c.right_col),
        })
        .collect();

    let mut left = app.tabs[st.left_tab].table.clone();
    left.apply_edits();
    let mut right = app.tabs[st.right_tab].table.clone();
    right.apply_edits();

    let result =
        join_two(("l", &left), ("r", &right), &conds, st.join_type).map_err(|e| e.to_string())?;

    let mut new_tab = TabState::new(app.settings.default_search_mode);
    new_tab.table = result;
    new_tab.table.source_path = None;
    new_tab.table.format_name = None;
    new_tab.custom_tab_label = Some(octa::i18n::t("join.title"));
    new_tab.filter_dirty = true;
    if new_tab.table.row_count() > 0 && new_tab.table.col_count() > 0 {
        new_tab.table_state.selected_cell = Some((0, 0));
    }
    app.tabs.push(new_tab);
    app.active_tab = app.tabs.len() - 1;
    Ok(())
}
