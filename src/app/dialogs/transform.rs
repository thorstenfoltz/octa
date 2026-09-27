//! Transform-column dialog (Edit -> Transform column...). OpenRefine-style
//! column shaping over the active tab, in place. The op selector picks one of
//! [`TransformOp`]; the op-specific widgets gather parameters; **Apply**
//! materialises the result through the pure functions in
//! [`octa::data::transform`].
//!
//! Apply builds the matching recipe step (`octa::data::recipe`) and runs it,
//! so a replayed recipe does exactly what this dialog did: same new-column
//! names, same positions. The step mutates through `insert_column` / `set`,
//! and the whole transform folds into one undo entry.

use eframe::egui;
use egui::RichText;

use octa::data::DataTable;
use octa::data::SearchMode;
use octa::data::id_checks::IdKind;
use octa::data::recipe::{
    Extract, Fill, Merge, RecipeStep, RepairEncoding, Replace, Split, TidyId,
};
use octa::data::search::RowMatcher;
use octa::data::validation::ValidationKind;
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
            .show(ui, |ui| {
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
            .show(ui, |ui| {
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

        egui::CentralPanel::default().show(ui, |ui| {
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
            op_body(ui, &mut st, &col_names, &app.tabs[app.active_tab].table);

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
fn op_body(ui: &mut egui::Ui, st: &mut TransformState, cols: &[String], table: &DataTable) {
    match st.op {
        TransformOp::TidyId => {
            ui.label(
                RichText::new(octa::i18n::t("transform.tidy_desc"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            source_col(ui, "tr_tidy_col", &mut st.col, cols);
            // The kind names and hints are Data validation's, so the two
            // features never describe the same check in different words.
            let label = |k: IdKind| octa::i18n::t(ValidationKind::Id(k).i18n_key());
            let hint = |k: IdKind| {
                ValidationKind::Id(k)
                    .hint_key()
                    .map(octa::i18n::t)
                    .unwrap_or_default()
            };
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("transform.tidy_kind"));
                egui::ComboBox::from_id_salt("tr_tidy_kind")
                    .selected_text(label(st.tidy_kind))
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for k in IdKind::ALL {
                            ui.selectable_value(&mut st.tidy_kind, k, label(k))
                                .on_hover_text(hint(k));
                        }
                    })
                    .response
                    .on_hover_text(hint(st.tidy_kind));
            });
            if let Some(c) = st.col.filter(|&c| c < table.col_count()) {
                let changes = TidyId::changes(table, c, st.tidy_kind).len();
                let invalid = (0..table.row_count())
                    .filter(|&r| {
                        let v = table.get(r, c).map(|v| v.to_string()).unwrap_or_default();
                        !v.trim().is_empty() && !st.tidy_kind.check(&v)
                    })
                    .count();
                ui.add_space(4.0);
                ui.label(
                    octa::i18n::t("transform.tidy_count")
                        .replace("{changed}", &changes.to_string())
                        .replace("{invalid}", &invalid.to_string()),
                );
            }
        }
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
        TransformOp::RepairEncoding => {
            ui.label(
                RichText::new(octa::i18n::t("transform.repair_encoding_desc"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            source_col(ui, "tr_repair_col", &mut st.col, cols);
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

/// The user's 1-based "insert at" text as a position, or `None` for the
/// op's default. Out-of-range values also fall back, inside the step.
fn typed_position(text: &str) -> Option<usize> {
    text.trim().parse::<usize>().ok()
}

/// The recipe step this dialog state describes, by column name. Validation
/// that needs a localized message happens here; everything else is the
/// step's job, so a replay behaves exactly like this dialog.
fn transform_step(st: &TransformState, col_names: &[String]) -> Result<RecipeStep, String> {
    let column = || {
        st.col
            .and_then(|c| col_names.get(c).cloned())
            .ok_or_else(|| octa::i18n::t("transform.need_column"))
    };
    let new_name = st.new_name.trim().to_string();
    let position = typed_position(&st.insert_pos_text);
    Ok(match st.op {
        TransformOp::Split => {
            let (by, value) = match st.split_mode {
                SplitMode::Delimiter => {
                    if st.split_delim.is_empty() {
                        return Err(octa::i18n::t("transform.need_delimiter"));
                    }
                    ("delimiter", st.split_delim.clone())
                }
                SplitMode::Regex => ("regex", st.split_regex.clone()),
                SplitMode::FixedWidth => {
                    if !st.split_width.trim().parse::<usize>().is_ok_and(|w| w > 0) {
                        return Err(octa::i18n::t("transform.need_width"));
                    }
                    ("width", st.split_width.trim().to_string())
                }
            };
            RecipeStep::Split(Split {
                column: column()?,
                by: by.to_string(),
                value,
                new_name,
                position,
            })
        }
        TransformOp::Merge => {
            if st.merge_cols.len() < 2 {
                return Err(octa::i18n::t("transform.need_two_cols"));
            }
            RecipeStep::Merge(Merge {
                columns: st
                    .merge_cols
                    .iter()
                    .filter_map(|&c| col_names.get(c).cloned())
                    .collect(),
                separator: st.merge_sep.clone(),
                new_name,
                position,
            })
        }
        TransformOp::RepairEncoding => {
            RecipeStep::RepairEncoding(RepairEncoding { column: column()? })
        }
        TransformOp::TidyId => RecipeStep::TidyId(TidyId {
            column: column()?,
            kind: st.tidy_kind.id().to_string(),
        }),
        TransformOp::FillDown | TransformOp::FillUp => RecipeStep::Fill(Fill {
            column: column()?,
            direction: if st.op == TransformOp::FillDown {
                "down"
            } else {
                "up"
            }
            .to_string(),
        }),
        TransformOp::Extract => {
            let column = column()?;
            if st.extract_pattern.trim().is_empty() {
                return Err(octa::i18n::t("transform.need_pattern"));
            }
            if let Err(e) = regex::Regex::new(&st.extract_pattern) {
                return Err(format!("{}: {e}", octa::i18n::t("transform.bad_regex")));
            }
            RecipeStep::Extract(Extract {
                column,
                pattern: st.extract_pattern.clone(),
                new_name,
                position,
            })
        }
        TransformOp::Replace => {
            let column = column()?;
            if st.replace_query.is_empty() {
                return Err(octa::i18n::t("transform.need_find"));
            }
            if matches!(
                RowMatcher::new(&st.replace_query, st.replace_mode),
                RowMatcher::Invalid
            ) {
                return Err(octa::i18n::t("transform.bad_regex"));
            }
            RecipeStep::Replace(Replace::new(
                column,
                st.replace_query.clone(),
                st.replace_with.clone(),
                st.replace_mode,
            ))
        }
    })
}

/// Apply the configured transform to the active tab as one undo step, and
/// record it for the tab's recipe. Returns a user-facing error string
/// (already localized) on bad input.
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
    let step = transform_step(st, &col_names)?;

    let tab = &mut app.tabs[active];
    let start = tab.table.undo_stack.len();
    step.apply(&mut tab.table)
        .map_err(|e| format!("{}: {e:#}", octa::i18n::t("transform.failed")))?;
    tab.table.coalesce_undo_since(start);
    tab.table_state.widths_initialized = false;
    tab.filter_dirty = true;
    app.record_step(step);
    Ok(())
}
