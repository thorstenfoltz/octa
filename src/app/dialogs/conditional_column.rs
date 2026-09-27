//! Conditional-column dialog (Columns -> Conditional column...). Builds a new
//! column from an if / else-if / else rule chain over the active tab: each rule
//! is "if `<conditions>` then `<output>`", its conditions joined by and / or;
//! the first rule
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
use octa::data::retype::TargetType;
use octa::data::transform::{
    CaseCond, CaseRule, CaseSpec, build_case_column, infer_case_column_type,
};
use octa::data::value_frequency::{BinningMode, compute_value_frequency};
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
            .default_width(900.0)
            .default_height(480.0)
            .min_width(600.0)
            .min_height(220.0)
    });

    let inner = window.show(ctx, |ui| {
        // Buttons and combos size themselves against `interact_size`; one
        // height for all of them keeps every line of the dialog level.
        ui.spacing_mut().interact_size.y = control_h(ui);
        egui::Panel::top("ccol_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
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
            .show(ui, |ui| {
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

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("ccol.desc"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(8.0);
            rule_list(ui, &mut st, &col_names, &app.tabs[app.active_tab].table);
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // New-column name + position, labels in one column so the fields
            // line up.
            let col_count = col_names.len();
            egui::Grid::new("ccol_target_grid")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label(octa::i18n::t("ccol.new_name"));
                    field(
                        ui,
                        180.0,
                        egui::TextEdit::singleline(&mut st.new_name)
                            .hint_text(octa::i18n::t("ccol.default_name")),
                    );
                    ui.end_row();

                    ui.label(octa::i18n::t("ccol.type_label"));
                    let type_label = |t: Option<TargetType>| {
                        octa::i18n::t(t.map_or("ccol.type_auto", TargetType::i18n_key))
                    };
                    egui::ComboBox::from_id_salt("ccol_type")
                        .selected_text(type_label(st.output_type))
                        .width(180.0)
                        .show_ui(ui, |ui| {
                            let choices = std::iter::once(None)
                                .chain(TargetType::ALL.iter().copied().map(Some));
                            for choice in choices {
                                ui.selectable_value(
                                    &mut st.output_type,
                                    choice,
                                    type_label(choice),
                                );
                            }
                        })
                        .response
                        .on_hover_text(octa::i18n::t("ccol.type_hint"));
                    ui.end_row();

                    ui.label(octa::i18n::t("ccol.position"));
                    ui.horizontal(|ui| {
                        let buf_empty = st.insert_pos_text.is_empty();
                        let valid = buf_empty
                            || st
                                .insert_pos_text
                                .trim()
                                .parse::<usize>()
                                .is_ok_and(|v| (1..=col_count + 1).contains(&v));
                        let mut te = egui::TextEdit::singleline(&mut st.insert_pos_text)
                            .hint_text((col_count + 1).to_string());
                        if !valid {
                            te = te.text_color(egui::Color32::from_rgb(220, 80, 80));
                        }
                        field(ui, 56.0, te);
                        ui.label(format!("/ {}", col_count + 1));
                    });
                    ui.end_row();
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

/// Width of the column picker and of the output fields, shared so every
/// row's fields start and end at the same place.
const FIELD_W: f32 = 160.0;

/// Width of a condition's operator picker.
const OP_W: f32 = 140.0;

/// Width of a condition's comparison value field.
const VALUE_W: f32 = 240.0;

/// Width of the and / or picker.
const JOIN_W: f32 = 64.0;

/// How many of a column's values the value picker lists, most common first.
const PICK_MAX_VALUES: usize = 1000;

/// What a click in the rule list asked for, applied after it is drawn (the
/// rules are borrowed while it draws).
enum RuleEdit {
    AddCond(usize),
    RemoveCond(usize, usize),
    Up(usize),
    Down(usize),
    Remove(usize),
}

/// The one height every control in the dialog is given. Left alone a
/// ComboBox, a `TextEdit` and a button each take a different natural height,
/// so a row of them looks ragged.
fn control_h(ui: &egui::Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body) + 10.0
}

/// A text field of exactly `w` x [`control_h`], its text centred vertically.
fn field(ui: &mut egui::Ui, w: f32, edit: egui::TextEdit<'_>) -> egui::Response {
    let h = control_h(ui);
    ui.add_sized([w, h], edit.vertical_align(egui::Align::Center))
}

/// A square [`control_h`] button for a one-glyph action (move, remove, add).
fn icon_button(ui: &mut egui::Ui, enabled: bool, glyph: &str) -> egui::Response {
    let h = control_h(ui);
    ui.add_enabled(enabled, egui::Button::new(glyph).min_size(egui::vec2(h, h)))
}

/// An empty control-tall slot, so a line without some control keeps the
/// columns after it where the other lines have them.
fn blank(ui: &mut egui::Ui, w: f32) {
    let h = control_h(ui);
    ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
}

/// The fixed-width keyword column (If / Else if / and / Then / Else), its
/// content centred vertically on the line.
fn keyword_cell<R>(ui: &mut egui::Ui, w: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let size = egui::vec2(w, control_h(ui));
    ui.allocate_ui_with_layout(
        size,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_size(size);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            add(ui)
        },
    )
    .inner
}

/// One line of controls, every widget centred on a [`control_h`] line.
fn control_row<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.set_min_height(control_h(ui));
        add(ui)
    })
    .inner
}

/// A rule's card: a rounded, lightly filled box spanning the list's width.
fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            add(ui)
        })
        .inner
}

