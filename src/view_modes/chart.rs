//! Chart tab renderer. Top control bar lets the user pick a chart kind,
//! X column, Y columns, aggregation, and styling (title, axis labels,
//! legend, per-series renames + colors). Below that, an `egui_plot::Plot`
//! draws the chart from the prepped data.
//!
//! The chart is opened as its own tab (see `OctaApp::open_chart_tab`); this
//! renderer doesn't care how it got there. Data prep + sampling live in
//! `octa::data::chart`; export to PNG / SVG / PDF lives in
//! `octa::data::chart_export`.

use eframe::egui;
use egui_plot::{
    Bar, BarChart, BoxElem, BoxPlot, BoxSpread, Corner, Legend, Line, MarkerShape, Plot,
    PlotPoints, Points,
};

use crate::app::state::TabState;
use crate::ui::theme::{ThemeColors, ThemeMode};
use octa::data::chart::{
    Aggregation, ChartData, ChartKind, ChartLimits, LegendPosition, MAX_HIST_BINS, SeriesStyle,
    XAxisKind, build_chart, format_days_as_date, format_seconds_as_datetime, has_numeric_column,
};
use octa::data::chart_export::{self, ExportOptions};

/// Public entry point. Driven by `central_panel::render_central_panel` when
/// the active tab's `view_mode == ViewMode::Chart` (or the tab is a chart tab).
pub fn render_chart_view(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    theme_mode: ThemeMode,
    limits: ChartLimits,
) {
    if tab.table.col_count() == 0 {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("Open a file with columns to chart it.").weak());
        });
        return;
    }
    if !has_numeric_column(&tab.table) {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new("This table has no numeric columns — nothing to plot.").weak(),
            );
        });
        return;
    }
    seed_defaults(tab);

    let colors = ThemeColors::for_mode(theme_mode);
    draw_controls(ui, tab, &colors);
    ui.separator();

    let cfg = tab.chart_config.clone();
    let filtered: Vec<usize> = tab.filtered_rows.clone();
    let prep = build_chart(&tab.table, &filtered, &cfg, limits);

    match prep {
        Err(err) => {
            ui.add_space(8.0);
            ui.colored_label(colors.warning, err.message());
        }
        Ok(prep) => {
            // Title (if set) above the plot.
            if !cfg.title.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.heading(&cfg.title);
                });
            }
            // Sampling pill.
            ui.horizontal(|ui| {
                if prep.used_rows < prep.total_rows {
                    ui.label(
                        egui::RichText::new(format!(
                            "Sampled {} of {} rows",
                            fmt_count(prep.used_rows),
                            fmt_count(prep.total_rows)
                        ))
                        .small()
                        .color(colors.warning),
                    )
                    .on_hover_text(
                        "Plot evenly-spaces samples above 'Chart max points'. \
                         Filter the table or raise the cap in Settings \u{2192} \
                         Performance to plot every row.",
                    );
                } else {
                    ui.label(
                        egui::RichText::new(format!("{} rows", fmt_count(prep.total_rows)))
                            .small()
                            .weak(),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    draw_export_buttons(ui, &prep, &cfg);
                });
            });

            let x_axis_label = pick_label(&cfg.x_label_override, &prep.x_label);
            let y_axis_label_base = pick_label(&cfg.y_label_override, &prep.y_label);
            let y_axis_label = if cfg.y_log_scale {
                format!("{y_axis_label_base} (log10)")
            } else {
                y_axis_label_base
            };
            let plot_id = format!("chart_plot_{:?}", cfg.kind);

            // X-axis tick formatter selection:
            //  - Categorical → look up the category name at the integer
            //    tick position.
            //  - Date → days since 1970-01-01 → YYYY-MM-DD.
            //  - DateTime → seconds since the Unix epoch → YYYY-MM-DD HH:MM:SS.
            //  - Numeric → leave egui_plot's default formatter alone.
            let categories = prep.data.x_axis_categories().unwrap_or_default();
            let x_axis_kind = prep.x_axis_kind;
            let mut plot = Plot::new(plot_id)
                .x_axis_label(x_axis_label)
                .y_axis_label(y_axis_label)
                .show_grid(cfg.show_grid);
            if !categories.is_empty() {
                plot = plot.x_axis_formatter(move |mark, _range| {
                    let idx = mark.value.round() as i64;
                    if idx >= 0 && (idx as usize) < categories.len() {
                        let v = mark.value;
                        if (v - idx as f64).abs() < 1e-6 {
                            return categories[idx as usize].clone();
                        }
                    }
                    String::new()
                });
            } else {
                match x_axis_kind {
                    XAxisKind::Date => {
                        plot = plot.x_axis_formatter(|mark, _range| {
                            // Only label whole-day ticks; intermediate
                            // sub-day marks otherwise duplicate the date.
                            if (mark.value - mark.value.round()).abs() < 1e-3 {
                                format_days_as_date(mark.value.round())
                            } else {
                                String::new()
                            }
                        });
                    }
                    XAxisKind::DateTime => {
                        plot = plot.x_axis_formatter(|mark, _range| {
                            format_seconds_as_datetime(mark.value)
                        });
                    }
                    XAxisKind::Numeric => {}
                }
            }
            // Y-axis bounds: when both min and max are set we force them as
            // the default bounds. Half-set is ignored — partial bounds make
            // the bounding box meaningless. Log-scale projects the user
            // values into log10 space.
            if let (Some(mut y_min), Some(mut y_max)) = (cfg.y_min, cfg.y_max)
                && y_min < y_max
            {
                if cfg.y_log_scale {
                    y_min = log10_safe(y_min);
                    y_max = log10_safe(y_max);
                }
                if y_min.is_finite() && y_max.is_finite() && y_min < y_max {
                    plot = plot.default_y_bounds(y_min, y_max);
                }
            }
            // Y grid spacer: emit ticks every `step` units in the visible
            // range. Honoured by all chart kinds.
            if let Some(step) = cfg.y_step
                && step > 0.0
            {
                plot = plot.y_grid_spacer(move |input| {
                    let mut marks = Vec::new();
                    let mut v = (input.bounds.0 / step).ceil() * step;
                    let mut emitted = 0i64;
                    // Soft cap so a tiny step on a wide range doesn't generate
                    // millions of marks and lock the renderer.
                    const MARK_LIMIT: i64 = 10_000;
                    while v <= input.bounds.1 && emitted < MARK_LIMIT {
                        marks.push(egui_plot::GridMark {
                            value: v,
                            step_size: step,
                        });
                        v += step;
                        emitted += 1;
                    }
                    marks
                });
            }
            // Y axis tick formatter — integer rounding when `y_integer_only`,
            // or 10^N notation when log-scaled (the formatter takes log10
            // values and converts back to the original magnitude).
            let log = cfg.y_log_scale;
            let integer_only = cfg.y_integer_only;
            if log || integer_only {
                plot = plot.y_axis_formatter(move |mark, _range| {
                    if log {
                        // 10^mark.value, rendered compactly.
                        let v = 10f64.powf(mark.value);
                        if v >= 1.0 && v.fract() == 0.0 {
                            format!("{v:.0}")
                        } else {
                            format!("{v:.2}")
                        }
                    } else if integer_only {
                        format!("{:.0}", mark.value)
                    } else {
                        format!("{}", mark.value)
                    }
                });
            }
            if cfg.legend != LegendPosition::Off {
                plot = plot.legend(Legend::default().position(map_legend(cfg.legend)));
            }
            plot.show(ui, |plot_ui| {
                draw_plot_items(plot_ui, &prep.data, &cfg);
            });
        }
    }
}

