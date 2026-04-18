use crate::TabState;

use eframe::egui;
use octa::data::CellValue;

/// User actions emitted by the SQL view in a single frame.
#[derive(Debug, Clone, Default)]
pub struct SqlAction {
    pub run: bool,
    pub clear: bool,
}

/// Render a split-pane SQL editor (top) and result table (bottom).
/// The current tab's table is exposed in queries as `data`.
pub fn render_sql_view(ui: &mut egui::Ui, tab: &mut TabState) -> SqlAction {
    let mut action = SqlAction::default();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Query against `data`").strong());
        ui.add_space(8.0);
        if ui
            .button("Run (Ctrl+Enter)")
            .on_hover_text("Execute the query")
            .clicked()
        {
            action.run = true;
        }
        if ui.button("Clear result").clicked() {
            action.clear = true;
        }
        if let Some(rows) = tab.sql_result.as_ref().map(|t| t.row_count()) {
            ui.add_space(12.0);
            ui.label(format!(
                "{} result row{}",
                rows,
                if rows == 1 { "" } else { "s" }
            ));
        }
    });
    ui.add_space(4.0);

    let total = ui.available_height();
    let editor_height = (total * 0.4).max(120.0).min(total - 80.0);

    egui::ScrollArea::vertical()
        .id_salt("sql_editor_scroll")
        .max_height(editor_height)
        .show(ui, |ui| {
            let mono = egui::FontId::new(13.0, egui::FontFamily::Monospace);
            let resp = ui.add(
                egui::TextEdit::multiline(&mut tab.sql_query)
                    .font(mono)
                    .desired_width(f32::INFINITY)
                    .desired_rows(8)
                    .lock_focus(true)
                    .hint_text("SELECT * FROM data WHERE ..."),
            );
            if resp.has_focus()
                && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter))
            {
                action.run = true;
            }
        });

    ui.separator();

    if let Some(err) = &tab.sql_error {
        ui.colored_label(
            egui::Color32::from_rgb(220, 80, 80),
            format!("Error: {err}"),
        );
        ui.add_space(4.0);
    }

    if let Some(result) = &tab.sql_result {
        render_result_table(ui, result);
    } else if tab.sql_error.is_none() {
        ui.label(egui::RichText::new("Run a query to see results.").weak());
    }

    action
}

fn render_result_table(ui: &mut egui::Ui, table: &octa::data::DataTable) {
    use egui_extras::{Column, TableBuilder};

    if table.col_count() == 0 {
        ui.label(egui::RichText::new("Query returned no columns.").weak());
        return;
    }

    egui::ScrollArea::horizontal()
        .id_salt("sql_result_scroll")
        .show(ui, |ui| {
            let mut builder = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
            for _ in &table.columns {
                builder = builder.column(Column::auto().at_least(80.0).resizable(true));
            }
            builder
                .header(22.0, |mut header| {
                    for col in &table.columns {
                        header.col(|ui| {
                            ui.strong(&col.name);
                        });
                    }
                })
                .body(|mut body| {
                    for r in 0..table.row_count() {
                        body.row(20.0, |mut row| {
                            for c in 0..table.col_count() {
                                row.col(|ui| {
                                    let v = table.get(r, c).cloned().unwrap_or(CellValue::Null);
                                    ui.label(v.to_string());
                                });
                            }
                        });
                    }
                });
        });
}
