use crate::TabState;
use crate::ui;

use eframe::egui;
use egui::RichText;
use ui::theme::ThemeMode;

const COL_COLORS_DARK: [egui::Color32; 6] = [
    egui::Color32::from_rgb(0x7d, 0xb8, 0xf0), // soft blue
    egui::Color32::from_rgb(0xa8, 0xd8, 0x6e), // soft green
    egui::Color32::from_rgb(0xe0, 0x9f, 0x5e), // soft orange
    egui::Color32::from_rgb(0xc4, 0x8f, 0xd8), // soft purple
    egui::Color32::from_rgb(0x5e, 0xd4, 0xc8), // soft teal
    egui::Color32::from_rgb(0xe8, 0x78, 0x80), // soft red
];

const COL_COLORS_LIGHT: [egui::Color32; 6] = [
    egui::Color32::from_rgb(0x1d, 0x5f, 0xa0), // blue
    egui::Color32::from_rgb(0x2e, 0x7d, 0x32), // green
    egui::Color32::from_rgb(0xc4, 0x6a, 0x10), // orange
    egui::Color32::from_rgb(0x7b, 0x1f, 0xa2), // purple
    egui::Color32::from_rgb(0x00, 0x7a, 0x7a), // teal
    egui::Color32::from_rgb(0xb7, 0x1c, 0x1c), // red
];

/// Column colors that cycle for adjacent-column contrast.
fn column_colors(theme_mode: ThemeMode) -> &'static [egui::Color32] {
    if theme_mode.is_dark() {
        &COL_COLORS_DARK
    } else {
        &COL_COLORS_LIGHT
    }
}