/// Width of the keyword column: the widest localized keyword, or the and / or
/// picker, whichever is wider.
fn keyword_width(ui: &egui::Ui) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let colour = ui.visuals().strong_text_color();
    ["ccol.if", "ccol.elseif", "ccol.then", "ccol.else"]
        .into_iter()
        .map(|k| {
            ui.fonts_mut(|f| f.layout_no_wrap(octa::i18n::t(k), font.clone(), colour))
                .size()
                .x
        })
        .fold(JOIN_W, f32::max)
        + 12.0
}

fn keyword(text: String) -> RichText {
    RichText::new(text).strong()
}

fn join_label(all: bool) -> String {
    octa::i18n::t(if all { "ccol.and" } else { "ccol.or" })
}

/// What the rule list's lines share while it draws.
struct RuleCtx<'a> {
    kw_w: f32,
    cols: &'a [String],
    table: &'a octa::data::DataTable,
    lists: &'a mut std::collections::HashMap<usize, ValueList>,
    edit: Option<RuleEdit>,
}

/// The ordered if / else-if rules and the else value. Each rule is a card:
/// one line per condition (keyword | column | operator | value), an
/// "Add condition" line, then a "Then" line with the output and the rule's
/// buttons. Every column has a fixed width, so all cards line up.
fn rule_list(
    ui: &mut egui::Ui,
    st: &mut ConditionalColumnState,
    cols: &[String],
    table: &octa::data::DataTable,
) {
    // Taken out for the list's duration: the rules are borrowed mutably there.
    let mut value_lists = std::mem::take(&mut st.value_lists);
    let mut cx = RuleCtx {
        kw_w: keyword_width(ui),
        cols,
        table,
        lists: &mut value_lists,
        edit: None,
    };
    let rule_count = st.rules.len();

    egui::ScrollArea::both()
        .id_salt("ccol_rules")
        .auto_shrink([false, true])
        .max_height((ui.available_height() - 110.0).max(120.0))
        .show(ui, |ui| {
            // A hovered widget's frame grows a pixel or two past its rect;
            // without this margin the scroll area's edge clips it.
            egui::Frame::NONE
                .inner_margin(egui::Margin::same(3))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    for (i, rule) in st.rules.iter_mut().enumerate() {
                        card(ui, |ui| {
                            rule_card(ui, &mut cx, i, rule_count, rule);
                        });
                    }
                    card(ui, |ui| {
                        control_row(ui, |ui| {
                            keyword_cell(ui, cx.kw_w, |ui| {
                                ui.label(keyword(octa::i18n::t("ccol.else")))
                            });
                            field(
                                ui,
                                FIELD_W,
                                egui::TextEdit::singleline(&mut st.else_output)
                                    .hint_text(octa::i18n::t("ccol.output_hint")),
                            );
                        });
                    });
                });
        });

    let edit = cx.edit;
    st.value_lists = value_lists;

    if ui
        .button(format!("+ {}", octa::i18n::t("ccol.add_rule")))
        .clicked()
    {
        st.rules.push(CaseRule::new());
    }

    let rules = &mut st.rules;
    match edit {
        Some(RuleEdit::AddCond(i)) if i < rules.len() => rules[i].conditions.push(CaseCond::new()),
        Some(RuleEdit::RemoveCond(i, j)) if i < rules.len() && j < rules[i].conditions.len() => {
            rules[i].conditions.remove(j);
        }
        Some(RuleEdit::Up(i)) if i > 0 && i < rules.len() => rules.swap(i, i - 1),
        Some(RuleEdit::Down(i)) if i + 1 < rules.len() => rules.swap(i, i + 1),
        Some(RuleEdit::Remove(i)) if i < rules.len() => {
            rules.remove(i);
        }
        _ => {}
    }
}

