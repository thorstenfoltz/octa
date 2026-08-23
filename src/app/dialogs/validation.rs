//! Data-validation dialog. Edits the active tab's session-scoped list of
//! [`ValidationRule`]s. Cells that fail a rule are painted red by the table
//! renderer (the violation set is recomputed in `recompute_filter`, see
//! `octa::data::validation`). Rules apply live: any change marks the tab
//! `filter_dirty` so the violation set is rebuilt, so there is no Apply step.

use eframe::egui;
use egui::RichText;

use octa::data::validation::rules_file;
use octa::data::validation::{ValidationKind, ValidationRule};
use octa::ui::settings::{
    DialogSize, draw_result_message, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

/// Where the last Save / Load outcome is parked between frames. The dialog has
/// no state struct of its own (its rules live on the tab), so the message rides
/// in egui's temp memory, the same trick the dialog size uses.
fn msg_id() -> egui::Id {
    egui::Id::new("octa_validation_rules_msg")
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
        egui::Panel::top("val_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.val_title"))
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

        egui::Panel::bottom("val_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
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
                                    egui::ComboBox::from_id_salt(("val_kind", i))
                                        .selected_text(octa::i18n::t(rule.kind.i18n_key()))
                                        .width(150.0)
                                        .show_ui(ui, |ui| {
                                            for kind in ValidationKind::all() {
                                                let selected = rule.kind.same_variant(&kind);
                                                if ui
                                                    .selectable_label(
                                                        selected,
                                                        octa::i18n::t(kind.i18n_key()),
                                                    )
                                                    .clicked()
                                                    && !selected
                                                {
                                                    rule.kind = kind;
                                                    changed = true;
                                                }
                                            }
                                        });

                                    // Per-kind parameter widgets, kept in one grid
                                    // cell so a rule with two of them (Range) does not
                                    // shift the remove button out of its column.
                                    ui.horizontal(|ui| {
                                        if kind_params(ui, i, &mut rule.kind) {
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
    }
}

/// Parameter widgets for one rule's kind. Returns whether a parameter changed.
fn kind_params(ui: &mut egui::Ui, idx: usize, kind: &mut ValidationKind) -> bool {
    let mut changed = false;
    match kind {
        ValidationKind::NotNull | ValidationKind::Unique => {
            // No parameters.
        }
        ValidationKind::Range { min, max } => {
            ui.label(octa::i18n::t("dialog.val_min"));
            changed |= opt_f64_field(ui, ui.id().with(("val_min", idx)), min);
            ui.label(octa::i18n::t("dialog.val_max"));
            changed |= opt_f64_field(ui, ui.id().with(("val_max", idx)), max);
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
    }
    changed
}

/// An optional-number text field backed by an egui temp-memory buffer so
/// in-progress text (e.g. a lone "-") survives across frames. Empty clears the
/// bound to `None`.
fn opt_f64_field(ui: &mut egui::Ui, id: egui::Id, value: &mut Option<f64>) -> bool {
    let mut buf: String = ui
        .data(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| value.map(|v| v.to_string()).unwrap_or_default());
    let resp = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(70.0));
    let mut changed = false;
    if resp.changed() {
        let trimmed = buf.trim();
        *value = if trimmed.is_empty() {
            None
        } else {
            trimmed.parse::<f64>().ok()
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
