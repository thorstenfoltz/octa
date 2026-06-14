//! Conditional-column dialog (Edit -> Conditional column...). Builds a new
//! column from an if / else-if / else rule chain over the active tab: each rule
//! is "if `<column>` `<operator>` `<value>` then `<output>`"; the first rule
//! that matches a row decides its value, otherwise the `else` output is used.
//!
//! The conditions reuse the conditional-formatting comparison operators
//! ([`CondOp`]); the evaluation is the pure
//! [`octa::data::transform::build_case_column`]. Apply materialises the result
//! as a new column via [`DataTable::insert_column`] + [`DataTable::set`] (so it
//! is undoable, like the Insert-column and Transform dialogs).

use eframe::egui;
use egui::RichText;

use octa::data::conditional_format::CondOp;
use octa::data::transform::{CaseRule, CaseSpec, build_case_column, infer_case_column_type};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{ConditionalColumnState, OctaApp};

pub(crate) fn render_conditional_column_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.conditional_column_dialog.is_none() {
        return;
    }

    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();

    let mut close = false;
    let mut apply = false;
    let mut st = app.conditional_column_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_conditional_column_dialog");
    let window = egui::Window::new("octa_conditional_column")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(640.0)
            .default_height(420.0)
            .min_width(480.0)
            .min_height(220.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("ccol_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("ccol.title"))
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

        egui::Panel::bottom("ccol_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("ccol.apply")).clicked() {
                        apply = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("common.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("ccol.desc"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            rule_list(ui, &mut st, &col_names);
            ui.add_space(6.0);
            ui.separator();

            // else branch + new-column name + position.
            ui.horizontal(|ui| {
                ui.label(RichText::new(octa::i18n::t("ccol.else_label")).strong());
                ui.add(
                    egui::TextEdit::singleline(&mut st.else_output)
                        .desired_width(160.0)
                        .hint_text(octa::i18n::t("ccol.output_hint")),
                );
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("ccol.new_name"));
                ui.add(
                    egui::TextEdit::singleline(&mut st.new_name)
                        .desired_width(180.0)
                        .hint_text(octa::i18n::t("ccol.default_name")),
                );
            });
            let col_count = col_names.len();
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("ccol.position"));
                let buf_empty = st.insert_pos_text.is_empty();
                let valid = buf_empty
                    || st
                        .insert_pos_text
                        .trim()
                        .parse::<usize>()
                        .is_ok_and(|v| (1..=col_count + 1).contains(&v));
                let mut te = egui::TextEdit::singleline(&mut st.insert_pos_text)
                    .desired_width(48.0)
                    .hint_text((col_count + 1).to_string());
                if !valid {
                    te = te.text_color(egui::Color32::from_rgb(220, 80, 80));
                }
                ui.add(te);
                ui.label(format!("/ {}", col_count + 1));
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

    if apply {
        match apply_conditional_column(app, &st, &col_names) {
            Ok(()) => return,
            Err(e) => st.error = Some(e),
        }
    }
    if !close {
        app.conditional_column_dialog = Some(st);
    }
}

/// The ordered list of if / else-if rules, each with reorder + delete.
fn rule_list(ui: &mut egui::Ui, st: &mut ConditionalColumnState, cols: &[String]) {
    let mut remove_idx: Option<usize> = None;
    let mut move_up: Option<usize> = None;
    let mut move_down: Option<usize> = None;
    let rule_count = st.rules.len();

    egui::ScrollArea::vertical()
        .id_salt("ccol_rules")
        .auto_shrink([false, true])
        .max_height(200.0)
        .show(ui, |ui| {
            for (i, rule) in st.rules.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(if i == 0 {
                        octa::i18n::t("ccol.if")
                    } else {
                        octa::i18n::t("ccol.elseif")
                    });

                    // Condition column.
                    let col_label = rule
                        .cond_col
                        .and_then(|c| cols.get(c).cloned())
                        .unwrap_or_else(|| octa::i18n::t("ccol.pick_column"));
                    egui::ComboBox::from_id_salt(("ccol_col", i))
                        .selected_text(col_label)
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for (c, name) in cols.iter().enumerate() {
                                if ui
                                    .selectable_label(rule.cond_col == Some(c), name)
                                    .clicked()
                                {
                                    rule.cond_col = Some(c);
                                }
                            }
                        });

                    // Operator.
                    egui::ComboBox::from_id_salt(("ccol_op", i))
                        .selected_text(rule.op.label_t())
                        .width(140.0)
                        .show_ui(ui, |ui| {
                            for &op in CondOp::ALL {
                                if ui.selectable_label(rule.op == op, op.label_t()).clicked() {
                                    rule.op = op;
                                }
                            }
                        });

                    // Comparison value (greyed for Empty / NotEmpty).
                    ui.add_enabled_ui(rule.op.uses_value(), |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut rule.value)
                                .desired_width(80.0)
                                .hint_text(octa::i18n::t("dialog.cnf_value")),
                        );
                    });

                    ui.label("->");
                    ui.add(
                        egui::TextEdit::singleline(&mut rule.output)
                            .desired_width(110.0)
                            .hint_text(octa::i18n::t("ccol.output_hint")),
                    );

                    if ui
                        .add_enabled(i > 0, egui::Button::new("^").small())
                        .on_hover_text(octa::i18n::t("dialog.cnf_move_up"))
                        .clicked()
                    {
                        move_up = Some(i);
                    }
                    if ui
                        .add_enabled(i + 1 < rule_count, egui::Button::new("v").small())
                        .on_hover_text(octa::i18n::t("dialog.cnf_move_down"))
                        .clicked()
                    {
                        move_down = Some(i);
                    }
                    if ui
                        .small_button("✕")
                        .on_hover_text(octa::i18n::t("dialog.cnf_remove"))
                        .clicked()
                    {
                        remove_idx = Some(i);
                    }
                });
            }
        });

    if ui.button(octa::i18n::t("ccol.add_rule")).clicked() {
        st.rules.push(CaseRule::new());
    }

    if let Some(i) = remove_idx
        && i < st.rules.len()
    {
        st.rules.remove(i);
    }
    if let Some(i) = move_up
        && i > 0
        && i < st.rules.len()
    {
        st.rules.swap(i, i - 1);
    }
    if let Some(i) = move_down
        && i + 1 < st.rules.len()
    {
        st.rules.swap(i, i + 1);
    }
}