/// One rule's card body: its conditions, "Add condition", then "Then".
fn rule_card(
    ui: &mut egui::Ui,
    cx: &mut RuleCtx<'_>,
    i: usize,
    rule_count: usize,
    rule: &mut CaseRule,
) {
    let cond_count = rule.conditions.len();
    for (j, cond) in rule.conditions.iter_mut().enumerate() {
        condition_lines(ui, cx, (i, j), cond_count, &mut rule.match_all, cond);
    }

    control_row(ui, |ui| {
        blank(ui, cx.kw_w);
        if ui
            .button(format!("+ {}", octa::i18n::t("ccol.add_condition")))
            .on_hover_text(octa::i18n::t("ccol.add_condition_hint"))
            .clicked()
        {
            cx.edit = Some(RuleEdit::AddCond(i));
        }
    });

    control_row(ui, |ui| {
        keyword_cell(ui, cx.kw_w, |ui| {
            ui.label(keyword(octa::i18n::t("ccol.then")))
        });
        field(
            ui,
            FIELD_W,
            egui::TextEdit::singleline(&mut rule.output)
                .hint_text(octa::i18n::t("ccol.output_hint")),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, true, "\u{2715}")
                .on_hover_text(octa::i18n::t("dialog.cnf_remove"))
                .clicked()
            {
                cx.edit = Some(RuleEdit::Remove(i));
            }
            if icon_button(ui, i + 1 < rule_count, "v")
                .on_hover_text(octa::i18n::t("dialog.cnf_move_down"))
                .clicked()
            {
                cx.edit = Some(RuleEdit::Down(i));
            }
            if icon_button(ui, i > 0, "^")
                .on_hover_text(octa::i18n::t("dialog.cnf_move_up"))
                .clicked()
            {
                cx.edit = Some(RuleEdit::Up(i));
            }
        });
    });
}