/// On first entry: pick a sensible X column (first numeric one) and an
/// empty Y for kinds that need it. The user can change anything afterwards.
fn seed_defaults(tab: &mut TabState) {
    if tab.chart_config.x_col.is_none() {
        if let Some(idx) = first_numeric_col(tab) {
            tab.chart_config.x_col = Some(idx);
        } else if tab.table.col_count() > 0 {
            tab.chart_config.x_col = Some(0);
        }
    }
    if tab.chart_config.kind.needs_y()
        && tab.chart_config.y_cols.is_empty()
        && let Some(idx) = first_numeric_col_excluding(tab, tab.chart_config.x_col)
    {
        tab.chart_config.y_cols.push(idx);
    }
}

fn first_numeric_col(tab: &TabState) -> Option<usize> {
    tab.table
        .columns
        .iter()
        .position(|c| octa::data::is_numeric_data_type(&c.data_type))
}

fn first_numeric_col_excluding(tab: &TabState, excluded: Option<usize>) -> Option<usize> {
    tab.table.columns.iter().enumerate().find_map(|(i, c)| {
        (Some(i) != excluded && octa::data::is_numeric_data_type(&c.data_type)).then_some(i)
    })
}

/// `log10(v)` that returns `f64::NEG_INFINITY` for `v <= 0` so callers can
/// `is_finite()`-filter the result without a separate guard.
fn log10_safe(v: f64) -> f64 {
    if v > 0.0 {
        v.log10()
    } else {
        f64::NEG_INFINITY
    }
}

