//! Per-column number-format dialog: choose decimals + rounding mode for a
//! numeric column. Display-only - the stored values are untouched; Save asks
//! the user before writing rounded values. Opened from the column-header
//! right-click menu ("Number format...") and **Edit -> Number format...**.
//!
//! Edits apply **live** to `column_number_formats` so the table reformats as
//! you type; there is no Apply step. The decimals input is a free-text signed
//! integer (negative rounds before the decimal point); empty means "Auto".

use eframe::egui;

use octa::data::CellValue;
use octa::data::num_format::{NumberFormat, format_cell_number};

use super::super::state::OctaApp;

pub(crate) fn render_column_format_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(col_idx) = app.tabs[app.active_tab].column_format_col else {
        return;
    };
    // Guard against the table changing shape while the dialog is open.
    if col_idx >= app.tabs[app.active_tab].table.col_count() {
        app.tabs[app.active_tab].column_format_col = None;
        return;
    }

    let column_name = app.tabs[app.active_tab].table.columns[col_idx].name.clone();

    // Numeric columns the picker offers - rounding only applies to numbers.
    let numeric_cols: Vec<(usize, String)> = {
        let table = &app.tabs[app.active_tab].table;
        (0..table.col_count())
            .filter(|&c| octa::data::is_numeric_data_type(&table.columns[c].data_type))
            .map(|c| (c, table.columns[c].name.clone()))
            .collect()
    };

    // Parse the persisted decimals buffer (empty / invalid = Auto). Negative
    // values round before the decimal point.
    let buf = app.tabs[app.active_tab].column_format_decimals_buf.clone();
    let decimals: Option<i32> = buf.trim().parse::<i32>().ok();

    // Build the live format from the current rounding mode + parsed decimals.
    let mut fmt = app.tabs[app.active_tab]
        .column_number_formats
        .get(&col_idx)
        .copied()
        .unwrap_or_default();
    fmt.decimals = decimals;

    // Columns the format applies to. Edited live by the in-dialog picker.
    let prev_cols = app.tabs[app.active_tab].column_format_cols.clone();
    let mut selected_cols = prev_cols.clone();

    let mut open = true;
    let mut close = false;
    let mut clear = false;
    let mut new_buf = buf.clone();

    egui::Window::new(format!(
        "{} - {column_name}",
        octa::i18n::t("column_format.title")
    ))
    // Explicit, stable id so egui doesn't restore a size persisted while the
    // dialog was a smaller, fixed layout.
    .id(egui::Id::new("octa_column_format_dialog_v2"))
    .open(&mut open)
    .resizable([true, true])
    .collapsible(false)
    .default_width(320.0)
    .default_height(420.0)
    .min_width(300.0)
    .min_height(260.0)
    .pivot(egui::Align2::CENTER_CENTER)
    .default_pos(ctx.content_rect().center())
    .show(ctx, |ui| {
        // Footer first (bottom panel) so the buttons stay pinned and visible
        // no matter how tall the column list grows.
        egui::Panel::bottom("octa_column_format_footer").show_inside(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(octa::i18n::t("column_format.done")).clicked() {
                    close = true;
                }
                if ui
                    .button(octa::i18n::t("column_format.clear_format"))
                    .clicked()
                {
                    clear = true;
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            // Decimals: free-text signed integer. Empty = Auto.
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("column_format.decimals"));
                ui.add(
                    egui::TextEdit::singleline(&mut new_buf)
                        .desired_width(56.0)
                        .hint_text(octa::i18n::t("column_format.auto")),
                );
            });
            // Always-visible hint - the negative behaviour isn't obvious.
            ui.label(
                egui::RichText::new(octa::i18n::t("column_format.decimals_hint"))
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );

            ui.add_space(4.0);
            ui.add_enabled_ui(fmt.decimals.is_some(), |ui| {
                ui.horizontal(|ui| {
                    ui.label(octa::i18n::t("column_format.rounding"));
                    for mode in octa::data::num_format::RoundingMode::ALL {
                        ui.radio_value(&mut fmt.rounding, *mode, mode.label_t());
                    }
                });
            });

            ui.add_space(8.0);
            // Live preview against the first non-null numeric cell, falling
            // back to a sample value so the user always sees something.
            let sample = first_numeric_sample(app, col_idx).unwrap_or(CellValue::Float(1234.5678));
            let preview = format_cell_number(
                &sample,
                Some(fmt),
                app.settings.thousands_separators_in_cells,
                app.settings.number_separator_style,
            )
            .unwrap_or_default();
            ui.label(
                egui::RichText::new(format!(
                    "{} {preview}",
                    octa::i18n::t("column_format.preview")
                ))
                .color(ui.visuals().weak_text_color()),
            );

            ui.separator();
            ui.label(octa::i18n::t("column_format.apply_to"));
            ui.horizontal(|ui| {
                if ui
                    .button(octa::i18n::t("column_format.select_all"))
                    .clicked()
                {
                    selected_cols = numeric_cols.iter().map(|(c, _)| *c).collect();
                }
                if ui
                    .button(octa::i18n::t("column_format.select_none"))
                    .clicked()
                {
                    selected_cols.clear();
                }
            });
            // Scrollable checkbox list of numeric columns; fills the
            // resizable window so dragging it taller shows more columns.
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (c, name) in &numeric_cols {
                        let mut checked = selected_cols.contains(c);
                        if ui.checkbox(&mut checked, name).changed() {
                            if checked {
                                if !selected_cols.contains(c) {
                                    selected_cols.push(*c);
                                }
                            } else {
                                selected_cols.retain(|x| x != c);
                            }
                        }
                    }
                });
        });
    });

    selected_cols.sort_unstable();
    selected_cols.dedup();

    let tab = &mut app.tabs[app.active_tab];

    if clear {
        // Drop the format from every column this dialog touched.
        for c in prev_cols.iter().chain(selected_cols.iter()) {
            tab.column_number_formats.remove(c);
        }
        tab.column_format_decimals_buf.clear();
        tab.column_format_cols.clear();
        tab.column_format_col = None;
        return;
    }

    // Persist the buffer and apply the format live to every selected column.
    tab.column_format_decimals_buf = new_buf;
    // Columns unchecked this frame lose their format.
    for c in &prev_cols {
        if !selected_cols.contains(c) {
            tab.column_number_formats.remove(c);
        }
    }
    for c in &selected_cols {
        if fmt == NumberFormat::default() {
            // No-op format (Auto decimals, default rounding) - drop the entry.
            tab.column_number_formats.remove(c);
        } else {
            tab.column_number_formats.insert(*c, fmt);
        }
    }
    tab.column_format_cols = selected_cols;

    if close || !open {
        tab.column_format_col = None;
    }
}

/// First non-null `Int`/`Float` cell in the column, for the live preview.
fn first_numeric_sample(app: &OctaApp, col_idx: usize) -> Option<CellValue> {
    let table = &app.tabs[app.active_tab].table;
    for row in 0..table.row_count().min(1000) {
        match table.get(row, col_idx) {
            Some(v @ CellValue::Int(_)) | Some(v @ CellValue::Float(_)) => return Some(v.clone()),
            _ => {}
        }
    }
    None
}