/// One condition: its first line (keyword | column | operator | first value |
/// value buttons | remove condition), then one line per further value, each
/// field straight under the first.
fn condition_lines(
    ui: &mut egui::Ui,
    cx: &mut RuleCtx<'_>,
    id: (usize, usize),
    cond_count: usize,
    match_all: &mut bool,
    cond: &mut CaseCond,
) {
    let (i, j) = id;
    let uses = cond.op.uses_value();
    let h = control_h(ui);
    let gap = ui.spacing().item_spacing.x;
    let mut remove_value: Option<usize> = None;

    // By index: the picker on the first line may add or remove values.
    let mut k = 0;
    while k < cond.values.len() {
        control_row(ui, |ui| {
            if k == 0 {
                keyword_cell(ui, cx.kw_w, |ui| match j {
                    0 => {
                        ui.label(keyword(octa::i18n::t(if i == 0 {
                            "ccol.if"
                        } else {
                            "ccol.elseif"
                        })));
                    }
                    // The second condition carries the rule's and / or
                    // choice; later ones repeat it.
                    1 => {
                        egui::ComboBox::from_id_salt(("ccol_join", i))
                            .selected_text(join_label(*match_all))
                            .width(JOIN_W)
                            .show_ui(ui, |ui| {
                                for all in [true, false] {
                                    ui.selectable_value(match_all, all, join_label(all));
                                }
                            })
                            .response
                            .on_hover_text(octa::i18n::t("ccol.combine_hint"));
                    }
                    _ => {
                        ui.label(keyword(join_label(*match_all)));
                    }
                });

                let col_label = cond
                    .cond_col
                    .and_then(|c| cx.cols.get(c).cloned())
                    .unwrap_or_else(|| octa::i18n::t("ccol.pick_column"));
                egui::ComboBox::from_id_salt(("ccol_col", i, j))
                    .selected_text(col_label)
                    .width(FIELD_W)
                    .show_ui(ui, |ui| {
                        for (c, name) in cx.cols.iter().enumerate() {
                            ui.selectable_value(&mut cond.cond_col, Some(c), name);
                        }
                    });

                egui::ComboBox::from_id_salt(("ccol_op", i, j))
                    .selected_text(cond.op.label_t())
                    .width(OP_W)
                    .show_ui(ui, |ui| {
                        for &op in CondOp::ALL {
                            ui.selectable_value(&mut cond.op, op, op.label_t());
                        }
                    });
            } else {
                // The keyword, column and operator columns and their gaps.
                blank(ui, cx.kw_w + FIELD_W + OP_W + 2.0 * gap);
            }

            // Comparison value, greyed for Empty / NotEmpty.
            ui.add_enabled_ui(uses, |ui| {
                field(
                    ui,
                    VALUE_W,
                    egui::TextEdit::singleline(&mut cond.values[k])
                        .hint_text(octa::i18n::t("dialog.cnf_value")),
                )
                .on_hover_text(octa::i18n::t("ccol.values_hint"));
            });

            // Remove-value keeps its slot while there is one value, so the
            // buttons after it do not jump when a second one is added.
            if cond.values.len() > 1 {
                if icon_button(ui, true, "\u{2715}")
                    .on_hover_text(octa::i18n::t("ccol.remove_value_hint"))
                    .clicked()
                {
                    remove_value = Some(k);
                }
            } else {
                blank(ui, h);
            }

            if k == 0 {
                value_picker(ui, id, cond, uses, cx.table, cx.lists);
                if icon_button(ui, uses, "+")
                    .on_hover_text(octa::i18n::t("ccol.add_value_hint"))
                    .on_disabled_hover_text(octa::i18n::t("ccol.pick_value_no_value"))
                    .clicked()
                {
                    cond.values.push(String::new());
                }
                if cond_count > 1
                    && icon_button(ui, true, "\u{2715}")
                        .on_hover_text(octa::i18n::t("ccol.remove_condition_hint"))
                        .clicked()
                {
                    cx.edit = Some(RuleEdit::RemoveCond(i, j));
                }
            }
        });
        k += 1;
    }
    if let Some(k) = remove_value.filter(|&k| k < cond.values.len()) {
        cond.values.remove(k);
    }
}