/// Render the raw text editor view with line numbers and optional column alignment.
pub fn render_raw_view(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    theme_mode: ThemeMode,
    color_aligned_columns: bool,
    tab_size: usize,
) {
    if let Some(ref mut content) = tab.raw_content {
        let colors = ui::theme::ThemeColors::for_mode(theme_mode);

        // Toolbar for CSV/TSV: align columns + delimiter selector
        let is_csv = tab.table.format_name.as_deref() == Some("CSV");
        let is_tsv = tab.table.format_name.as_deref() == Some("TSV");
        if is_csv || is_tsv {
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut tab.raw_view_formatted, "Align Columns")
                    .changed()
                {
                    if tab.raw_view_formatted {
                        let delim = tab.csv_delimiter as char;
                        *content = format_delimited_text(content, delim);
                        tab.raw_content_modified = true;
                    } else if let Some(ref path) = tab.table.source_path {
                        if let Ok(original) = std::fs::read_to_string(path) {
                            *content = original;
                            tab.raw_content_modified = false;
                        }
                    }
                }
                ui.add_space(16.0);
                if is_csv {
                    ui.label("Delimiter:");
                    let delim_label = match tab.csv_delimiter {
                        b',' => "Comma (,)",
                        b';' => "Semicolon (;)",
                        b'|' => "Pipe (|)",
                        b'\t' => "Tab (\\t)",
                        _ => "Comma (,)",
                    };
                    egui::ComboBox::from_id_salt("csv_delimiter_combo")
                        .selected_text(delim_label)
                        .show_ui(ui, |ui| {
                            let options: &[(u8, &str)] = &[
                                (b',', "Comma (,)"),
                                (b';', "Semicolon (;)"),
                                (b'|', "Pipe (|)"),
                                (b'\t', "Tab (\\t)"),
                            ];
                            for &(delim, label) in options {
                                if ui
                                    .selectable_value(&mut tab.csv_delimiter, delim, label)
                                    .clicked()
                                {
                                    tab.raw_content_modified = true;
                                }
                            }
                        });
                }
            });
            ui.add_space(2.0);
        }

        // Line numbers + text editor side by side
        let line_count = content.lines().count().max(1);
        let line_num_text: String = (1..=line_count)
            .map(|n| format!("{:>width$}", n, width = line_count.to_string().len()))
            .collect::<Vec<_>>()
            .join("\n");
        let line_num_width = line_count.to_string().len() as f32 * 8.0 + 16.0;

        let mono_font = egui::FontId::new(13.0, egui::FontFamily::Monospace);
        let nowrap_layouter = |ui: &egui::Ui, text: &str, _wrap_width: f32| {
            let mut job = egui::text::LayoutJob::simple(
                text.to_owned(),
                egui::FontId::new(13.0, egui::FontFamily::Monospace),
                ui.visuals().text_color(),
                f32::INFINITY,
            );
            job.wrap.max_width = f32::INFINITY;
            ui.fonts(|f| f.layout_job(job))
        };

        let use_col_colors = tab.raw_view_formatted && color_aligned_columns && (is_csv || is_tsv);
        let col_colors = column_colors(theme_mode);
        let delimiter = tab.csv_delimiter as char;

        let colored_layouter = move |ui: &egui::Ui, text: &str, _wrap_width: f32| {
            let font = egui::FontId::new(13.0, egui::FontFamily::Monospace);
            let default_color = ui.visuals().text_color();
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = f32::INFINITY;

            let mut first_line = true;
            for line in text.split('\n') {
                if !first_line {
                    job.append(
                        "\n",
                        0.0,
                        egui::text::TextFormat::simple(font.clone(), default_color),
                    );
                }
                first_line = false;

                for (col_idx, segment) in line.split(delimiter).enumerate() {
                    if col_idx > 0 {
                        let delim_str = &format!("{delimiter}");
                        job.append(
                            delim_str,
                            0.0,
                            egui::text::TextFormat::simple(font.clone(), default_color),
                        );
                    }
                    let color = col_colors[col_idx % col_colors.len()];
                    job.append(
                        segment,
                        0.0,
                        egui::text::TextFormat::simple(font.clone(), color),
                    );
                }
            }
            ui.fonts(|f| f.layout_job(job))
        };

        let content_for_copy = content.clone();

        // Allocate remaining available rect for right-click detection
        let full_rect = ui.available_rect_before_wrap();
        let raw_area = ui.interact(
            full_rect,
            ui.id().with("raw_view_ctx"),
            egui::Sense::click(),
        );

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    // Line numbers column (non-editable)
                    ui.add_sized(
                        [line_num_width, ui.available_height()],
                        egui::TextEdit::multiline(&mut line_num_text.clone())
                            .font(mono_font.clone())
                            .interactive(false)
                            .desired_width(line_num_width)
                            .text_color(colors.text_muted)
                            .frame(false)
                            .layouter(&mut nowrap_layouter.clone()),
                    );
                    // Separator line
                    ui.add_space(2.0);
                    let sep_rect = egui::Rect::from_min_size(
                        ui.cursor().left_top(),
                        egui::vec2(1.0, ui.available_height()),
                    );
                    ui.painter().rect_filled(sep_rect, 0.0, colors.border);
                    ui.add_space(4.0);
                    // Text editor (no wrapping — scroll horizontally)
                    // lock_focus(true) prevents Tab from navigating to other widgets
                    let mut output = if use_col_colors {
                        egui::TextEdit::multiline(content)
                            .font(mono_font)
                            .desired_width(f32::INFINITY)
                            .lock_focus(true)
                            .layouter(&mut colored_layouter.clone())
                            .show(ui)
                    } else {
                        egui::TextEdit::multiline(content)
                            .font(mono_font)
                            .desired_width(f32::INFINITY)
                            .lock_focus(true)
                            .text_color(colors.text_primary)
                            .layouter(&mut nowrap_layouter.clone())
                            .show(ui)
                    };

                    // Replace any literal \t egui may have inserted with spaces,
                    // then manually insert spaces at the cursor for our Tab handling.
                    // We must do the \t replacement first so we can adjust the cursor
                    // position to account for any expansion.
                    let had_tabs = content.contains('\t');
                    if had_tabs {
                        // Track cursor so we can restore it after replacement
                        let cursor_idx = output
                            .cursor_range
                            .map(|r| r.primary.ccursor.index)
                            .unwrap_or(0);
                        // Count \t chars before cursor to compute offset shift
                        let tabs_before = content[..cursor_idx.min(content.len())]
                            .chars()
                            .filter(|&c| c == '\t')
                            .count();
                        let spaces = " ".repeat(tab_size);
                        *content = content.replace('\t', &spaces);
                        // Adjust cursor for expanded tabs
                        let new_idx = cursor_idx + tabs_before * (tab_size - 1);
                        let new_cursor = egui::text::CCursor::new(new_idx);
                        let new_range = egui::text::CCursorRange::one(new_cursor);
                        output.state.cursor.set_char_range(Some(new_range));
                        output.state.store(ui.ctx(), output.response.id);
                        tab.raw_content_modified = true;
                    }
                    if output.response.changed() && !had_tabs {
                        tab.raw_content_modified = true;
                    }
                });
            });

        // Right-click context menu
        raw_area.context_menu(|ui| {
            if ui.button("Copy All").clicked() {
                ui.ctx().copy_text(content_for_copy.clone());
                ui.close_menu();
            }
        });
    } else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Raw text view is not available for binary formats")
                    .size(16.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
    }
}

/// Align columns in delimited text for display.
fn format_delimited_text(content: &str, delimiter: char) -> String {
    let lines: Vec<Vec<&str>> = content
        .lines()
        .map(|line| line.split(delimiter).collect())
        .collect();
    if lines.is_empty() {
        return content.to_string();
    }
    let max_cols = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; max_cols];
    for line in &lines {
        for (i, cell) in line.iter().enumerate() {
            widths[i] = widths[i].max(cell.trim().len());
        }
    }
    lines
        .iter()
        .map(|line| {
            line.iter()
                .enumerate()
                .map(|(i, cell)| {
                    let trimmed = cell.trim();
                    if i < line.len() - 1 {
                        format!("{:<width$}", trimmed, width = widths[i])
                    } else {
                        trimmed.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(&format!("{} ", delimiter))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
