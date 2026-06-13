//! Transform-column dialog (Edit -> Transform column...). OpenRefine-style
//! column shaping over the active tab, in place. The op selector picks one of
//! [`TransformOp`]; the op-specific widgets gather parameters; **Apply**
//! materialises the result through the pure functions in
//! [`octa::data::transform`].
//!
//! New columns are inserted via [`DataTable::insert_column`] and filled with
//! [`DataTable::set`]; in-place ops (Fill, Replace) overwrite cells via `set`.
//! Both push onto the table's undo stack, so the change is undoable (matching
//! the Insert-column dialog's behaviour).

use eframe::egui;
use egui::RichText;

use octa::data::SearchMode;
use octa::data::search::RowMatcher;
use octa::data::transform::{
    SplitSpec, extract_pattern, fill_down, fill_up, merge_columns, replace_in_column, split_column,
};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{OctaApp, SplitMode, TransformOp, TransformState};

pub(crate) fn render_transform_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.transform_dialog.is_none() {
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
    let mut st = app.transform_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_transform_dialog");
    let window = egui::Window::new("octa_transform")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(500.0)
            .default_height(380.0)
            .min_width(380.0)
            .min_height(200.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("transform_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("transform.title"))
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

        egui::Panel::bottom("transform_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("transform.apply")).clicked() {
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
            // Op selector.
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("transform.operation"));
                egui::ComboBox::from_id_salt("tr_op")
                    .selected_text(octa::i18n::t(st.op.i18n_key()))
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for &op in TransformOp::ALL {
                            if ui
                                .selectable_label(st.op == op, octa::i18n::t(op.i18n_key()))
                                .clicked()
                            {
                                st.op = op;
                                st.error = None;
                                // Defaults differ per op, so don't carry a
                                // name / position typed for the previous one.
                                st.new_name.clear();
                                st.insert_pos_text.clear();
                            }
                        }
                    });
            });
            ui.separator();
            op_body(ui, &mut st, &col_names);

            if st.op.creates_column() {
                new_column_controls(ui, &mut st, &col_names);
            }

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
        match apply_transform(app, &st) {
            Ok(()) => {
                // Success: close the dialog.
                return;
            }
            Err(e) => st.error = Some(e),
        }
    }
    if !close {
        app.transform_dialog = Some(st);
    }
}

