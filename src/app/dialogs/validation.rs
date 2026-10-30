//! Data-validation dialog. Edits the active tab's session-scoped list of
//! [`ValidationRule`]s. Cells that fail a rule are painted red by the table
//! renderer (the violation set is recomputed in `recompute_filter`, see
//! `octa::data::validation`). Rules apply live: any change marks the tab
//! `filter_dirty` so the violation set is rebuilt, so there is no Apply step.

use eframe::egui;
use egui::RichText;

use octa::data::DataTable;
use octa::data::conditional_format::CondOp;
use octa::data::validation::rules_file;
use octa::data::validation::{ValidationKind, ValidationRule};
use octa::ui::settings::{
    DialogSize, dialog_header, draw_result_message, remember_dialog_rect, size_dialog_window,
};

/// Where the last Save / Load outcome is parked between frames. The dialog has
/// no state struct of its own (its rules live on the tab), so the message rides
/// in egui's temp memory, the same trick the dialog size uses.
fn msg_id() -> egui::Id {
    egui::Id::new("octa_validation_rules_msg")
}

/// Which rule's value list is open (`Some(index)`), parked between frames.
fn one_of_id() -> egui::Id {
    egui::Id::new("octa_validation_one_of_open")
}

/// The value list's Find and Add fields, parked the same way.
fn find_id() -> egui::Id {
    egui::Id::new("octa_validation_one_of_find")
}

fn add_id() -> egui::Id {
    egui::Id::new("octa_validation_one_of_add")
}

/// How many different values the picker lists before it gives up.
const MAX_LISTED: usize = 1000;

/// A column's values for the picker, most common first.
#[derive(Clone)]
struct ValueList {
    values: Vec<(String, usize)>,
    unique: usize,
    /// The table holds only some of its rows (a database tab still paging).
    partial: bool,
}

/// The values of `col`, recomputed only when the table changes.
fn value_list(ctx: &egui::Context, table: &DataTable, col: usize) -> ValueList {
    use octa::data::value_frequency::{BinningMode, compute_value_frequency};
    // The rows' address tells two tabs apart whose counts happen to match.
    let key = (
        table.rows.as_ptr() as usize,
        col,
        table.undo_stack.len(),
        table.edits.len(),
        table.row_count(),
    );
    let id = egui::Id::new("octa_validation_value_list");
    if let Some((k, list)) =
        ctx.data(|d| d.get_temp::<((usize, usize, usize, usize, usize), ValueList)>(id))
        && k == key
    {
        return list;
    }
    let vf = compute_value_frequency(table, col, Some(MAX_LISTED + 1), BinningMode::None);
    let list = ValueList {
        values: vf
            .as_ref()
            .map(|v| v.rows.iter().map(|r| (r.label.clone(), r.count)).collect())
            .unwrap_or_default(),
        unique: vf.map_or(0, |v| v.unique_count),
        partial: table.total_rows.is_some_and(|t| t > table.row_count()),
    };
    ctx.data_mut(|d| d.insert_temp(id, (key, list.clone())));
    list
}

use super::super::state::OctaApp;

