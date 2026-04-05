use egui::{Align, Layout, RichText, Ui};

use super::table_view::TableViewState;
use super::theme::{ThemeColors, ThemeMode};
use crate::data::DataTable;

/// Format a number with comma thousand separators (e.g. 1234567 -> "1,234,567").
pub fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

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
            let row_text = if table.total_rows.is_some() {
                let loaded = table.row_offset + table.row_count();
                if search_active {
                    format!(
                        "{} / {}+ rows (partial)",
                        format_number(filtered_count),
                        format_number(loaded)
                    )
                } else {
                    format!("{}+ rows (scroll to load more)", format_number(loaded))
                }
            } else if search_active {
                format!(
                    "{} / {} rows",
                    format_number(filtered_count),
                    format_number(table.row_count())
                )
            } else {
                format!("{} rows", format_number(table.row_count()))
            };
            ui.label(
                RichText::new(row_text)
                    .size(11.0)
                    .color(colors.text_secondary),
            );
            ui.separator();
            ui.label(
                RichText::new(format!("{} cols", format_number(table.col_count())))
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
                    RichText::new(format!(
                        "Cell: R{}:C{} ({})",
                        row + 1 + table.row_offset,
                        col + 1,
                        col_name
                    ))
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
            if table.is_modified() {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(8.0);
                    let edit_count = table.edits.len();
                    if edit_count > 0 {
                        ui.label(
                            RichText::new(format!("({} edits)", edit_count))
                                .size(11.0)
                                .color(colors.warning),
                        );
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number_zero() {
        assert_eq!(format_number(0), "0");
    }

    #[test]
    fn test_format_number_small() {
        assert_eq!(format_number(1), "1");
        assert_eq!(format_number(12), "12");
        assert_eq!(format_number(999), "999");
    }

    #[test]
    fn test_format_number_thousands() {
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(1_234), "1,234");
        assert_eq!(format_number(12_345), "12,345");
        assert_eq!(format_number(999_999), "999,999");
    }

    #[test]
    fn test_format_number_millions() {
        assert_eq!(format_number(1_000_000), "1,000,000");
        assert_eq!(format_number(1_234_567), "1,234,567");
        assert_eq!(format_number(123_456_789), "123,456,789");
    }
}