/// Apply log10 to the Y component of each `[x, y]` point, dropping points
/// where Y is non-positive (log10 undefined). Used by the renderer when
/// `cfg.y_log_scale` is on so all chart kinds share one transformation.
fn log_transform_points(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    points
        .iter()
        .filter_map(|p| {
            let ly = log10_safe(p[1]);
            ly.is_finite().then_some([p[0], ly])
        })
        .collect()
}

fn map_legend(p: LegendPosition) -> Corner {
    match p {
        LegendPosition::TopLeft => Corner::LeftTop,
        LegendPosition::TopRight => Corner::RightTop,
        LegendPosition::BottomLeft => Corner::LeftBottom,
        LegendPosition::BottomRight => Corner::RightBottom,
        // `Off` is filtered before this is called.
        LegendPosition::Off => Corner::RightTop,
    }
}

fn pick_label(override_: &str, fallback: &str) -> String {
    if override_.is_empty() {
        fallback.to_string()
    } else {
        override_.to_string()
    }
}

/// Y-axis Min / Max / Step input.
///
/// Renders as a plain `TextEdit` so hovering doesn't flash the horizontal-
/// resize cursor egui's `DragValue` always shows. The buffer is the source
/// of truth for *what's typed*; we re-parse on every change and write back
/// into `Option<f64>`. An empty buffer (or one that fails to parse) maps
/// to `None`, which the renderer reads as "auto-fit".
///
/// `positive_only` filters values `<= 0` — used for the Y step where zero /
/// negative either no-ops or upsets the grid spacer.
fn optional_f64_input(
    ui: &mut egui::Ui,
    id_salt: &str,
    buffer: &mut String,
    value: &mut Option<f64>,
    positive_only: bool,
) {
    let response = ui.add(
        egui::TextEdit::singleline(buffer)
            .id_salt(id_salt)
            .desired_width(80.0)
            .hint_text("Auto"),
    );
    if response.changed() {
        let trimmed = buffer.trim();
        if trimmed.is_empty() {
            *value = None;
        } else if let Ok(v) = trimmed.parse::<f64>() {
            if positive_only && v <= 0.0 {
                *value = None;
            } else {
                *value = Some(v);
            }
        } else {
            // Leave value unchanged — the user's typing transient bytes
            // ("1.2e", "-", etc.) that aren't yet parseable. They'll
            // finish typing and the next changed event lands.
        }
    }
}

