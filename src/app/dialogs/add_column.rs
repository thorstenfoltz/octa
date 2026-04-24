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
    egui::Window::new("Insert Column")
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name:");
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
                            RichText::new("Autofill:")
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
                ui.label("Type:");
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
                ui.label("Insert at position:");
                let col_count = app.tabs[app.active_tab].table.col_count();
                let mut pos_val = app.tabs[app.active_tab].insert_col_at.unwrap_or(col_count) + 1;
                let drag = egui::DragValue::new(&mut pos_val)
                    .range(1..=(col_count + 1))
                    .speed(1.0);
                if ui.add(drag).changed() {
                    app.tabs[app.active_tab].insert_col_at = Some((pos_val - 1).min(col_count));
                }
                ui.label(format!("/ {}", col_count + 1));
            });
            ui.horizontal(|ui| {
                ui.label("Formula:");
                ui.add(
                    egui::TextEdit::singleline(&mut app.tabs[app.active_tab].new_col_formula)
                        .hint_text("e.g. =A1+B1 or =A1*2"),
                );
            });
            ui.label(
                RichText::new(
                    "Tip: click a column header to set insert position. \
                     Formula uses Excel-style references (A1, B2, ...) with +, -, *, /.",
                )
                .size(10.0)
                .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Add").clicked() && !app.tabs[app.active_tab].new_col_name.is_empty() {
                    should_add = true;
                }
                if ui.button("Cancel").clicked() {
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
        if formula_text.starts_with('=') {
            let formula_body = &formula_text[1..];
            let row_count = app.tabs[app.active_tab].table.row_count();
            for row in 0..row_count {
                let shifted = shift_formula_row(formula_body, row);
                if let Some(result) =
                    data::evaluate_formula(&shifted, &app.tabs[app.active_tab].table)
                {
                    let val = if result.fract() == 0.0 && result.abs() < i64::MAX as f64 {
                        data::CellValue::Int(result as i64)
                    } else {
                        data::CellValue::Float(result)
                    };
                    app.tabs[app.active_tab].table.set(row, idx, val);
                }
            }
        }
        if let Some((row, _)) = app.tabs[app.active_tab].table_state.selected_cell {
            app.tabs[app.active_tab].table_state.selected_cell = Some((row, idx));
        }
        app.tabs[app.active_tab].table_state.widths_initialized = false;
        app.tabs[app.active_tab].filter_dirty = true;
        app.tabs[app.active_tab].show_add_column_dialog = false;
    }
    if !open {
        app.tabs[app.active_tab].show_add_column_dialog = false;
    }
}