/// Op-specific parameter widgets.
fn op_body(ui: &mut egui::Ui, st: &mut TransformState, cols: &[String]) {
    match st.op {
        TransformOp::Split => {
            ui.label(
                RichText::new(octa::i18n::t("transform.split_desc"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            source_col(ui, "tr_split_col", &mut st.col, cols);
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("transform.split_by"));
                egui::ComboBox::from_id_salt("tr_split_mode")
                    .selected_text(octa::i18n::t(st.split_mode.i18n_key()))
                    .show_ui(ui, |ui| {
                        for &m in SplitMode::ALL {
                            ui.selectable_value(&mut st.split_mode, m, octa::i18n::t(m.i18n_key()));
                        }
                    });
            });
            ui.horizontal(|ui| match st.split_mode {
                SplitMode::Delimiter => {
                    ui.label(octa::i18n::t("transform.delimiter"));
                    ui.add(egui::TextEdit::singleline(&mut st.split_delim).desired_width(120.0));
                }
                SplitMode::Regex => {
                    ui.label(octa::i18n::t("transform.pattern"));
                    ui.add(egui::TextEdit::singleline(&mut st.split_regex).desired_width(180.0));
                }
                SplitMode::FixedWidth => {
                    ui.label(octa::i18n::t("transform.width"));
                    ui.add(egui::TextEdit::singleline(&mut st.split_width).desired_width(60.0));
                }
            });
        }
        TransformOp::Merge => {
            ui.label(
                RichText::new(octa::i18n::t("transform.merge_desc"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            ui.label(RichText::new(octa::i18n::t("transform.merge_cols")).strong());
            multi_col_picker(ui, "tr_merge", &mut st.merge_cols, cols);
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("transform.separator"));
                ui.add(egui::TextEdit::singleline(&mut st.merge_sep).desired_width(80.0));
            });
        }
        TransformOp::FillDown | TransformOp::FillUp => {
            ui.label(
                RichText::new(octa::i18n::t("transform.fill_desc"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            source_col(ui, "tr_fill_col", &mut st.col, cols);
        }
        TransformOp::Extract => {
            ui.label(
                RichText::new(octa::i18n::t("transform.extract_desc"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            source_col(ui, "tr_extract_col", &mut st.col, cols);
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("transform.pattern"));
                ui.add(egui::TextEdit::singleline(&mut st.extract_pattern).desired_width(220.0));
            });
        }
        TransformOp::Replace => {
            ui.label(
                RichText::new(octa::i18n::t("transform.replace_desc"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            source_col(ui, "tr_replace_col", &mut st.col, cols);
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("transform.find"));
                ui.add(egui::TextEdit::singleline(&mut st.replace_query).desired_width(150.0));
                egui::ComboBox::from_id_salt("tr_replace_mode")
                    .selected_text(st.replace_mode.label_t())
                    .show_ui(ui, |ui| {
                        for m in [SearchMode::Plain, SearchMode::Wildcard, SearchMode::Regex] {
                            ui.selectable_value(&mut st.replace_mode, m, m.label_t());
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("transform.replace_with"));
                ui.add(egui::TextEdit::singleline(&mut st.replace_with).desired_width(150.0));
            });
        }
    }
}

/// Single source-column dropdown.
fn source_col(ui: &mut egui::Ui, id: &str, sel: &mut Option<usize>, cols: &[String]) {
    ui.horizontal(|ui| {
        ui.label(octa::i18n::t("transform.column"));
        let text = sel
            .and_then(|i| cols.get(i).cloned())
            .unwrap_or_else(|| octa::i18n::t("transform.pick"));
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
    });
}

/// Ordered checkbox picker (preserves pick order), bounded height.
fn multi_col_picker(ui: &mut egui::Ui, id: &str, sel: &mut Vec<usize>, cols: &[String]) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id)
            .auto_shrink([false, true])
            .max_height(180.0)
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

/// Name + insert-position widgets for the column-creating ops (Split / Merge /
/// Extract). Both are optional: an empty name / position falls back to the
/// op's auto default, shown as the field's hint text.
fn new_column_controls(ui: &mut egui::Ui, st: &mut TransformState, cols: &[String]) {
    ui.add_space(6.0);
    ui.separator();

    // Output column name.
    let name_hint = default_new_name(st, cols);
    ui.horizontal(|ui| {
        ui.label(octa::i18n::t("transform.new_name"));
        ui.add(
            egui::TextEdit::singleline(&mut st.new_name)
                .desired_width(180.0)
                .hint_text(name_hint),
        );
    });
    if st.op == TransformOp::Split {
        ui.label(
            RichText::new(octa::i18n::t("transform.split_name_hint"))
                .size(10.0)
                .color(ui.visuals().weak_text_color()),
        );
    }

    // Insert position (1-based), mirroring the Insert-column dialog.
    let col_count = cols.len();
    let default_pos = default_insert_index(st, cols) + 1;
    ui.horizontal(|ui| {
        ui.label(octa::i18n::t("transform.position"));
        let buf_empty = st.insert_pos_text.is_empty();
        let valid = buf_empty
            || st
                .insert_pos_text
                .trim()
                .parse::<usize>()
                .is_ok_and(|v| (1..=col_count + 1).contains(&v));
        let mut te = egui::TextEdit::singleline(&mut st.insert_pos_text)
            .desired_width(48.0)
            .hint_text(default_pos.to_string());
        if !valid {
            te = te.text_color(egui::Color32::from_rgb(220, 80, 80));
        }
        ui.add(te);
        ui.label(format!("/ {}", col_count + 1));
    });
}

/// The auto default output name for the current op (also used as the name
/// field's hint text). For Split it shows the first generated column.
fn default_new_name(st: &TransformState, cols: &[String]) -> String {
    let src = st.col.and_then(|i| cols.get(i)).map(|s| s.as_str());
    match st.op {
        TransformOp::Merge => "merged".to_string(),
        TransformOp::Extract => format!("{}_extracted", src.unwrap_or("column")),
        TransformOp::Split => format!("{}_1", src.unwrap_or("column")),
        _ => String::new(),
    }
}

/// The 0-based insert index used when the position field is left blank:
/// after the source column for Split / Extract, at the end for Merge.
fn default_insert_index(st: &TransformState, cols: &[String]) -> usize {
    match st.op {
        TransformOp::Merge => cols.len(),
        _ => st.col.map(|i| i + 1).unwrap_or(cols.len()),
    }
}

/// Resolve the position field to a 0-based insert index, falling back to
/// `default_idx` when the buffer is empty or out of range.
fn resolve_insert_index(text: &str, default_idx: usize, col_count: usize) -> usize {
    text.trim()
        .parse::<usize>()
        .ok()
        .filter(|v| (1..=col_count + 1).contains(v))
        .map(|v| v - 1)
        .unwrap_or(default_idx)
}

/// Make `base` unique against the existing column names by appending `_2`,
/// `_3`, ... when needed.
fn unique_name(cols: &[String], base: &str) -> String {
    if !cols.iter().any(|c| c == base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if !cols.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Apply the configured transform to the active tab. Returns a user-facing
/// error string (already localized) on bad input.
fn apply_transform(app: &mut OctaApp, st: &TransformState) -> Result<(), String> {
    if app.is_readonly() {
        return Err(octa::i18n::t("transform.readonly"));
    }
    let active = app.active_tab;
    let col_names: Vec<String> = app.tabs[active]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();

    match st.op {
        TransformOp::Split => {
            let col = st
                .col
                .ok_or_else(|| octa::i18n::t("transform.need_column"))?;
            let spec = match st.split_mode {
                SplitMode::Delimiter => {
                    if st.split_delim.is_empty() {
                        return Err(octa::i18n::t("transform.need_delimiter"));
                    }
                    SplitSpec::Delimiter(st.split_delim.clone())
                }
                SplitMode::Regex => SplitSpec::Regex(st.split_regex.clone()),
                SplitMode::FixedWidth => {
                    let w: usize = st
                        .split_width
                        .trim()
                        .parse()
                        .ok()
                        .filter(|&w| w > 0)
                        .ok_or_else(|| octa::i18n::t("transform.need_width"))?;
                    SplitSpec::FixedWidth(w)
                }
            };
            let out = split_column(&app.tabs[active].table, col, &spec)
                .map_err(|e| format!("{}: {e}", octa::i18n::t("transform.failed")))?;
            let base = st.new_name.trim();
            let start = resolve_insert_index(&st.insert_pos_text, col + 1, col_names.len());
            // Uniquify against a growing list so several new columns can't
            // collide with each other (or with the existing names).
            let mut taken = col_names.clone();
            let tbl = &mut app.tabs[active].table;
            for (offset, (auto_name, values)) in out.into_iter().enumerate() {
                let proposed = if base.is_empty() {
                    auto_name
                } else {
                    format!("{base}_{}", offset + 1)
                };
                let name = unique_name(&taken, &proposed);
                taken.push(name.clone());
                let idx = start + offset;
                tbl.insert_column(idx, name, "Utf8".to_string());
                for (r, v) in values.into_iter().enumerate() {
                    tbl.set(r, idx, v);
                }
            }
        }
        TransformOp::Merge => {
            if st.merge_cols.len() < 2 {
                return Err(octa::i18n::t("transform.need_two_cols"));
            }
            let values = merge_columns(&app.tabs[active].table, &st.merge_cols, &st.merge_sep);
            let base = st.new_name.trim();
            let name = unique_name(&col_names, if base.is_empty() { "merged" } else { base });
            let idx = resolve_insert_index(&st.insert_pos_text, col_names.len(), col_names.len());
            let tbl = &mut app.tabs[active].table;
            tbl.insert_column(idx, name, "Utf8".to_string());
            for (r, v) in values.into_iter().enumerate() {
                tbl.set(r, idx, v);
            }
        }
        TransformOp::FillDown | TransformOp::FillUp => {
            let col = st
                .col
                .ok_or_else(|| octa::i18n::t("transform.need_column"))?;
            let values = if st.op == TransformOp::FillDown {
                fill_down(&app.tabs[active].table, col)
            } else {
                fill_up(&app.tabs[active].table, col)
            };
            let tbl = &mut app.tabs[active].table;
            for (r, v) in values.into_iter().enumerate() {
                tbl.set(r, col, v);
            }
        }
        TransformOp::Extract => {
            let col = st
                .col
                .ok_or_else(|| octa::i18n::t("transform.need_column"))?;
            if st.extract_pattern.trim().is_empty() {
                return Err(octa::i18n::t("transform.need_pattern"));
            }
            let re = regex::Regex::new(&st.extract_pattern)
                .map_err(|e| format!("{}: {e}", octa::i18n::t("transform.bad_regex")))?;
            let values = extract_pattern(&app.tabs[active].table, col, &re);
            let typed = st.new_name.trim();
            let base = if typed.is_empty() {
                format!("{}_extracted", col_names[col])
            } else {
                typed.to_string()
            };
            let name = unique_name(&col_names, &base);
            let idx = resolve_insert_index(&st.insert_pos_text, col + 1, col_names.len());
            let tbl = &mut app.tabs[active].table;
            tbl.insert_column(idx, name, "Utf8".to_string());
            for (r, v) in values.into_iter().enumerate() {
                tbl.set(r, idx, v);
            }
        }
        TransformOp::Replace => {
            let col = st
                .col
                .ok_or_else(|| octa::i18n::t("transform.need_column"))?;
            if st.replace_query.is_empty() {
                return Err(octa::i18n::t("transform.need_find"));
            }
            let matcher = RowMatcher::new(&st.replace_query, st.replace_mode);
            if matches!(matcher, RowMatcher::Invalid) {
                return Err(octa::i18n::t("transform.bad_regex"));
            }
            let values =
                replace_in_column(&app.tabs[active].table, col, &matcher, &st.replace_with);
            let tbl = &mut app.tabs[active].table;
            for (r, v) in values.into_iter().enumerate() {
                tbl.set(r, col, v);
            }
        }
    }

    app.tabs[active].table_state.widths_initialized = false;
    app.tabs[active].filter_dirty = true;
    Ok(())
}