fn draw_controls(ui: &mut egui::Ui, tab: &mut TabState, colors: &ThemeColors) {
    // Row 1: kind + X/Y pickers + agg / bin picker.
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Chart:")
                .color(colors.text_primary)
                .strong(),
        );
        let kind_before = tab.chart_config.kind;
        egui::ComboBox::from_id_salt("chart_kind_combo")
            .selected_text(tab.chart_config.kind.label())
            .show_ui(ui, |ui| {
                for &k in ChartKind::ALL {
                    ui.selectable_value(&mut tab.chart_config.kind, k, k.label());
                }
            });
        if kind_before != tab.chart_config.kind && !tab.chart_config.kind.needs_y() {
            tab.chart_config.y_cols.clear();
        }

        ui.separator();
        ui.label("X:");
        let column_names: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();
        col_picker(
            ui,
            "chart_x_combo",
            &mut tab.chart_config.x_col,
            &column_names,
        );

        if tab.chart_config.kind.needs_y() {
            ui.separator();
            ui.label("Y:");
            y_picker(ui, &mut tab.chart_config.y_cols, &column_names);
        }

        match tab.chart_config.kind {
            ChartKind::Bar => {
                ui.separator();
                ui.label("Agg:");
                egui::ComboBox::from_id_salt("chart_agg_combo")
                    .selected_text(tab.chart_config.agg.label())
                    .show_ui(ui, |ui| {
                        for &a in Aggregation::ALL {
                            ui.selectable_value(&mut tab.chart_config.agg, a, a.label());
                        }
                    });
            }
            ChartKind::Histogram => {
                ui.separator();
                ui.label("Bins:").on_hover_text(
                    "A histogram counts how many values fall into each \
                     range. \"Bins\" is the number of those ranges — fewer \
                     bins make a coarser shape, more bins reveal detail \
                     but get noisy. \"Auto (Sturges)\" picks a count from \
                     row count via ceil(1 + log2(n)) clamped to [5, 50].",
                );
                let mut auto = tab.chart_config.hist_bins.is_none();
                if ui.checkbox(&mut auto, "Auto (Sturges)").changed() {
                    if auto {
                        tab.chart_config.hist_bins = None;
                        tab.chart_buffers.hist_bins.clear();
                    } else {
                        tab.chart_config.hist_bins = Some(20);
                        tab.chart_buffers.hist_bins = "20".to_string();
                    }
                }
                if !auto {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut tab.chart_buffers.hist_bins)
                            .id_salt("chart_hist_bins")
                            .desired_width(80.0)
                            .hint_text("20"),
                    );
                    if response.changed() {
                        let trimmed = tab.chart_buffers.hist_bins.trim();
                        if let Ok(n) = trimmed.parse::<usize>() {
                            tab.chart_config.hist_bins = Some(n.clamp(1, MAX_HIST_BINS));
                        }
                        // Mid-typing transients (empty / "0" / non-digits)
                        // leave hist_bins unchanged — the user'll finish
                        // typing and the next change lands a valid value.
                    }
                }
            }
            _ => {}
        }
    });

    // Row 2: collapsible customisation. Laid out as three wrapping
    // horizontal groups so multiple controls share each row and the chart
    // gets more vertical real estate. `horizontal_wrapped` re-flows the
    // controls to the next line when the window is narrow, so the user
    // never has to scroll sideways.
    egui::CollapsingHeader::new("Customise")
        .id_salt("chart_customize_collapsible")
        .default_open(false)
        .show(ui, |ui| {
            // Group A — Labels + legend + grid toggle. One row.
            ui.horizontal_wrapped(|ui| {
                ui.label("Title:");
                ui.add(
                    egui::TextEdit::singleline(&mut tab.chart_config.title)
                        .desired_width(160.0)
                        .hint_text("(none)"),
                );
                ui.separator();
                ui.label("X label:");
                ui.add(
                    egui::TextEdit::singleline(&mut tab.chart_config.x_label_override)
                        .desired_width(120.0)
                        .hint_text("(auto)"),
                );
                ui.separator();
                ui.label("Y label:");
                ui.add(
                    egui::TextEdit::singleline(&mut tab.chart_config.y_label_override)
                        .desired_width(120.0)
                        .hint_text("(auto)"),
                );
                ui.separator();
                ui.label("Legend:");
                egui::ComboBox::from_id_salt("chart_legend_combo")
                    .selected_text(tab.chart_config.legend.label())
                    .show_ui(ui, |ui| {
                        for &p in LegendPosition::ALL {
                            ui.selectable_value(&mut tab.chart_config.legend, p, p.label());
                        }
                    });
                ui.separator();
                ui.checkbox(&mut tab.chart_config.show_grid, "Show grid");
            });

            // Group B — Y axis. One row.
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Y axis:").strong());
                ui.label("Min:").on_hover_text(
                    "Lower bound on the Y axis (original-data units). \
                     Both Min and Max must be set for the bounds to take \
                     effect — half-set is ignored. Leave blank for auto.",
                );
                optional_f64_input(
                    ui,
                    "chart_y_min",
                    &mut tab.chart_buffers.y_min,
                    &mut tab.chart_config.y_min,
                    false,
                );
                ui.label("Max:");
                optional_f64_input(
                    ui,
                    "chart_y_max",
                    &mut tab.chart_buffers.y_max,
                    &mut tab.chart_config.y_max,
                    false,
                );
                ui.label("Step:").on_hover_text(
                    "Custom Y-axis grid step (original-data units). \
                     Leave blank to let egui_plot pick.",
                );
                optional_f64_input(
                    ui,
                    "chart_y_step",
                    &mut tab.chart_buffers.y_step,
                    &mut tab.chart_config.y_step,
                    true,
                );
                ui.separator();
                ui.checkbox(&mut tab.chart_config.y_integer_only, "Integers only")
                    .on_hover_text(
                        "Format Y-axis ticks as whole numbers — useful for \
                         counts where the default 1.0 / 2.0 reads oddly.",
                    );
                ui.checkbox(&mut tab.chart_config.y_log_scale, "Log scale")
                    .on_hover_text(
                        "Apply log10 to the Y values before plotting. \
                         Non-positive values are dropped (log10 undefined). \
                         The Y axis label gets a '(log10)' suffix.",
                    );
            });
            // Group C — Series. Each Y-column gets a single horizontal
            // row (column name → label override → color picker). Wrapped
            // so multi-Y charts still fit a narrow window.
            if tab.chart_config.kind.needs_y() && !tab.chart_config.y_cols.is_empty() {
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Series:").strong());
                    let y_cols = tab.chart_config.y_cols.clone();
                    let column_names: Vec<String> =
                        tab.table.columns.iter().map(|c| c.name.clone()).collect();
                    for (i, col_idx) in y_cols.iter().enumerate() {
                        if i > 0 {
                            ui.separator();
                        }
                        let col_name = column_names
                            .get(*col_idx)
                            .cloned()
                            .unwrap_or_else(|| format!("col_{col_idx}"));
                        ui.label(egui::RichText::new(&col_name).monospace().small());
                        let style = tab.chart_config.series_styles.entry(*col_idx).or_default();
                        ui.add(
                            egui::TextEdit::singleline(&mut style.display_name)
                                .desired_width(120.0)
                                .hint_text(&col_name),
                        );
                        let mut on = style.color.is_some();
                        if ui
                            .checkbox(&mut on, "")
                            .on_hover_text("Custom color")
                            .changed()
                        {
                            style.color = if on {
                                Some([0x4c, 0x72, 0xb0, 0xff])
                            } else {
                                None
                            };
                        }
                        if let Some(ref mut c) = style.color {
                            // egui's color_edit_button_rgba takes &mut Rgba,
                            // so we stage in a local then write the u8 quad
                            // back if it changed.
                            let mut staged = egui::Rgba::from_rgba_unmultiplied(
                                c[0] as f32 / 255.0,
                                c[1] as f32 / 255.0,
                                c[2] as f32 / 255.0,
                                c[3] as f32 / 255.0,
                            );
                            if egui::color_picker::color_edit_button_rgba(
                                ui,
                                &mut staged,
                                egui::color_picker::Alpha::Opaque,
                            )
                            .changed()
                            {
                                let arr = staged.to_array();
                                *c = [
                                    (arr[0] * 255.0).round() as u8,
                                    (arr[1] * 255.0).round() as u8,
                                    (arr[2] * 255.0).round() as u8,
                                    (arr[3] * 255.0).round() as u8,
                                ];
                            }
                        }
                    }
                });
            }
        });
}

