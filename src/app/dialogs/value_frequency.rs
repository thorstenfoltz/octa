//! Value-frequency dialog: top-N most common values in a column, with
//! their counts and percentages. Opened by:
//!
//! - Column-header right-click → "Value frequency…"
//! - `ShortcutAction::ColumnValueFrequency` (default Ctrl+Shift+I), which
//!   targets the column of the currently selected cell.
//!
//! Compute lives in `octa::data::value_frequency`; this file only renders.

use eframe::egui;
use egui::RichText;
use egui_extras::{Column, TableBuilder};

use octa::data::is_numeric_data_type;
use octa::data::value_frequency::{BinningMode, compute_value_frequency};
use octa::ui::settings::{DialogSize, draw_window_controls};

use super::super::state::OctaApp;

/// Top-N presets shown in the toolbar. `None` means "all distinct values".
const TOP_N_PRESETS: &[(Option<usize>, &str)] = &[
    (Some(20), "Top 20"),
    (Some(50), "Top 50"),
    (Some(100), "Top 100"),
    (Some(500), "Top 500"),
    (None, "All"),
];

pub(crate) fn render_value_frequency_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(col_idx) = app.tabs[app.active_tab].value_frequency_col else {
        return;
    };

    // Guard: the active tab might have lost the column (load_file replaced
    // the table while the dialog flag persisted). Close cleanly in that
    // case rather than rendering against stale state.
    let col_count = app.tabs[app.active_tab].table.col_count();
    if col_idx >= col_count {
        app.tabs[app.active_tab].value_frequency_col = None;
        return;
    }

    let top_n = app.tabs[app.active_tab].value_frequency_top_n;
    let bin = app.tabs[app.active_tab].value_frequency_bin_numeric;
    let mut size = app.tabs[app.active_tab].value_frequency_size;
    let mut top_n_state = top_n;
    let mut bin_state = bin;
    let mut close_requested = false;
    let mut copy_payload: Option<String> = None;
    let mut filter_to_this: Option<String> = None;

    let (column_name, is_numeric) = {
        let tab = &app.tabs[app.active_tab];
        let col = &tab.table.columns[col_idx];
        (col.name.clone(), is_numeric_data_type(&col.data_type))
    };

    let binning_mode = if is_numeric && bin {
        BinningMode::Sturges
    } else {
        BinningMode::None
    };

    let freq = {
        let tab = &app.tabs[app.active_tab];
        match compute_value_frequency(&tab.table, col_idx, top_n, binning_mode) {
            Some(f) => f,
            None => {
                // Shouldn't reach here — the bounds check above already
                // ruled out an out-of-range index — but the function returns
                // Option for caller safety. Close defensively.
                app.tabs[app.active_tab].value_frequency_col = None;
                return;
            }
        }
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
                        RichText::new(format!("Value Frequency — {}", column_name))
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

        egui::Panel::bottom("value_frequency_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        close_requested = true;
                    }
                    if ui.button("Copy as TSV").clicked() {
                        copy_payload = Some(build_tsv(&column_name, &freq));
                    }
                    ui.label(
                        RichText::new(format!(
                            "{} distinct · {} non-null · {} null{}",
                            freq.unique_count,
                            freq.total_non_null,
                            freq.nulls,
                            if freq.binned { " · binned" } else { "" }
                        ))
                        .size(10.0)
                        .color(ui.visuals().weak_text_color()),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Show:");
                    for (preset, label) in TOP_N_PRESETS {
                        let selected = top_n_state == *preset;
                        if ui.selectable_label(selected, *label).clicked() {
                            top_n_state = *preset;
                        }
                    }
                    if is_numeric {
                        ui.add_space(8.0);
                        ui.checkbox(&mut bin_state, "Bin numeric values (Sturges)");
                    }
                });
                ui.separator();

                if freq.rows.is_empty() {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(
                            "No non-null values in this column. \
                             Nothing to count.",
                        )
                        .color(ui.visuals().weak_text_color()),
                    );
                    return;
                }

                let total_for_pct = freq.total_non_null.max(1) as f64;
                let body_height = ui.available_height();
                // Capture the weak text color before TableBuilder takes the
                // exclusive borrow on `ui` — looking it up inside the body
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
                        for h in ["#", "Value", "Count", "%"] {
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
                                        if ui.button("Copy value").clicked() {
                                            copy_payload = Some(row.label.clone());
                                            ui.close();
                                        }
                                        if ui.button("Filter table to this value").clicked() {
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
        app.status_message = Some((
            "Copied value-frequency data".to_string(),
            std::time::Instant::now(),
        ));
    }

    if let Some(value) = filter_to_this {
        // Add a column filter limiting the active tab to this exact value.
        // Same path as the column-filter dialog's apply step.
        let tab = &mut app.tabs[app.active_tab];
        let mut allow = std::collections::HashSet::new();
        allow.insert(value);
        tab.column_filters.insert(col_idx, allow);
        tab.filter_dirty = true;
    }

    let tab = &mut app.tabs[app.active_tab];
    tab.value_frequency_top_n = top_n_state;
    tab.value_frequency_bin_numeric = bin_state;
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
