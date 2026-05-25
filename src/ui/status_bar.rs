use egui::{self, Align, Color32, Layout, RichText, Ui};

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
    /// User clicked the column-filter chip — open the Column Filter dialog
    /// preselected on the first filtered column.
    pub open_column_filter: Option<usize>,
}

/// Per-selection rollups shown as a status-bar pill when more than one
/// cell is highlighted. Numeric counts mirror what Excel shows in the
/// bottom-right "AutoCalculate" zone; the plain `count` is always the
/// total number of non-null cells in the selection regardless of type.
#[derive(Debug, Default, Clone, Copy)]
pub struct SelectionStats {
    pub count: usize,
    pub numeric_count: usize,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
}

/// Build a [`SelectionStats`] for the given (row, col) cells. Cells whose
/// value can't be coerced to `f64` contribute to `count` only.
pub fn compute_selection_stats(
    table: &DataTable,
    cells: impl Iterator<Item = (usize, usize)>,
) -> SelectionStats {
    use crate::data::CellValue;
    let mut out = SelectionStats {
        min: f64::INFINITY,
        max: f64::NEG_INFINITY,
        ..Default::default()
    };
    for (row, col) in cells {
        let Some(value) = table.get(row, col) else {
            continue;
        };
        if matches!(value, CellValue::Null) {
            continue;
        }
        out.count += 1;
        let numeric = match value {
            CellValue::Int(n) => Some(*n as f64),
            CellValue::Float(f) if f.is_finite() => Some(*f),
            _ => None,
        };
        if let Some(n) = numeric {
            out.numeric_count += 1;
            out.sum += n;
            if n < out.min {
                out.min = n;
            }
            if n > out.max {
                out.max = n;
            }
        }
    }
    if out.numeric_count == 0 {
        out.min = 0.0;
        out.max = 0.0;
    }
    out
}

fn format_float(n: f64) -> String {
    if n.abs() >= 1e15 || (n != 0.0 && n.abs() < 1e-3) {
        format!("{:.3e}", n)
    } else if n.fract() == 0.0 {
        let abs = n.abs() as usize;
        let formatted = format_number(abs);
        if n < 0.0 {
            format!("-{}", formatted)
        } else {
            formatted
        }
    } else {
        format!("{:.3}", n)
    }
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
    readonly: bool,
    busy: bool,
    busy_hint: Option<&str>,
    column_filter_count: usize,
    first_filtered_col: Option<usize>,
    selected_rows: &std::collections::HashSet<usize>,
    selected_cells: &std::collections::HashSet<(usize, usize)>,
) -> StatusBarAction {
    let mut action = StatusBarAction::default();
    let colors = ThemeColors::for_mode(theme_mode);

    ui.horizontal(|ui| {
        ui.add_space(8.0);

        // Busy indicator: small spinner + optional one-word reason. Shown
        // only while a long-running operation is in flight (background
        // row load, update check, update install). Idle frames stay
        // completely silent so startup feels fast.
        if busy {
            ui.add(egui::Spinner::new().size(12.0));
            if let Some(hint) = busy_hint {
                ui.label(RichText::new(hint).size(11.0).color(colors.text_secondary));
            }
            ui.separator();
        }

        if readonly {
            // Plain text instead of a lock emoji — many bundled fonts lack
            // U+1F512 and render it as a tofu / replacement glyph.
            ui.label(
                RichText::new("[Read-only]")
                    .size(11.0)
                    .color(Color32::from_rgb(0xc0, 0x6a, 0x10))
                    .strong(),
            );
            ui.separator();
        }

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
                RichText::new(format!("{} columns", format_number(table.col_count())))
                    .size(11.0)
                    .color(colors.text_secondary),
            );

            // Active column-filter chip. Clickable shortcut into the dialog,
            // preselected on the first filtered column.
            if column_filter_count > 0 {
                ui.separator();
                let chip = ui
                    .add(
                        egui::Label::new(
                            RichText::new(format!(
                                "Filter: {} col{}",
                                column_filter_count,
                                if column_filter_count == 1 { "" } else { "s" }
                            ))
                            .size(11.0)
                            .color(colors.accent)
                            .strong(),
                        )
                        .sense(egui::Sense::click()),
                    )
                    .on_hover_text("Click to manage column filters");
                if chip.clicked() {
                    action.open_column_filter = first_filtered_col;
                }
            }

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

            // Selection rollup pill (Excel-style Sum / Count / Avg / Min / Max).
            // Shown only when more than one cell is selected; the single-cell
            // info above already covers the one-cell case. Selection sources
            // resolve in the same priority order the clipboard uses.
            let stats_cells: Option<Vec<(usize, usize)>> = if !selected_cells.is_empty() {
                Some(selected_cells.iter().copied().collect())
            } else if !selected_rows.is_empty() {
                let cols = table.col_count();
                Some(
                    selected_rows
                        .iter()
                        .flat_map(|&r| (0..cols).map(move |c| (r, c)))
                        .collect(),
                )
            } else if !state.selected_cols.is_empty() {
                let rows = table.row_count();
                Some(
                    state
                        .selected_cols
                        .iter()
                        .flat_map(|&c| (0..rows).map(move |r| (r, c)))
                        .collect(),
                )
            } else {
                None
            };
            if let Some(cells) = stats_cells {
                let stats = compute_selection_stats(table, cells.into_iter());
                if stats.count > 1 {
                    ui.separator();
                    let text = if stats.numeric_count > 0 {
                        let avg = stats.sum / stats.numeric_count as f64;
                        format!(
                            "Count={} Sum={} Avg={} Min={} Max={}",
                            format_number(stats.count),
                            format_float(stats.sum),
                            format_float(avg),
                            format_float(stats.min),
                            format_float(stats.max),
                        )
                    } else {
                        format!("Count={}", format_number(stats.count))
                    };
                    ui.label(RichText::new(text).size(11.0).color(colors.accent).strong());
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
    if let Some(stripped) = input.strip_prefix('C').or_else(|| input.strip_prefix('c'))
        && let Ok(n) = stripped.parse::<usize>()
        && n >= 1
        && n <= table.col_count()
    {
        return Some((0, n - 1));
    }

    // Try R<n> — row only
    if let Some(stripped) = input.strip_prefix('R').or_else(|| input.strip_prefix('r'))
        && let Ok(n) = stripped.parse::<usize>()
        && n >= 1
        && n <= table.row_count()
    {
        return Some((n - 1, 0));
    }

    // Try pure number — row
    if let Ok(n) = input.parse::<usize>()
        && n >= 1
        && n <= table.row_count()
    {
        return Some((n - 1, 0));
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
    if let Ok(n) = num_str.parse::<usize>()
        && n >= 1
        && n <= table.col_count()
    {
        return Some(n - 1);
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