fn draw_export_buttons(
    ui: &mut egui::Ui,
    prep: &octa::data::chart::ChartPrep,
    cfg: &octa::data::chart::ChartConfig,
) {
    // Build options once — the three buttons all reuse the same SVG.
    let opts = ExportOptions::from_prep(
        prep,
        cfg.title.clone(),
        &cfg.x_label_override,
        &cfg.y_label_override,
        cfg.legend,
        |idx| {
            cfg.y_cols
                .get(idx)
                .and_then(|col_idx| cfg.series_styles.get(col_idx))
                .cloned()
                .unwrap_or_default()
        },
    );

    // The actual write happens on a click; the SVG/PNG/PDF byte buffer is
    // built lazily so a chart with thousands of points isn't re-encoded
    // every frame.
    if ui.button("Export PDF").clicked() {
        save_export("pdf", "Chart export (PDF)", &["pdf"], || {
            let svg = chart_export::to_svg(prep, &opts);
            chart_export::to_pdf(&svg)
        });
    }
    if ui.button("Export PNG").clicked() {
        save_export("png", "Chart export (PNG)", &["png"], || {
            let svg = chart_export::to_svg(prep, &opts);
            chart_export::to_png(&svg, 2.0)
        });
    }
    if ui.button("Export SVG").clicked() {
        save_export("svg", "Chart export (SVG)", &["svg"], || {
            Ok::<Vec<u8>, String>(chart_export::to_svg(prep, &opts).into_bytes())
        });
    }
}

