//! Value-frequency dialog: top-N most common values in a column, with
//! their counts and percentages. Opened by:
//!
//! - Column-header right-click -> "Value frequency..."
//! - **Analyse -> Value frequency...** (via the column picker)
//! - `ShortcutAction::ColumnValueFrequency` (default Ctrl+Shift+I), which
//!   targets the column of the currently selected cell.
//!
//! Compute lives in `octa::data::value_frequency`; this file only renders.
//! The controls (Top-N / binning / bin count) are drawn *before* the result
//! is computed, so edits take effect in the same frame.

use eframe::egui;
use egui::RichText;
use egui_extras::{Column, TableBuilder};

use octa::data::is_numeric_data_type;
use octa::data::value_frequency::{BinningMode, compute_value_frequency};
use octa::ui::settings::{DialogSize, draw_window_controls};

use super::super::state::OctaApp;

/// Top-N presets shown in the toolbar. `None` means "all distinct values".
/// Labels are built at render time via `t("dialog.vf_top")` + the number.
const TOP_N_PRESETS: &[Option<usize>] = &[Some(20), Some(50), Some(100), Some(500), None];

pub(crate) fn render_value_frequency_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let active = app.active_tab;
    let Some(col_idx) = app.tabs[active].value_frequency_col else {
        return;
    };

    // Guard: the active tab might have lost the column (load_file replaced
    // the table while the dialog flag persisted). Close cleanly in that
    // case rather than rendering against stale state.
    let col_count = app.tabs[active].table.col_count();
    if col_idx >= col_count {
        app.tabs[active].value_frequency_col = None;
        return;
    }

    let top_n = app.tabs[active].value_frequency_top_n;
    let bin = app.tabs[active].value_frequency_bin_numeric;
    let mut size = app.tabs[active].value_frequency_size;
    let mut top_n_state = top_n;
    let mut bin_state = bin;
    let mut bins_buf = app.tabs[active].value_frequency_bins_buf.clone();
    let mut close_requested = false;
    let mut copy_payload: Option<String> = None;
    let mut filter_to_this: Option<String> = None;

    let (column_name, is_numeric) = {
        let tab = &app.tabs[active];
        let col = &tab.table.columns[col_idx];
        (col.name.clone(), is_numeric_data_type(&col.data_type))
    };

    let mut window = egui::Window::new("Value Frequency")
        .title_bar(false)
        .collapsible(false);
    window = match size {
        DialogSize::Maximized => window.fixed_rect(ctx.content_rect().shrink(8.0)),
        DialogSize::Minimized => window.resizable(false),
        DialogSize::Normal => window
            .resizable(true)
            .default_width(520.0)
            .default_height(520.0)
            .min_width(360.0)
            .min_height(220.0),
    };
    let minimized = size == DialogSize::Minimized;

    window.show(ctx, |ui| {
        egui::Panel::top("value_frequency_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} - {}",
                            octa::i18n::t("dialog.vf_title"),
                            column_name
                        ))
                        .strong()
                        .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close_requested = true;
                        }
                    });
                });
            });

        if minimized {
            return;
        }

        // Controls first, so the result below reflects this frame's edits.
        egui::Panel::top("value_frequency_controls")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                let binning_active = is_numeric && bin_state;
                ui.horizontal_wrapped(|ui| {
                    // Top-N only applies to raw value counts; when binning, the
                    // bin count is the control, so hide the presets.
                    if !binning_active {
                        ui.label(octa::i18n::t("dialog.vf_show"));
                        for preset in TOP_N_PRESETS {
                            let label = match preset {
                                Some(n) => format!("{} {}", octa::i18n::t("dialog.vf_top"), n),
                                None => octa::i18n::t("dialog.sel_all"),
                            };
                            let selected = top_n_state == *preset;
                            if ui.selectable_label(selected, label).clicked() {
                                top_n_state = *preset;
                            }
                        }
                    }
                    if is_numeric {
                        if !binning_active {
                            ui.add_space(8.0);
                        }
                        ui.checkbox(&mut bin_state, octa::i18n::t("dialog.vf_bin_numeric"));
                        if bin_state {
                            ui.add_space(4.0);
                            ui.label(octa::i18n::t("dialog.vf_bins"));
                            ui.add(
                                egui::TextEdit::singleline(&mut bins_buf)
                                    .desired_width(48.0)
                                    .hint_text(octa::i18n::t("dialog.vf_auto")),
                            )
                            .on_hover_text(octa::i18n::t("dialog.vf_bins_hint"));
                            ui.label(
                                egui::RichText::new(octa::i18n::t("dialog.vf_ranges_note"))
                                    .small()
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                    }
                });
            });

        // Compute the result from the live control values (no frame lag).
        let custom_bins: Option<usize> = bins_buf.trim().parse::<usize>().ok().filter(|n| *n > 0);
        let binning_mode = if is_numeric && bin_state {
            match custom_bins {
                Some(n) => BinningMode::Custom(n),
                None => BinningMode::Sturges,
            }
        } else {
            BinningMode::None
        };
        let Some(freq) =
            compute_value_frequency(&app.tabs[active].table, col_idx, top_n_state, binning_mode)
        else {
            // Bounds already checked above; defensive only.
            return;
        };

        egui::Panel::bottom("value_frequency_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("common.close")).clicked() {
                        close_requested = true;
                    }
                    if ui.button(octa::i18n::t("dialog.vf_copy_tsv")).clicked() {
                        copy_payload = Some(build_tsv(&column_name, &freq));
                    }
                    ui.label(
                        RichText::new(format!(
                            "{} {} | {} {} | {} {}{}",
                            freq.unique_count,
                            octa::i18n::t("dialog.vf_distinct"),
                            freq.total_non_null,
                            octa::i18n::t("dialog.vf_non_null"),
                            freq.nulls,
                            octa::i18n::t("dialog.vf_null"),
                            if freq.binned {
                                format!(" | {}", octa::i18n::t("dialog.vf_binned"))
                            } else {
                                String::new()
                            }
                        ))
                        .size(10.0)
                        .color(ui.visuals().weak_text_color()),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show_inside(ui, |ui| {
                if freq.rows.is_empty() {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.vf_no_values"))
                            .color(ui.visuals().weak_text_color()),
                    );
                    return;
                }

                let total_for_pct = freq.total_non_null.max(1) as f64;
                let body_height = ui.available_height();
                // Capture the weak text color before TableBuilder takes the
                // exclusive borrow on `ui` - looking it up inside the body
                // closure would re-borrow `ui` and the compiler rejects.
                let weak_text = ui.visuals().weak_text_color();

                TableBuilder::new(ui)
                    .sense(egui::Sense::click())
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::initial(40.0).at_least(32.0))
                    .column(Column::initial(260.0).at_least(120.0).clip(true))
                    .column(Column::initial(80.0).at_least(60.0))
                    .column(Column::initial(80.0).at_least(60.0))
                    .max_scroll_height(body_height)
                    .header(24.0, |mut header| {
                        let headers = [
                            "#".to_string(),
                            octa::i18n::t("dialog.vf_col_value"),
                            octa::i18n::t("dialog.vf_col_count"),
                            "%".to_string(),
                        ];
                        for h in headers {
                            header.col(|ui| {
                                ui.label(RichText::new(h).strong());
                            });
                        }
                    })
                    .body(|mut body| {
                        for (i, row) in freq.rows.iter().enumerate() {
                            body.row(22.0, |mut tr| {
                                tr.col(|ui| {
                                    ui.label(RichText::new(format!("{}", i + 1)).color(weak_text));
                                });
                                let mut value_resp = None;
                                tr.col(|ui| {
                                    let resp = ui.add(
                                        egui::Label::new(row.label.clone())
                                            .selectable(false)
                                            .truncate(),
                                    );
                                    value_resp = Some(resp);
                                });
                                tr.col(|ui| {
                                    ui.label(format!("{}", row.count));
                                });
                                tr.col(|ui| {
                                    let pct = (row.count as f64 / total_for_pct) * 100.0;
                                    ui.label(format!("{:.1}%", pct));
                                });
                                if let Some(resp) = value_resp
                                    && !freq.binned
                                {
                                    resp.context_menu(|ui| {
                                        if ui
                                            .button(octa::i18n::t("dialog.vf_copy_value"))
                                            .clicked()
                                        {
                                            copy_payload = Some(row.label.clone());
                                            ui.close();
                                        }
                                        if ui
                                            .button(octa::i18n::t("dialog.vf_filter_to_value"))
                                            .clicked()
                                        {
                                            filter_to_this = Some(row.label.clone());
                                            ui.close();
                                        }
                                    });
                                }
                            });
                        }
                    });
            });
    });

    if let Some(payload) = copy_payload {
        ctx.copy_text(payload);
        app.status_message = Some((octa::i18n::t("dialog.vf_copied"), std::time::Instant::now()));
    }

    if let Some(value) = filter_to_this {
        // Add a column filter limiting the active tab to this exact value.
        // Same path as the column-filter dialog's apply step.
        let tab = &mut app.tabs[active];
        let mut allow = std::collections::HashSet::new();
        allow.insert(value);
        tab.column_filters.insert(col_idx, allow);
        tab.filter_dirty = true;
    }

    let custom_bins: Option<usize> = bins_buf.trim().parse::<usize>().ok().filter(|n| *n > 0);
    let tab = &mut app.tabs[active];
    tab.value_frequency_top_n = top_n_state;
    tab.value_frequency_bin_numeric = bin_state;
    tab.value_frequency_bins = custom_bins;
    tab.value_frequency_bins_buf = bins_buf;
    tab.value_frequency_size = size;
    if close_requested {
        tab.value_frequency_col = None;
        tab.value_frequency_size = DialogSize::Normal;
    }
}

fn build_tsv(column_name: &str, freq: &octa::data::value_frequency::ValueFrequency) -> String {
    let mut s = String::new();
    s.push_str(&format!("{}\tcount\tpercent\n", column_name));
    let total = freq.total_non_null.max(1) as f64;
    for row in &freq.rows {
        let pct = (row.count as f64 / total) * 100.0;
        s.push_str(&format!("{}\t{}\t{:.1}\n", row.label, row.count, pct));
    }
    s
}
