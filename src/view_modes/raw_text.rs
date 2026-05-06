use crate::app::state::{RawCsvEscape, RawCsvQuote, TabState};
use crate::ui;

use eframe::egui;
use egui::RichText;
use ui::theme::ThemeMode;

/// Signals the raw view emits back to the app.
#[derive(Default, Debug, Clone)]
pub struct RawAction {
    /// User clicked the Align Columns checkbox toggling it *off* while there
    /// are unsaved edits. The app should show a confirmation dialog before
    /// reloading. `raw_view_formatted` has already been flipped back to `true`
    /// so the state visibly stays aligned until the user confirms.
    pub confirm_unalign: bool,
}

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
    warn_unalign: bool,
) -> RawAction {
    let mut action = RawAction::default();
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
                        *content = format_delimited_text(
                            content,
                            delim,
                            tab.raw_csv_quote,
                            tab.raw_csv_escape,
                        );
                        tab.raw_content_modified = true;
                    } else if warn_unalign && tab.raw_content_modified {
                        // Reloading would discard in-buffer edits — bounce the
                        // checkbox back and let the app confirm first.
                        tab.raw_view_formatted = true;
                        action.confirm_unalign = true;
                    } else if let Some(ref original) = tab.raw_content_original {
                        *content = original.clone();
                        tab.raw_content_modified = false;
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

                ui.add_space(12.0);
                ui.label("Quotes:");
                let quote_label = match tab.raw_csv_quote {
                    RawCsvQuote::Double => "Double (\")",
                    RawCsvQuote::Single => "Single (')",
                    RawCsvQuote::Both => "Either",
                    RawCsvQuote::None => "None",
                };
                let prev_quote = tab.raw_csv_quote;
                egui::ComboBox::from_id_salt("csv_quote_combo")
                    .selected_text(quote_label)
                    .show_ui(ui, |ui| {
                        for (variant, label) in [
                            (RawCsvQuote::Double, "Double (\")"),
                            (RawCsvQuote::Single, "Single (')"),
                            (RawCsvQuote::Both, "Either"),
                            (RawCsvQuote::None, "None"),
                        ] {
                            ui.selectable_value(&mut tab.raw_csv_quote, variant, label);
                        }
                    });

                ui.label("Escape:");
                let esc_label = match tab.raw_csv_escape {
                    RawCsvEscape::Doubled => "Doubled (\"\")",
                    RawCsvEscape::Backslash => "Backslash (\\\")",
                    RawCsvEscape::None => "None",
                };
                let prev_escape = tab.raw_csv_escape;
                egui::ComboBox::from_id_salt("csv_escape_combo")
                    .selected_text(esc_label)
                    .show_ui(ui, |ui| {
                        for (variant, label) in [
                            (RawCsvEscape::Doubled, "Doubled (\"\")"),
                            (RawCsvEscape::Backslash, "Backslash (\\\")"),
                            (RawCsvEscape::None, "None"),
                        ] {
                            ui.selectable_value(&mut tab.raw_csv_escape, variant, label);
                        }
                    });

                // Re-format the buffer if either combo changed and alignment
                // is currently on, so the user sees the effect immediately.
                // Reformatting starts from the cached original so the prior
                // (now-stale) quote/escape choices don't leak through.
                let mode_changed =
                    prev_quote != tab.raw_csv_quote || prev_escape != tab.raw_csv_escape;
                if mode_changed && tab.raw_view_formatted {
                    if let Some(ref original) = tab.raw_content_original {
                        let delim = tab.csv_delimiter as char;
                        *content = format_delimited_text(
                            original,
                            delim,
                            tab.raw_csv_quote,
                            tab.raw_csv_escape,
                        );
                        tab.raw_content_modified = true;
                    }
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

        let use_col_colors = tab.raw_view_formatted
            && color_aligned_columns
            && tab.raw_color_enabled
            && (is_csv || is_tsv);
        let col_colors = column_colors(theme_mode);
        let delimiter = tab.csv_delimiter as char;
        let layouter_quote = tab.raw_csv_quote;
        let layouter_escape = tab.raw_csv_escape;

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

                let ranges =
                    split_delimited_line_ranges(line, delimiter, layouter_quote, layouter_escape);
                let mut prev_end = 0;
                for (col_idx, range) in ranges.iter().enumerate() {
                    if range.start > prev_end {
                        job.append(
                            &line[prev_end..range.start],
                            0.0,
                            egui::text::TextFormat::simple(font.clone(), default_color),
                        );
                    }
                    let color = col_colors[col_idx % col_colors.len()];
                    job.append(
                        &line[range.clone()],
                        0.0,
                        egui::text::TextFormat::simple(font.clone(), color),
                    );
                    prev_end = range.end;
                }
                if prev_end < line.len() {
                    job.append(
                        &line[prev_end..],
                        0.0,
                        egui::text::TextFormat::simple(font.clone(), default_color),
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
                    let editor_id = egui::Id::new("raw_text_editor");
                    let mut output = if use_col_colors {
                        egui::TextEdit::multiline(content)
                            .id(editor_id)
                            .font(mono_font)
                            .desired_width(f32::INFINITY)
                            .lock_focus(true)
                            .layouter(&mut colored_layouter.clone())
                            .show(ui)
                    } else {
                        egui::TextEdit::multiline(content)
                            .id(editor_id)
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

        // Right-click context menu (selection-aware Copy + whole-content Copy All)
        let editor_id = egui::Id::new("raw_text_editor");
        raw_area.context_menu(|ui| {
            let selection = super::text_ops::selected_text(ui.ctx(), editor_id, &content_for_copy);
            let copy_label = if selection.is_some() {
                "Copy"
            } else {
                "Copy (no selection)"
            };
            let copy_btn = ui.add_enabled(selection.is_some(), egui::Button::new(copy_label));
            if copy_btn.clicked() {
                if let Some(s) = selection {
                    ui.ctx().copy_text(s);
                }
                ui.close_menu();
            }
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
    action
}

/// Align columns in delimited text for display. The tokenizer respects the
/// configured quoting and escape conventions so a delimiter inside `"a,b"`
/// stays inside that single field.
pub(crate) fn format_delimited_text(
    content: &str,
    delimiter: char,
    quote: RawCsvQuote,
    escape: RawCsvEscape,
) -> String {
    // Tokenize each line, then re-emit cells preserving the configured quotes
    // around fields whose content contains the delimiter. Round-tripping the
    // formatted output through the same tokenizer is what lets the colored
    // layouter assign one color per logical column.
    let parsed: Vec<Vec<String>> = content
        .lines()
        .map(|line| {
            split_delimited_line(line, delimiter, quote, escape)
                .into_iter()
                .map(|cell| requote_if_needed(cell.trim(), delimiter, quote, escape))
                .collect()
        })
        .collect();
    if parsed.is_empty() {
        return content.to_string();
    }
    let max_cols = parsed.iter().map(|l| l.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; max_cols];
    for line in &parsed {
        for (i, cell) in line.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    parsed
        .iter()
        .map(|line| {
            line.iter()
                .enumerate()
                .map(|(i, cell)| {
                    let glyph_count = cell.chars().count();
                    if i < line.len() - 1 {
                        let pad = widths[i].saturating_sub(glyph_count);
                        format!("{}{}", cell, " ".repeat(pad))
                    } else {
                        cell.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(&format!("{} ", delimiter))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Re-emit a logical cell value with surrounding quotes (and escaped internal
/// quotes) when the value contains the delimiter and the active quoting mode
/// permits a quote character. Cells that don't contain the delimiter, or
/// modes with `RawCsvQuote::None`, are left untouched.
fn requote_if_needed(
    cell: &str,
    delimiter: char,
    quote: RawCsvQuote,
    escape: RawCsvEscape,
) -> String {
    if !cell.contains(delimiter) {
        return cell.to_string();
    }
    let q_char: char = match quote {
        RawCsvQuote::Double | RawCsvQuote::Both => '"',
        RawCsvQuote::Single => '\'',
        RawCsvQuote::None => return cell.to_string(),
    };
    let escaped = match escape {
        RawCsvEscape::Doubled => cell.replace(q_char, &format!("{}{}", q_char, q_char)),
        RawCsvEscape::Backslash => cell.replace(q_char, &format!("\\{}", q_char)),
        RawCsvEscape::None => cell.to_string(),
    };
    format!("{}{}{}", q_char, escaped, q_char)
}

/// Same tokenization rules as [`split_delimited_line`], but returns the byte
/// ranges of each logical column *within the input line*. Ranges include any
/// wrapping quote characters and escape bytes; the bytes between consecutive
/// ranges are exactly the delimiter character. Used by the raw view's
/// per-column color layouter so a delimiter inside a quoted field shares the
/// column's color rather than starting a new one.
pub(crate) fn split_delimited_line_ranges(
    line: &str,
    delimiter: char,
    quote: RawCsvQuote,
    escape: RawCsvEscape,
) -> Vec<std::ops::Range<usize>> {
    let allowed_quotes: &[char] = match quote {
        RawCsvQuote::Double => &['"'],
        RawCsvQuote::Single => &['\''],
        RawCsvQuote::Both => &['"', '\''],
        RawCsvQuote::None => &[],
    };

    let mut ranges = Vec::new();
    let mut field_start: usize = 0;
    let mut in_quote: Option<char> = None;
    let mut at_field_start = true;
    let mut iter = line.char_indices().peekable();

    while let Some((i, c)) = iter.next() {
        match in_quote {
            None => {
                if c == delimiter {
                    ranges.push(field_start..i);
                    field_start = i + c.len_utf8();
                    at_field_start = true;
                } else if at_field_start && i == field_start && allowed_quotes.contains(&c) {
                    in_quote = Some(c);
                    at_field_start = false;
                } else {
                    at_field_start = false;
                }
            }
            Some(q) => match escape {
                RawCsvEscape::Doubled => {
                    if c == q {
                        if let Some(&(_, next)) = iter.peek() {
                            if next == q {
                                iter.next();
                                continue;
                            }
                        }
                        in_quote = None;
                    }
                }
                RawCsvEscape::Backslash => {
                    if c == '\\' {
                        if iter.peek().is_some() {
                            iter.next();
                        }
                    } else if c == q {
                        in_quote = None;
                    }
                }
                RawCsvEscape::None => {
                    if c == q {
                        in_quote = None;
                    }
                }
            },
        }
    }
    ranges.push(field_start..line.len());
    ranges
}

/// Split a single line into fields, respecting `quote` and `escape`. The
/// outer quotes are stripped from quoted fields; embedded escape sequences
/// (`""` or `\"`) are decoded so the displayed cell is the logical value, not
/// its on-disk form.
fn split_delimited_line(
    line: &str,
    delimiter: char,
    quote: RawCsvQuote,
    escape: RawCsvEscape,
) -> Vec<String> {
    let allowed_quotes: &[char] = match quote {
        RawCsvQuote::Double => &['"'],
        RawCsvQuote::Single => &['\''],
        RawCsvQuote::Both => &['"', '\''],
        RawCsvQuote::None => &[],
    };

    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quote: Option<char> = None;
    let mut at_field_start = true;

    while let Some(c) = chars.next() {
        match in_quote {
            None => {
                if c == delimiter {
                    fields.push(std::mem::take(&mut cur));
                    at_field_start = true;
                } else if at_field_start && cur.is_empty() && allowed_quotes.contains(&c) {
                    in_quote = Some(c);
                    at_field_start = false;
                } else {
                    cur.push(c);
                    at_field_start = false;
                }
            }
            Some(q) => match escape {
                RawCsvEscape::Doubled => {
                    if c == q {
                        if chars.peek() == Some(&q) {
                            chars.next();
                            cur.push(q);
                        } else {
                            in_quote = None;
                        }
                    } else {
                        cur.push(c);
                    }
                }
                RawCsvEscape::Backslash => {
                    if c == '\\' {
                        if let Some(&next) = chars.peek() {
                            chars.next();
                            cur.push(next);
                        }
                    } else if c == q {
                        in_quote = None;
                    } else {
                        cur.push(c);
                    }
                }
                RawCsvEscape::None => {
                    if c == q {
                        in_quote = None;
                    } else {
                        cur.push(c);
                    }
                }
            },
        }
    }
    fields.push(cur);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_plain_csv() {
        let r = split_delimited_line("a,b,c", ',', RawCsvQuote::Double, RawCsvEscape::Doubled);
        assert_eq!(r, vec!["a", "b", "c"]);
    }

    #[test]
    fn split_quoted_comma_inside() {
        let r = split_delimited_line(
            r#""Smith, A","note","x""#,
            ',',
            RawCsvQuote::Double,
            RawCsvEscape::Doubled,
        );
        assert_eq!(r, vec!["Smith, A", "note", "x"]);
    }

    #[test]
    fn split_doubled_quote_escape() {
        // "a""b" -> a"b
        let r = split_delimited_line(
            r#""a""b","c""#,
            ',',
            RawCsvQuote::Double,
            RawCsvEscape::Doubled,
        );
        assert_eq!(r, vec![r#"a"b"#, "c"]);
    }

    #[test]
    fn split_backslash_escape() {
        let r = split_delimited_line(
            r#""a\"b","c""#,
            ',',
            RawCsvQuote::Double,
            RawCsvEscape::Backslash,
        );
        assert_eq!(r, vec![r#"a"b"#, "c"]);
    }

    #[test]
    fn split_single_quotes() {
        let r = split_delimited_line(
            "'Smith, A',note",
            ',',
            RawCsvQuote::Single,
            RawCsvEscape::None,
        );
        assert_eq!(r, vec!["Smith, A", "note"]);
    }

    #[test]
    fn split_either_quote() {
        let r = split_delimited_line(
            r#""a, b",'c, d',e"#,
            ',',
            RawCsvQuote::Both,
            RawCsvEscape::None,
        );
        assert_eq!(r, vec!["a, b", "c, d", "e"]);
    }

    #[test]
    fn split_none_mode_treats_quotes_as_literal() {
        let r = split_delimited_line(r#""a,b",c"#, ',', RawCsvQuote::None, RawCsvEscape::None);
        assert_eq!(r, vec![r#""a"#, r#"b""#, "c"]);
    }

    #[test]
    fn ranges_plain_csv() {
        let line = "a,b,c";
        let r = split_delimited_line_ranges(line, ',', RawCsvQuote::Double, RawCsvEscape::Doubled);
        assert_eq!(r.len(), 3);
        assert_eq!(&line[r[0].clone()], "a");
        assert_eq!(&line[r[1].clone()], "b");
        assert_eq!(&line[r[2].clone()], "c");
    }

    #[test]
    fn ranges_quoted_field_with_internal_delim_is_one_column() {
        // The whole `"1,2,3,4,5"` must come back as one column range — that's
        // the bug the colored layouter was hitting before this fix.
        let line = r#""1,2,3,4,5",foo"#;
        let r = split_delimited_line_ranges(line, ',', RawCsvQuote::Double, RawCsvEscape::Doubled);
        assert_eq!(r.len(), 2);
        assert_eq!(&line[r[0].clone()], r#""1,2,3,4,5""#);
        assert_eq!(&line[r[1].clone()], "foo");
    }

    #[test]
    fn ranges_handle_doubled_quote_escape() {
        let line = r#""a""b",c"#;
        let r = split_delimited_line_ranges(line, ',', RawCsvQuote::Double, RawCsvEscape::Doubled);
        assert_eq!(r.len(), 2);
        assert_eq!(&line[r[0].clone()], r#""a""b""#);
        assert_eq!(&line[r[1].clone()], "c");
    }

    #[test]
    fn format_preserves_quotes_around_embedded_delimiter() {
        // After alignment the cell `"1,2,3,4,5"` keeps its quotes so the
        // tokenizer can group it as one column when re-rendered.
        let formatted = format_delimited_text(
            r#""1,2,3,4,5",foo"#,
            ',',
            RawCsvQuote::Double,
            RawCsvEscape::Doubled,
        );
        let r = split_delimited_line_ranges(
            &formatted,
            ',',
            RawCsvQuote::Double,
            RawCsvEscape::Doubled,
        );
        assert_eq!(r.len(), 2);
        assert!(formatted[r[0].clone()].starts_with('"'));
        assert!(formatted[r[0].clone()].contains("1,2,3,4,5"));
    }
}