/// Show a native save-file dialog, then run `build_bytes()` and write the
/// result. Threading the closure here keeps the per-format call sites
/// short (one line) and centralises the dialog + error handling.
fn save_export<F>(extension: &str, dialog_title: &str, filters: &[&str], build_bytes: F)
where
    F: FnOnce() -> Result<Vec<u8>, String>,
{
    let dialog = rfd::FileDialog::new()
        .set_title(dialog_title)
        .add_filter(extension.to_uppercase(), filters)
        .set_file_name(format!("chart.{extension}"));
    let Some(path) = dialog.save_file() else {
        return;
    };
    match build_bytes() {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(&path, bytes) {
                eprintln!("chart export: failed to write {}: {}", path.display(), e);
            }
        }
        Err(e) => {
            eprintln!("chart export: failed to render {extension}: {e}");
        }
    }
}

fn col_picker(
    ui: &mut egui::Ui,
    id_salt: &str,
    selected: &mut Option<usize>,
    column_names: &[String],
) {
    let current_label = selected
        .and_then(|i| column_names.get(i).cloned())
        .unwrap_or_else(|| "(pick…)".to_string());
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(current_label)
        .show_ui(ui, |ui| {
            for (i, name) in column_names.iter().enumerate() {
                ui.selectable_value(selected, Some(i), name);
            }
        });
}

fn y_picker(ui: &mut egui::Ui, y_cols: &mut Vec<usize>, column_names: &[String]) {
    let label = match y_cols.len() {
        0 => "(pick…)".to_string(),
        1 => column_names.get(y_cols[0]).cloned().unwrap_or_default(),
        n => format!("{n} columns"),
    };
    egui::ComboBox::from_id_salt("chart_y_combo")
        .selected_text(label)
        .show_ui(ui, |ui| {
            for (i, name) in column_names.iter().enumerate() {
                let mut on = y_cols.contains(&i);
                if ui.checkbox(&mut on, name).changed() {
                    if on {
                        if !y_cols.contains(&i) {
                            y_cols.push(i);
                        }
                    } else {
                        y_cols.retain(|c| *c != i);
                    }
                }
            }
        });
}

fn series_style_for(cfg: &octa::data::chart::ChartConfig, slot: usize) -> SeriesStyle {
    cfg.y_cols
        .get(slot)
        .and_then(|col_idx| cfg.series_styles.get(col_idx))
        .cloned()
        .unwrap_or_default()
}