/// The drop-down beside a condition's first value: the chosen column's distinct
/// values, most common first, each with its count. Clicking one adds it to
/// the condition's values, clicking it again takes it out. The list is built the first time it opens and kept for the
/// dialog's lifetime.
///
/// ponytail: built on the UI thread and never refreshed while the dialog is
/// open. Move it to a worker if a multi-million-row column makes it stutter.
fn value_picker(
    ui: &mut egui::Ui,
    id: (usize, usize),
    cond: &mut CaseCond,
    uses_value: bool,
    table: &octa::data::DataTable,
    lists: &mut std::collections::HashMap<usize, ValueList>,
) {
    let enabled = uses_value && cond.cond_col.is_some();
    let disabled_hint = if uses_value {
        "ccol.pick_value_need_column"
    } else {
        "ccol.pick_value_no_value"
    };
    ui.add_enabled_ui(enabled, |ui| {
        let resp = egui::ComboBox::from_id_salt(("ccol_pick", id))
            .selected_text("")
            .width(control_h(ui))
            .height(320.0)
            // Stay open so several values can be picked in one go.
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show_ui(ui, |ui| {
                // The button is a narrow arrow; the list needs room for values.
                ui.set_min_width(VALUE_W);
                let Some(col) = cond.cond_col else {
                    return;
                };
                let list = lists.entry(col).or_insert_with(|| {
                    compute_value_frequency(table, col, Some(PICK_MAX_VALUES), BinningMode::None)
                        .map(|vf| ValueList {
                            values: vf.rows.into_iter().map(|r| (r.label, r.count)).collect(),
                            unique: vf.unique_count,
                        })
                        .unwrap_or_default()
                });
                if list.values.is_empty() {
                    ui.weak(octa::i18n::t("ccol.pick_value_empty"));
                }
                // A click adds the value (into the first blank field, else a
                // new one) or, when it is already there, takes it out again.
                for (value, count) in &list.values {
                    let chosen = cond.values.iter().position(|v| v == value);
                    if ui
                        .selectable_label(chosen.is_some(), format!("{value}  ({count})"))
                        .clicked()
                    {
                        match (chosen, cond.values.iter().position(|v| v.trim().is_empty())) {
                            (Some(k), _) if cond.values.len() > 1 => {
                                cond.values.remove(k);
                            }
                            (Some(k), _) => cond.values[k].clear(),
                            (None, Some(blank)) => cond.values[blank] = value.clone(),
                            (None, None) => cond.values.push(value.clone()),
                        }
                    }
                }
                if list.unique > list.values.len() {
                    ui.separator();
                    ui.weak(
                        octa::i18n::t("ccol.pick_value_capped")
                            .replace("{n}", &list.values.len().to_string())
                            .replace("{total}", &list.unique.to_string()),
                    );
                }
            })
            .response;
        resp.on_hover_text(octa::i18n::t("ccol.pick_value_hint"))
            .on_disabled_hover_text(octa::i18n::t(disabled_hint));
    });
}

/// A column's values for the value picker: (value, count), most common
/// first, and how many distinct values the column has in total.
#[derive(Default)]
pub(crate) struct ValueList {
    values: Vec<(String, usize)>,
    unique: usize,
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
    let usable_rules = st.rules.iter().any(|r| r.is_usable());
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
    // A chosen type goes through Change type, so values that do not fit keep
    // their text as problem cells (F10) and the status line says how many.
    if let Some(target) = st.output_type {
        app.run_retype(idx, target);
    }
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

#[cfg(test)]
mod tests {
    use super::{control_row, field, icon_button, keyword_cell};
    use eframe::egui;

    /// A combo, a text field, a glyph button and a keyword on one line must
    /// be the same height and share one vertical centre.
    #[test]
    fn every_control_on_a_line_has_one_height_and_centre() {
        let ctx = egui::Context::default();
        let mut rects = Vec::new();
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.spacing_mut().interact_size.y = super::control_h(ui);
                let mut v = String::new();
                rects = control_row(ui, |ui| {
                    let kw = keyword_cell(ui, 80.0, |ui| ui.label("If"));
                    let combo = egui::ComboBox::from_id_salt("c")
                        .selected_text("col")
                        .width(120.0)
                        .show_ui(ui, |_| {})
                        .response;
                    let edit = field(ui, 120.0, egui::TextEdit::singleline(&mut v));
                    let btn = icon_button(ui, true, "+");
                    vec![combo.rect, edit.rect, btn.rect, kw.rect]
                });
            });
            out.textures_delta.clear();
        }
        let (h, cy) = (rects[0].height(), rects[0].center().y);
        for r in &rects[..3] {
            assert!((r.height() - h).abs() < 0.5, "heights differ: {rects:?}");
        }
        for r in &rects {
            assert!((r.center().y - cy).abs() < 0.5, "centres differ: {rects:?}");
        }
    }
}
