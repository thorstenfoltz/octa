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

/// Result of the status bar: an optional navigation target.
#[derive(Default)]
pub struct StatusBarAction {
    /// Navigate to this cell (row, col) — 0-indexed.
    pub navigate_to: Option<(usize, usize)>,
    /// User typed the secret "kraken" command into the nav input.
    pub kraken_summoned: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn draw_status_bar(
    ui: &mut Ui,
    table: &DataTable,
    state: &TableViewState,
    theme_mode: ThemeMode,
    filtered_count: usize,
    search_active: bool,
    nav_input: &mut String,
    nav_focus_requested: bool,
    zoom_percent: u32,
) -> StatusBarAction {
    let mut action = StatusBarAction::default();
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

            // Selected cell info + navigation input
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

            ui.separator();
            let nav_response = ui.add(
                egui::TextEdit::singleline(nav_input)
                    .desired_width(180.0)
                    .hint_text("Go to R:C or col name")
                    .font(egui::FontId::new(11.0, egui::FontFamily::Monospace)),
            );
            if nav_focus_requested {
                nav_response.request_focus();
            }
            if nav_response.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !nav_input.is_empty()
            {
                if nav_input.trim().eq_ignore_ascii_case("kraken") {
                    action.kraken_summoned = true;
                } else if let Some(target) = parse_nav_input(nav_input, table) {
                    action.navigate_to = Some(target);
                }
                nav_input.clear();
            }

            // Right-aligned: zoom + edit indicator
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);

                // Zoom indicator
                if zoom_percent != 100 {
                    ui.label(
                        RichText::new(format!("{}%", zoom_percent))
                            .size(11.0)
                            .color(colors.text_muted),
                    );
                    ui.separator();
                }

                // Edit indicator
                if table.is_modified() {
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
                }
            });
        } else {
            ui.label(
                RichText::new("No file loaded")
                    .size(11.0)
                    .color(colors.text_muted),
            );
        }
    });

    action
}

/// Parse navigation input into a (row, col) target.
/// Supports:
/// - "R5:C3" or "5:3" — row:col (1-indexed)
/// - "R5" or "5" — row only (keeps current col or 0)
/// - "C3" — column only (keeps current row or 0)
/// - "colname" — jump to column by name (keeps current row or 0)
pub fn parse_nav_input(input: &str, table: &DataTable) -> Option<(usize, usize)> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    // Try R<n>:C<n> or <n>:<n>
    if let Some((left, right)) = input.split_once(':') {
        let row = parse_row_part(left, table)?;
        let col = parse_col_part(right, table)?;
        return Some((row, col));
    }

    // Try C<n> — column only
    if let Some(stripped) = input.strip_prefix('C').or_else(|| input.strip_prefix('c')) {
        if let Ok(n) = stripped.parse::<usize>() {
            if n >= 1 && n <= table.col_count() {
                return Some((0, n - 1));
            }
        }
    }

    // Try R<n> — row only
    if let Some(stripped) = input.strip_prefix('R').or_else(|| input.strip_prefix('r')) {
        if let Ok(n) = stripped.parse::<usize>() {
            if n >= 1 && n <= table.row_count() {
                return Some((n - 1, 0));
            }
        }
    }

    // Try pure number — row
    if let Ok(n) = input.parse::<usize>() {
        if n >= 1 && n <= table.row_count() {
            return Some((n - 1, 0));
        }
    }

    // Try column name (case-insensitive)
    let lower = input.to_lowercase();
    for (i, col) in table.columns.iter().enumerate() {
        if col.name.to_lowercase() == lower {
            return Some((0, i));
        }
    }

    None
}

fn parse_row_part(s: &str, table: &DataTable) -> Option<usize> {
    let s = s.trim();
    let num_str = s
        .strip_prefix('R')
        .or_else(|| s.strip_prefix('r'))
        .unwrap_or(s);
    let n = num_str.parse::<usize>().ok()?;
    if n >= 1 && n <= table.row_count() {
        Some(n - 1)
    } else {
        None
    }
}

fn parse_col_part(s: &str, table: &DataTable) -> Option<usize> {
    let s = s.trim();
    let num_str = s
        .strip_prefix('C')
        .or_else(|| s.strip_prefix('c'))
        .unwrap_or(s);
    if let Ok(n) = num_str.parse::<usize>() {
        if n >= 1 && n <= table.col_count() {
            return Some(n - 1);
        }
    }
    // Try column name
    let lower = s.to_lowercase();
    for (i, col) in table.columns.iter().enumerate() {
        if col.name.to_lowercase() == lower {
            return Some(i);
        }
    }
    None
}