pub(crate) fn render_validation_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.tabs[app.active_tab].show_validation {
        return;
    }

    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    // Range bounds on these columns are clock times or durations (in seconds).
    let time_cols: Vec<bool> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| octa::data::times::TimeKind::of_type(&c.data_type).is_some())
        .collect();

    let mut rules = app.tabs[app.active_tab].validation_rules.clone();
    let violation_count = app.tabs[app.active_tab].validation_violations.len();
    let mut close_requested = false;
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;
    let mut save_requested = false;
    let mut load_requested = false;
    // Rules are stored by column NAME, so both directions need the real
    // column list, not just the names used by the pickers below.
    let columns = app.tabs[app.active_tab].table.columns.clone();
    let rules_msg: Option<(bool, String)> = ctx.data(|d| d.get_temp(msg_id()));
    let mut size = app.tabs[app.active_tab].validation_size;
    let minimized = size == DialogSize::Minimized;

    let editing: Option<usize> = ctx
        .data(|d| d.get_temp::<Option<usize>>(one_of_id()))
        .flatten();
    let picker: Option<(usize, usize, ValueList)> = editing.and_then(|i| {
        let rule = rules.get(i)?;
        match (&rule.kind, rule.column) {
            (ValidationKind::OneOf(_), Some(c)) => {
                Some((i, c, value_list(ctx, &app.tabs[app.active_tab].table, c)))
            }
            _ => None,
        }
    });

    let dialog_id = egui::Id::new("octa_validation_dialog");
    let window = egui::Window::new("octa_validation")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(620.0)
            .default_height(400.0)
            .min_width(480.0)
            .min_height(200.0)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "val_header",
            RichText::new(octa::i18n::t("dialog.val_title")),
            &mut size,
        ) {
            close_requested = true;
        }

        if minimized {
            return;
        }

        egui::Panel::bottom("val_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                octa::ui::control_row::control_line(ui, |ui| {
                    if ui.button(octa::i18n::t("dialog.cnf_add_rule")).clicked() {
                        rules.push(ValidationRule {
                            column: None,
                            kind: ValidationKind::NotNull,
                        });
                        changed = true;
                    }
                    if !rules.is_empty()
                        && ui.button(octa::i18n::t("dialog.cnf_clear_all")).clicked()
                    {
                        rules.clear();
                        changed = true;
                    }
                    if !rules.is_empty()
                        && ui
                            .button(octa::i18n::t("rules.save"))
                            .on_hover_text(octa::i18n::t("rules.save_hint"))
                            .clicked()
                    {
                        save_requested = true;
                    }
                    if ui
                        .button(octa::i18n::t("rules.load"))
                        .on_hover_text(octa::i18n::t("rules.load_hint"))
                        .clicked()
                    {
                        load_requested = true;
                    }
                    // Live violation count.
                    if !rules.is_empty() {
                        ui.label(
                            RichText::new(
                                octa::i18n::t("dialog.val_violations")
                                    .replace("{n}", &violation_count.to_string()),
                            )
                            .size(11.0)
                            .color(ui.visuals().weak_text_color()),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("common.close")).clicked() {
                            close_requested = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("dialog.val_desc"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // A grid, not one `horizontal` per rule: `ComboBox::width`
                    // is a desired width, not a cap, so a long column name
                    // widens its button and pushes everything after it right
                    // on that row alone. A grid sizes each column to the
                    // widest row, so the pickers line up.
                    egui::Grid::new("validation_rules")
                        .num_columns(4)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            octa::ui::control_row::pin_control_height(ui);
                            for (i, rule) in rules.iter_mut().enumerate() {
                                {
                                    // Column picker: "(any)" or a specific column.
                                    let col_label = match rule.column {
                                        None => octa::i18n::t("dialog.cnf_any_column"),
                                        Some(c) => col_names
                                            .get(c)
                                            .cloned()
                                            .unwrap_or_else(|| format!("col {c}")),
                                    };
                                    egui::ComboBox::from_id_salt(("val_col", i))
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

                                    // Kind picker.
                                    let kind_combo = egui::ComboBox::from_id_salt(("val_kind", i))
                                        .selected_text(octa::i18n::t(rule.kind.i18n_key()))
                                        .width(150.0)
                                        .show_ui(ui, |ui| {
                                            for kind in ValidationKind::all() {
                                                let selected = rule.kind.same_variant(&kind);
                                                let mut item = ui.selectable_label(
                                                    selected,
                                                    octa::i18n::t(kind.i18n_key()),
                                                );
                                                if let Some(h) = kind.hint_key() {
                                                    item = item.on_hover_text(octa::i18n::t(h));
                                                }
                                                if item.clicked() && !selected {
                                                    rule.kind = kind;
                                                    changed = true;
                                                }
                                            }
                                        });
                                    if let Some(h) = rule.kind.hint_key() {
                                        kind_combo.response.on_hover_text(octa::i18n::t(h));
                                    }

                                    // Per-kind parameter widgets, kept in one grid
                                    // cell so a rule with two of them (Range) does not
                                    // shift the remove button out of its column.
                                    octa::ui::control_row::control_line(ui, |ui| {
                                        if kind_params(ui, i, rule, &col_names, &time_cols) {
                                            changed = true;
                                        }
                                    });

                                    // `add_sized` with the standard interact
                                    // height, not `small_button`: a small
                                    // button is shorter than the combo boxes
                                    // beside it and the row looks ragged.
                                    if ui
                                        .add_sized(
                                            [24.0, ui.spacing().interact_size.y],
                                            egui::Button::new("X"),
                                        )
                                        .on_hover_text(octa::i18n::t("dialog.cnf_remove"))
                                        .clicked()
                                    {
                                        remove_idx = Some(i);
                                    }
                                    ui.end_row();
                                }
                            }
                        });
                    if let Some((i, col, list)) = &picker
                        && let Some(ValidationKind::OneOf(values)) =
                            rules.get_mut(*i).map(|r| &mut r.kind)
                    {
                        ui.separator();
                        if one_of_panel(
                            ui,
                            col_names.get(*col).map_or("", |s| s.as_str()),
                            list,
                            values,
                        ) {
                            changed = true;
                        }
                    }
                    if let Some((ok, msg)) = &rules_msg {
                        ui.add_space(6.0);
                        draw_result_message(ui, *ok, msg);
                    }
                    if rules.is_empty() {
                        ui.label(
                            RichText::new(octa::i18n::t("dialog.val_empty"))
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
    app.tabs[app.active_tab].validation_size = size;

    if save_requested
        && let Some(path) = rfd::FileDialog::new()
            .add_filter("TOML", &["toml"])
            .set_file_name("rules.toml")
            .save_file()
    {
        let file = rules_file::to_named(&rules, &columns);
        let msg = match rules_file::save(&path, &file) {
            Ok(()) => (
                true,
                octa::i18n::t("rules.saved").replace("{path}", &path.display().to_string()),
            ),
            Err(e) => (false, format!("{e:#}")),
        };
        ctx.data_mut(|d| d.insert_temp(msg_id(), msg));
    }
    if load_requested
        && let Some(path) = rfd::FileDialog::new()
            .add_filter("TOML", &["toml"])
            .pick_file()
    {
        let msg = match rules_file::load(&path) {
            Ok(file) => {
                let (resolved, unknown) = rules_file::resolve(&file, &columns);
                let n = resolved.len();
                rules = resolved;
                changed = true;
                if unknown.is_empty() {
                    (
                        true,
                        octa::i18n::t("rules.loaded").replace("{n}", &n.to_string()),
                    )
                } else {
                    // Naming what could not be applied, rather than dropping it
                    // silently: a rules file whose columns were renamed would
                    // otherwise look like it still checks everything.
                    (
                        false,
                        octa::i18n::t("rules.unknown_columns")
                            .replace("{n}", &unknown.len().to_string())
                            .replace("{names}", &unknown.join(", ")),
                    )
                }
            }
            Err(e) => (false, format!("{e:#}")),
        };
        ctx.data_mut(|d| d.insert_temp(msg_id(), msg));
    }

    if let Some(i) = remove_idx
        && i < rules.len()
    {
        rules.remove(i);
        // Indices shift: close any open value list rather than point at another rule.
        ctx.data_mut(|d| d.insert_temp(one_of_id(), None::<usize>));
        changed = true;
    }

    if changed {
        app.tabs[app.active_tab].validation_rules = rules;
        // Rebuild the violation set (recompute_filter does it when dirty).
        app.tabs[app.active_tab].filter_dirty = true;
        ctx.request_repaint();
    }
    if close_requested {
        app.tabs[app.active_tab].show_validation = false;
        // The picker's state is shared by every tab's dialog: leave none behind.
        ctx.data_mut(|d| {
            d.remove_temp::<Option<usize>>(one_of_id());
            d.remove_temp::<String>(find_id());
            d.remove_temp::<String>(add_id());
        });
    }
}

/// Parameter widgets for one rule. Returns whether a parameter changed.
fn kind_params(
    ui: &mut egui::Ui,
    idx: usize,
    rule: &mut ValidationRule,
    col_names: &[String],
    time_cols: &[bool],
) -> bool {
    let mut changed = false;
    let has_column = rule.column.is_some();
    let time = rule.column.is_some_and(|c| time_cols.get(c) == Some(&true));
    match &mut rule.kind {
        ValidationKind::NotNull | ValidationKind::Unique | ValidationKind::Id(_) => {
            // No parameters.
        }
        ValidationKind::Range { min, max } => {
            ui.label(octa::i18n::t("dialog.val_min"));
            changed |= opt_f64_field(ui, ui.id().with(("val_min", idx, time)), min, time);
            ui.label(octa::i18n::t("dialog.val_max"));
            changed |= opt_f64_field(ui, ui.id().with(("val_max", idx, time)), max, time);
        }
        ValidationKind::Regex(pattern) => {
            if ui
                .add(
                    egui::TextEdit::singleline(pattern)
                        .desired_width(160.0)
                        .hint_text(octa::i18n::t("dialog.val_pattern")),
                )
                .changed()
            {
                changed = true;
            }
        }
        ValidationKind::MaxLength(max) => {
            changed |= usize_field(ui, ui.id().with(("val_len", idx)), max);
        }
        ValidationKind::CompareColumn { op, other } => {
            if !has_column {
                ui.label(
                    egui::RichText::new(octa::i18n::t("valrule.needs_column"))
                        .weak()
                        .size(11.0),
                );
            }
            egui::ComboBox::from_id_salt(("val_cmp_op", idx))
                .selected_text(op.label_t())
                .width(120.0)
                .show_ui(ui, |ui| {
                    for o in CondOp::COMPARISONS {
                        if ui.selectable_label(*op == *o, o.label_t()).clicked() {
                            *op = *o;
                            changed = true;
                        }
                    }
                })
                .response
                .on_hover_text(octa::i18n::t("valrule.cmp_op_hint"));
            egui::ComboBox::from_id_salt(("val_cmp_other", idx))
                .selected_text(col_names.get(*other).cloned().unwrap_or_default())
                .width(120.0)
                .show_ui(ui, |ui| {
                    for (c, name) in col_names.iter().enumerate() {
                        if ui.selectable_label(*other == c, name).clicked() {
                            *other = c;
                            changed = true;
                        }
                    }
                })
                .response
                .on_hover_text(octa::i18n::t("valrule.cmp_other_hint"));
        }
        ValidationKind::OneOf(values) => {
            let label = octa::i18n::t("valrule.values").replace("{n}", &values.len().to_string());
            let button = ui.add_enabled(has_column, egui::Button::new(label));
            if button
                .on_hover_text(octa::i18n::t("valrule.values_hint"))
                .on_disabled_hover_text(octa::i18n::t("valrule.needs_column"))
                .clicked()
            {
                ui.data_mut(|d| d.insert_temp(one_of_id(), Some(idx)));
            }
        }
    }
    changed
}

/// The tick list for a "One of these values" rule. Returns whether the
/// allowed values changed.
fn one_of_panel(
    ui: &mut egui::Ui,
    column: &str,
    list: &ValueList,
    values: &mut Vec<String>,
) -> bool {
    let mut changed = false;
    let (find_id, add_id) = (find_id(), add_id());
    let mut find: String = ui.data(|d| d.get_temp(find_id)).unwrap_or_default();
    let mut add: String = ui.data(|d| d.get_temp(add_id)).unwrap_or_default();

    octa::ui::control_row::control_line(ui, |ui| {
        ui.label(
            egui::RichText::new(octa::i18n::t("valrule.values_title").replace("{column}", column))
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(octa::i18n::t("valrule.done"))
                .on_hover_text(octa::i18n::t("valrule.done_hint"))
                .clicked()
            {
                ui.data_mut(|d| d.insert_temp(one_of_id(), None::<usize>));
            }
        });
    });
    if list.partial {
        ui.label(
            egui::RichText::new(octa::i18n::t("valrule.loaded_only"))
                .weak()
                .size(11.0),
        );
    }
    if list.unique > MAX_LISTED {
        ui.label(
            egui::RichText::new(
                octa::i18n::t("valrule.too_many").replace("{n}", &list.unique.to_string()),
            )
            .weak()
            .size(11.0),
        );
    } else {
        octa::ui::control_row::control_text_edit(
            ui,
            220.0,
            egui::TextEdit::singleline(&mut find).hint_text(octa::i18n::t("valrule.find")),
        )
        .on_hover_text(octa::i18n::t("valrule.find_hint"));
        let needle = find.trim().to_lowercase();
        egui::ScrollArea::vertical()
            .id_salt("octa_validation_one_of_list")
            .max_height(220.0)
            .show(ui, |ui| {
                for (value, count) in &list.values {
                    if !needle.is_empty() && !value.to_lowercase().contains(&needle) {
                        continue;
                    }
                    let mut on = values.contains(value);
                    if ui
                        .checkbox(&mut on, format!("{value}  ({count})"))
                        .changed()
                    {
                        if on {
                            values.push(value.clone());
                        } else {
                            values.retain(|v| v != value);
                        }
                        changed = true;
                    }
                }
                // Allowed values the data does not hold (added by hand, or
                // from a rules file) stay visible so they can be unticked.
                let extra: Vec<String> = values
                    .iter()
                    .filter(|v| !list.values.iter().any(|(d, _)| d == *v))
                    .cloned()
                    .collect();
                for value in extra {
                    let mut on = true;
                    let label = format!("{value}  ({})", octa::i18n::t("valrule.not_in_data"));
                    if ui.checkbox(&mut on, label).changed() && !on {
                        values.retain(|v| *v != value);
                        changed = true;
                    }
                }
            });
    }
    octa::ui::control_row::control_line(ui, |ui| {
        octa::ui::control_row::control_text_edit(
            ui,
            180.0,
            egui::TextEdit::singleline(&mut add).hint_text(octa::i18n::t("valrule.add_value")),
        )
        .on_hover_text(octa::i18n::t("valrule.add_value_hint"));
        let ready = !add.is_empty() && !values.contains(&add);
        if ui
            .add_enabled(ready, egui::Button::new(octa::i18n::t("valrule.add")))
            .on_hover_text(octa::i18n::t("valrule.add_value_hint"))
            .clicked()
        {
            values.push(std::mem::take(&mut add));
            changed = true;
        }
    });
    ui.data_mut(|d| {
        d.insert_temp(find_id, find);
        d.insert_temp(add_id, add);
    });
    changed
}

/// An optional-number text field backed by an egui temp-memory buffer so
/// in-progress text (e.g. a lone "-") survives across frames. Empty clears the
/// bound to `None`.
/// With `time`, the value is seconds, typed and shown as a clock time or
/// duration (`08:30`, `26:00:00`); a plain number still reads as seconds.
fn opt_f64_field(ui: &mut egui::Ui, id: egui::Id, value: &mut Option<f64>, time: bool) -> bool {
    use octa::data::times::{DurationStyle, format_duration, read_time_text};
    let shown = |v: f64| {
        if time {
            format_duration((v * 1e9).round() as i64, DurationStyle::Clock)
        } else {
            v.to_string()
        }
    };
    let mut buf: String = ui
        .data(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| value.map(shown).unwrap_or_default());
    let resp = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(70.0));
    let mut changed = false;
    if resp.changed() {
        let trimmed = buf.trim();
        *value = if trimmed.is_empty() {
            None
        } else {
            trimmed.parse::<f64>().ok().or_else(|| {
                time.then(|| read_time_text(trimmed).map(|ns| ns as f64 / 1e9))
                    .flatten()
            })
        };
        changed = true;
    }
    ui.data_mut(|d| d.insert_temp(id, buf));
    changed
}

/// A non-negative integer text field backed by a temp-memory buffer.
fn usize_field(ui: &mut egui::Ui, id: egui::Id, value: &mut usize) -> bool {
    let mut buf: String = ui
        .data(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| value.to_string());
    let resp = ui.add(
        egui::TextEdit::singleline(&mut buf)
            .desired_width(60.0)
            .hint_text(octa::i18n::t("dialog.val_max")),
    );
    let mut changed = false;
    if resp.changed() {
        if let Ok(n) = buf.trim().parse::<usize>() {
            *value = n;
            changed = true;
        } else if buf.trim().is_empty() {
            *value = 0;
            changed = true;
        }
    }
    ui.data_mut(|d| d.insert_temp(id, buf));
    changed
}