fn series_color_override(
    cfg: &octa::data::chart::ChartConfig,
    slot: usize,
) -> Option<egui::Color32> {
    series_style_for(cfg, slot)
        .color
        .map(|c| egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
}

fn series_display_name(
    cfg: &octa::data::chart::ChartConfig,
    slot: usize,
    fallback: &str,
) -> String {
    let style = series_style_for(cfg, slot);
    if style.display_name.is_empty() {
        fallback.to_string()
    } else {
        style.display_name
    }
}

fn draw_plot_items(
    plot_ui: &mut egui_plot::PlotUi<'_>,
    data: &ChartData,
    cfg: &octa::data::chart::ChartConfig,
) {
    let log_y = cfg.y_log_scale;
    let xform_y = |y: f64| if log_y { log10_safe(y) } else { y };
    match data {
        ChartData::Histogram { bins, bin_width } => {
            // log10(count) is undefined at 0; skip empty bins under log scale
            // rather than painting a bar at -inf.
            let bars: Vec<Bar> = bins
                .iter()
                .filter_map(|(left, count)| {
                    let value = xform_y(*count);
                    value
                        .is_finite()
                        .then(|| Bar::new(left + bin_width / 2.0, value).width(*bin_width * 0.95))
                })
                .collect();
            let mut chart = BarChart::new("Count", bars);
            if let Some(color) = series_color_override(cfg, 0) {
                chart = chart.color(color);
            }
            plot_ui.bar_chart(chart);
        }
        ChartData::Bars {
            categories: _,
            series,
        } => {
            let series_count = series.len() as f64;
            let group_width = 0.8;
            let bar_width = group_width / series_count.max(1.0);
            for (si, ser) in series.iter().enumerate() {
                let offset = if series_count <= 1.0 {
                    0.0
                } else {
                    -group_width / 2.0 + bar_width / 2.0 + si as f64 * bar_width
                };
                let bars: Vec<Bar> = ser
                    .points
                    .iter()
                    .filter_map(|p| {
                        let value = xform_y(p[1]);
                        value
                            .is_finite()
                            .then(|| Bar::new(p[0] + offset, value).width(bar_width * 0.95))
                    })
                    .collect();
                let label = series_display_name(cfg, si, &ser.name);
                let mut chart = BarChart::new(label, bars);
                if let Some(color) = series_color_override(cfg, si) {
                    chart = chart.color(color);
                }
                plot_ui.bar_chart(chart);
            }
        }
        ChartData::Lines { series, .. } => {
            for (si, ser) in series.iter().enumerate() {
                if ser.points.is_empty() {
                    continue;
                }
                let pts: Vec<[f64; 2]> = if log_y {
                    log_transform_points(&ser.points)
                } else {
                    ser.points.clone()
                };
                if pts.is_empty() {
                    continue;
                }
                let label = series_display_name(cfg, si, &ser.name);
                let mut line = Line::new(label, PlotPoints::from(pts));
                if let Some(color) = series_color_override(cfg, si) {
                    line = line.color(color);
                }
                plot_ui.line(line);
            }
        }
        ChartData::Scatter { series, .. } => {
            for (si, ser) in series.iter().enumerate() {
                if ser.points.is_empty() {
                    continue;
                }
                let pts: Vec<[f64; 2]> = if log_y {
                    log_transform_points(&ser.points)
                } else {
                    ser.points.clone()
                };
                if pts.is_empty() {
                    continue;
                }
                let label = series_display_name(cfg, si, &ser.name);
                let mut pts_widget = Points::new(label, PlotPoints::from(pts))
                    .radius(2.5)
                    .shape(MarkerShape::Circle);
                if let Some(color) = series_color_override(cfg, si) {
                    pts_widget = pts_widget.color(color);
                }
                plot_ui.points(pts_widget);
            }
        }
        ChartData::Boxes(summaries) => {
            for (i, s) in summaries.iter().enumerate() {
                // For log-scale, transform all five summary values. If any
                // are non-positive we silently skip the box rather than
                // painting a degenerate -inf summary.
                let spread = if log_y {
                    let parts =
                        [s.lower_whisker, s.q1, s.median, s.q3, s.upper_whisker].map(log10_safe);
                    if parts.iter().any(|v| !v.is_finite()) {
                        continue;
                    }
                    BoxSpread::new(parts[0], parts[1], parts[2], parts[3], parts[4])
                } else {
                    BoxSpread::new(s.lower_whisker, s.q1, s.median, s.q3, s.upper_whisker)
                };
                let elem = BoxElem::new(i as f64, spread)
                    .name(&s.name)
                    .box_width(0.6)
                    .whisker_width(0.3);
                let label = series_display_name(cfg, i, &s.name);
                let mut plot = BoxPlot::new(label, vec![elem]);
                if let Some(color) = series_color_override(cfg, i) {
                    plot = plot.color(color);
                }
                plot_ui.box_plot(plot);
            }
        }
    }
}

/// Insert thousands separators without pulling in a formatting crate.
fn fmt_count(n: usize) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let first_chunk = if bytes.len().is_multiple_of(3) {
        3
    } else {
        bytes.len() % 3
    };
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (i - first_chunk).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}
