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

    let mut open = true;
    let mut close = false;
    let mut clear = false;
    let mut new_buf = buf.clone();

    egui::Window::new(format!(
        "{} - {column_name}",
        octa::i18n::t("column_format.title")
    ))
    .open(&mut open)
    .resizable(true)
    .collapsible(false)
    .default_width(300.0)
    .pivot(egui::Align2::CENTER_CENTER)
    .default_pos(ctx.content_rect().center())
    .show(ctx, |ui| {
        ui.set_min_width(280.0);

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

        ui.add_space(10.0);
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

    let tab = &mut app.tabs[app.active_tab];

    if clear {
        tab.column_number_formats.remove(&col_idx);
        tab.column_format_decimals_buf.clear();
        tab.column_format_col = None;
        return;
    }

    // Persist the buffer and apply the format live.
    tab.column_format_decimals_buf = new_buf;
    if fmt == NumberFormat::default() {
        // No-op format (Auto decimals, default rounding) - drop the entry.
        tab.column_number_formats.remove(&col_idx);
    } else {
        tab.column_number_formats.insert(col_idx, fmt);
    }

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
