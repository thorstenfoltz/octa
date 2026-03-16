use egui::{Align, Layout, RichText, Ui};

use super::table_view::TableViewState;
use super::theme::{ThemeColors, ThemeMode};
use crate::data::DataTable;

pub fn draw_status_bar(
    ui: &mut Ui,
    table: &DataTable,
    state: &TableViewState,
    theme_mode: ThemeMode,
    filtered_count: usize,
    search_active: bool,
) {
    let colors = ThemeColors::for_mode(theme_mode);

    ui.horizontal(|ui| {
        ui.add_space(8.0);

        if table.col_count() > 0 {
            // File info
            if let Some(ref path) = table.source_path {
                let filename = std::path::Path::new(path)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.clone());
                ui.label(
                    RichText::new(format!("File: {}", filename))
                        .size(11.0)
                        .color(colors.text_secondary),
                );
                ui.separator();
            }

            if let Some(ref fmt) = table.format_name {
                ui.label(RichText::new(fmt.as_str()).size(11.0).color(colors.accent));
                ui.separator();
            }

            // Row/col count
            let row_text = if search_active {
                format!("{} / {} rows", filtered_count, table.row_count())
            } else {
                format!("{} rows", table.row_count())
            };
            ui.label(
                RichText::new(row_text)
                    .size(11.0)
                    .color(colors.text_secondary),
            );
            ui.separator();
            ui.label(
                RichText::new(format!("{} cols", table.col_count()))
                    .size(11.0)
                    .color(colors.text_secondary),
            );

            // Selected cell info
            if let Some((row, col)) = state.selected_cell {
                ui.separator();
                let col_name = table
                    .columns
                    .get(col)
                    .map(|c| c.name.as_str())
                    .unwrap_or("?");
                ui.label(
                    RichText::new(format!("Cell: R{}:C{} ({})", row + 1, col + 1, col_name))
                        .size(11.0)
                        .color(colors.text_secondary),
                );

                if let Some(val) = table.get(row, col) {
                    ui.separator();
                    ui.label(
                        RichText::new(format!("Type: {}", val.type_name()))
                            .size(11.0)
                            .color(colors.text_muted),
                    );
                }
            }

            // Edit indicator
            if table.has_edits() {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("({} edits)", table.edits.len()))
                            .size(11.0)
                            .color(colors.warning),
                    );
                    ui.label(
                        RichText::new("Modified")
                            .size(11.0)
                            .strong()
                            .color(colors.warning),
                    );
                });
            }
        } else {
            ui.label(
                RichText::new("No file loaded")
                    .size(11.0)
                    .color(colors.text_muted),
            );
        }
    });
}
