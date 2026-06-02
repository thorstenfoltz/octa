//! "Insert Column" modal dialog: name + type + insert position + formula.
//! On confirm, the formula (if any) is evaluated for every existing row.

use eframe::egui;
use egui::RichText;

use octa::data;

use super::super::file_io::shift_formula_row;
use super::super::init::COLUMN_TYPES;
use super::super::state::OctaApp;

pub(crate) fn render_add_column_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.tabs[app.active_tab].show_add_column_dialog {
        return;
    }
    let mut open = true;
    let mut should_add = false;
    egui::Window::new(octa::i18n::t("dialog.add_column_title"))
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("dialog.field_name"));
                ui.text_edit_singleline(&mut app.tabs[app.active_tab].new_col_name);
            });
            // Autofill: show existing column names that match what the user
            // has typed so far. Clicking one fills the Name field.
            let typed = app.tabs[app.active_tab].new_col_name.clone();
            if !typed.is_empty() {
                let lower = typed.to_lowercase();
                let matches: Vec<String> = app.tabs[app.active_tab]
                    .table
                    .columns
                    .iter()
                    .filter(|c| {
                        let n = c.name.to_lowercase();
                        n != lower && n.contains(&lower)
                    })
                    .take(8)
                    .map(|c| c.name.clone())
                    .collect();
                if !matches.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(octa::i18n::t("dialog.autofill"))
                                .size(10.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                        for name in matches {
                            if ui.small_button(&name).clicked() {
                                app.tabs[app.active_tab].new_col_name = name;
                            }
                        }
                    });
                }
            }
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("dialog.field_type"));
                egui::ComboBox::from_id_salt("col_type_combo")
                    .selected_text(app.tabs[app.active_tab].new_col_type.as_str())
                    .show_ui(ui, |ui| {
                        for t in COLUMN_TYPES {
                            ui.selectable_value(
                                &mut app.tabs[app.active_tab].new_col_type,
                                t.to_string(),
                                *t,
                            );
                        }
                    });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("dialog.insert_at_position"));
                let col_count = app.tabs[app.active_tab].table.col_count();
                // Plain text input instead of DragValue - DragValue draws
                // hover spinner arrows that look out of place in this dialog
                // and the value range is small enough that typing it is
                // easier than dragging. Empty buffer falls back to
                // `col_count + 1` (append at end).
                let target_pos = app.tabs[app.active_tab].insert_col_at.unwrap_or(col_count) + 1;
                if app.tabs[app.active_tab].insert_col_at_text.is_empty() {
                    app.tabs[app.active_tab].insert_col_at_text = target_pos.to_string();
                }
                let parsed = app.tabs[app.active_tab]
                    .insert_col_at_text
                    .trim()
                    .parse::<usize>()
                    .ok();
                let valid = parsed.is_some_and(|v| (1..=col_count + 1).contains(&v));
                let buf_is_empty = app.tabs[app.active_tab].insert_col_at_text.is_empty();
                let buf = &mut app.tabs[app.active_tab].insert_col_at_text;
                let mut text_edit = egui::TextEdit::singleline(buf).desired_width(48.0);
                if !valid && !buf_is_empty {
                    text_edit = text_edit.text_color(egui::Color32::from_rgb(220, 80, 80));
                }
                if ui.add(text_edit).changed()
                    && let Some(v) = app.tabs[app.active_tab]
                        .insert_col_at_text
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .filter(|v| (1..=col_count + 1).contains(v))
                {
                    app.tabs[app.active_tab].insert_col_at = Some(v - 1);
                }
                ui.label(format!("/ {}", col_count + 1));
            });
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("dialog.field_formula"));
                ui.add(
                    egui::TextEdit::singleline(&mut app.tabs[app.active_tab].new_col_formula)
                        .hint_text(octa::i18n::t("dialog.formula_hint")),
                );
            });
            ui.label(
                RichText::new(octa::i18n::t("dialog.add_column_tip"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(octa::i18n::t("common.add")).clicked()
                    && !app.tabs[app.active_tab].new_col_name.is_empty()
                {
                    should_add = true;
                }
                if ui.button(octa::i18n::t("common.cancel")).clicked() {
                    app.tabs[app.active_tab].show_add_column_dialog = false;
                }
            });
        });
    if should_add {
        let idx = app.tabs[app.active_tab]
            .insert_col_at
            .unwrap_or(app.tabs[app.active_tab].table.col_count());
        let formula_text = app.tabs[app.active_tab].new_col_formula.trim().to_string();
        let col_name = app.tabs[app.active_tab].new_col_name.clone();
        let col_type = app.tabs[app.active_tab].new_col_type.clone();
        app.tabs[app.active_tab]
            .table
            .insert_column(idx, col_name, col_type);
        if let Some(formula_body) = formula_text.strip_prefix('=') {
            let row_count = app.tabs[app.active_tab].table.row_count();
            // Collect per-row diagnostics so we can pop a "skipped N rows;
            // first non-numeric cell: X" banner if any reference resolves
            // to a non-numeric value (previously silent-fall-back to 0).
            let mut skipped: usize = 0;
            let mut first_bad: Option<data::FormulaBadCell> = None;
            for row in 0..row_count {
                let shifted = shift_formula_row(formula_body, row);
                let outcome = data::evaluate_formula_with_diagnostics(
                    &shifted,
                    &app.tabs[app.active_tab].table,
                );
                if let Some(bad) = outcome.bad_cell
                    && first_bad.is_none()
                {
                    first_bad = Some(bad);
                }
                match outcome.value {
                    Some(result) => {
                        let val = if result.fract() == 0.0 && result.abs() < i64::MAX as f64 {
                            data::CellValue::Int(result as i64)
                        } else {
                            data::CellValue::Float(result)
                        };
                        app.tabs[app.active_tab].table.set(row, idx, val);
                    }
                    None => {
                        skipped += 1;
                    }
                }
            }
            if let Some(bad) = first_bad {
                let col_name = app.tabs[app.active_tab]
                    .table
                    .columns
                    .get(bad.col)
                    .map(|c| c.name.as_str())
                    .unwrap_or("?");
                app.tabs[app.active_tab].parse_error_banner = Some(format!(
                    "{} {}/{} {}; {} \"{}\" {} {} = {:?}",
                    octa::i18n::t("dialog.formula_skipped"),
                    skipped,
                    row_count,
                    octa::i18n::t("dialog.formula_rows"),
                    octa::i18n::t("dialog.formula_first_ref"),
                    col_name,
                    octa::i18n::t("dialog.formula_row"),
                    bad.row + 1,
                    bad.content
                ));
            }
        }
        if let Some((row, _)) = app.tabs[app.active_tab].table_state.selected_cell {
            app.tabs[app.active_tab].table_state.selected_cell = Some((row, idx));
        }
        app.tabs[app.active_tab].table_state.widths_initialized = false;
        app.tabs[app.active_tab].filter_dirty = true;
        app.tabs[app.active_tab].show_add_column_dialog = false;
        app.tabs[app.active_tab].insert_col_at_text.clear();
    }
    if !open {
        app.tabs[app.active_tab].show_add_column_dialog = false;
        app.tabs[app.active_tab].insert_col_at_text.clear();
    }
}