/// Build the column and insert it. Returns a localized error on bad input.
fn apply_conditional_column(
    app: &mut OctaApp,
    st: &ConditionalColumnState,
    col_names: &[String],
) -> Result<(), String> {
    if app.is_readonly() {
        return Err(octa::i18n::t("transform.readonly"));
    }
    // At least one usable rule, or an else output, must be present, otherwise
    // the column would be all-empty.
    let usable_rules = st.rules.iter().any(|r| r.cond_col.is_some());
    if !usable_rules && st.else_output.trim().is_empty() {
        return Err(octa::i18n::t("ccol.need_rule"));
    }

    let active = app.active_tab;
    let spec = CaseSpec {
        rules: st.rules.clone(),
        else_output: st.else_output.clone(),
    };
    let values = build_case_column(&app.tabs[active].table, &spec);
    let data_type = infer_case_column_type(&values);

    let base = st.new_name.trim();
    let name = unique_name(col_names, if base.is_empty() { "derived" } else { base });
    let idx = st
        .insert_pos_text
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|v| (1..=col_names.len() + 1).contains(v))
        .map(|v| v - 1)
        .unwrap_or(col_names.len());

    let tbl = &mut app.tabs[active].table;
    tbl.insert_column(idx, name, data_type);
    for (r, v) in values.into_iter().enumerate() {
        tbl.set(r, idx, v);
    }

    app.tabs[active].table_state.widths_initialized = false;
    app.tabs[active].filter_dirty = true;
    Ok(())
}

/// Make `base` unique against existing column names by appending `_2`, `_3`, ...
fn unique_name(cols: &[String], base: &str) -> String {
    if !cols.iter().any(|c| c == base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if !cols.iter().any(|c| c == &candidate) {
            return candidate;
        }
        n += 1;
    }
}
