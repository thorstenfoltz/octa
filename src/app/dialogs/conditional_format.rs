//! Conditional formatting dialog. Edits the active tab's session-scoped list
//! of [`CondRule`]s: each rule colours cells whose value matches a predicate
//! (`<column> <operator> <value> -> colour`). Rules apply live - the table
//! re-evaluates them every frame (see `octa::data::conditional_format` and the
//! evaluation in `src/ui/table_view/rows.rs`), so there is no Apply step; the
//! dialog just adds / edits / removes entries.

use eframe::egui;
use egui::RichText;

use octa::data::MarkColor;
use octa::data::conditional_format::{CondOp, CondRule};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};
use octa::ui::theme::ThemeColors;

use super::super::state::OctaApp;

pub(crate) fn render_conditional_format_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.tabs[app.active_tab].show_conditional_format {
        return;
    }

    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();

    let mut rules = app.tabs[app.active_tab].conditional_format_rules.clone();
    let mut close_requested = false;
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;
    let mut move_up: Option<usize> = None;
    let mut move_down: Option<usize> = None;
    let mut size = app.tabs[app.active_tab].conditional_format_size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_conditional_format_dialog");
    let window = egui::Window::new("octa_conditional_format")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(620.0)
            .default_height(400.0)
            .min_width(460.0)
            .min_height(200.0)
    });

    let inner = window.show(ctx, |ui| {
        // Header: title + window controls (minimize / maximize / close).
        egui::Panel::top("cnf_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.cnf_title"))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close_requested = true;
                        }
                    });
                });
            });

        if minimized {
            return;
        }

        // Footer: Add rule / Clear all / Close.
        egui::Panel::bottom("cnf_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("dialog.cnf_add_rule")).clicked() {
                        rules.push(CondRule::new());
                        changed = true;
                    }
                    if !rules.is_empty()
                        && ui.button(octa::i18n::t("dialog.cnf_clear_all")).clicked()
                    {
                        rules.clear();
                        changed = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("common.close")).clicked() {
                            close_requested = true;
                        }
                    });
                });
            });

        // Body: rule list (scrolls), in the central area.
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("dialog.cnf_desc"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.label(
                RichText::new(octa::i18n::t("dialog.cnf_order_hint"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            let rule_count = rules.len();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (i, rule) in rules.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            // Column picker: "(any)" or a specific column.
                            let col_label = match rule.column {
                                None => octa::i18n::t("dialog.cnf_any_column"),
                                Some(c) => col_names
                                    .get(c)
                                    .cloned()
                                    .unwrap_or_else(|| format!("col {c}")),
                            };
                            egui::ComboBox::from_id_salt(("cf_col", i))
                                .selected_text(col_label)
                                .width(130.0)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(
                                            rule.column.is_none(),
                                            octa::i18n::t("dialog.cnf_any_column"),
                                        )
                                        .clicked()
                                    {
                                        rule.column = None;
                                        changed = true;
                                    }
                                    for (c, name) in col_names.iter().enumerate() {
                                        if ui
                                            .selectable_label(rule.column == Some(c), name)
                                            .clicked()
                                        {
                                            rule.column = Some(c);
                                            changed = true;
                                        }
                                    }
                                });

                            // Operator picker.
                            egui::ComboBox::from_id_salt(("cf_op", i))
                                .selected_text(rule.op.label_t())
                                .width(150.0)
                                .show_ui(ui, |ui| {
                                    for &op in CondOp::ALL {
                                        if ui
                                            .selectable_label(rule.op == op, op.label_t())
                                            .clicked()
                                        {
                                            rule.op = op;
                                            changed = true;
                                        }
                                    }
                                });

                            // Value box (greyed out for Empty / NotEmpty).
                            ui.add_enabled_ui(rule.op.uses_value(), |ui| {
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut rule.value)
                                            .desired_width(90.0)
                                            .hint_text(octa::i18n::t("dialog.cnf_value")),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            });

                            // Colour picker, each entry tinted with its swatch.
                            egui::ComboBox::from_id_salt(("cf_color", i))
                                .selected_text(
                                    RichText::new(rule.color.label_t())
                                        .color(ThemeColors::mark_swatch(rule.color)),
                                )
                                .width(90.0)
                                .show_ui(ui, |ui| {
                                    for &mc in MarkColor::ALL {
                                        let label = RichText::new(mc.label_t())
                                            .color(ThemeColors::mark_swatch(mc));
                                        if ui.selectable_label(rule.color == mc, label).clicked() {
                                            rule.color = mc;
                                            changed = true;
                                        }
                                    }
                                });

                            if ui
                                .checkbox(
                                    &mut rule.case_sensitive,
                                    octa::i18n::t("dialog.cnf_case"),
                                )
                                .changed()
                            {
                                changed = true;
                            }

                            // Reorder: rules are first-match-wins, so order is
                            // the if / else-if chain the user builds.
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
                    if rules.is_empty() {
                        ui.label(
                            RichText::new(octa::i18n::t("dialog.cnf_empty"))
                                .size(11.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    app.tabs[app.active_tab].conditional_format_size = size;

    if let Some(i) = remove_idx
        && i < rules.len()
    {
        rules.remove(i);
        changed = true;
    }
    if let Some(i) = move_up
        && i > 0
        && i < rules.len()
    {
        rules.swap(i, i - 1);
        changed = true;
    }
    if let Some(i) = move_down
        && i + 1 < rules.len()
    {
        rules.swap(i, i + 1);
        changed = true;
    }

    if changed {
        app.tabs[app.active_tab].conditional_format_rules = rules;
        ctx.request_repaint();
    }
    if close_requested {
        app.tabs[app.active_tab].show_conditional_format = false;
    }
}
