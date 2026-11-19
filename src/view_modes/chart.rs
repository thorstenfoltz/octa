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

use crate::app::chart_server::ChartShow;
use crate::app::state::TabState;
use crate::ui::theme::{ThemeColors, ThemeMode};
use octa::data::chart::{
    Aggregation, ChartData, ChartKind, ChartLimits, LegendPosition, MAX_HIST_BINS, SeriesStyle,
    XAxisKind, build_chart, calendar_cell, default_x_range, format_clock_tick, format_days_as_date,
    format_seconds_as_datetime, has_numeric_column, heat_rgb, is_chart_numeric, text_on,
};
use octa::data::chart::{ChartConfig, ChartSeries};
use octa::data::chart_export::{self, ExportOptions};
use octa::data::forecast::{Band, TrendKind, forecast, forecast_table, overlays};
use octa::data::seasonality::{decompose, decomposition_table};
use octa::db::pushdown::chart::ChartKey;
use octa::i18n::t;
use octa::ui::control_row::{control_row, control_text_edit};

/// A Pareto chart labels its share points up to this many bars; beyond it
/// the labels would run into each other.
const PARETO_LABELS: usize = 30;

/// A table the chart view asks to be opened in a new tab.
pub struct ChartTab {
    pub label: String,
    pub table: octa::data::DataTable,
    /// Shown as the tab's hint.
    pub hint: String,
    /// Also open a chart tab with this label beside it: a line chart of
    /// every column over the first.
    pub chart: Option<String>,
}

/// Public entry point. Driven by `central_panel::render_central_panel` when
/// the active tab's `view_mode == ViewMode::Chart` (or the tab is a chart tab).
/// Returns the tables **Forecast to table** and **Seasonality to table**
/// asked for, for the caller to open.
pub fn render_chart_view(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    theme_mode: ThemeMode,
    limits: ChartLimits,
) -> Vec<ChartTab> {
    if tab.table.col_count() == 0 {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new(t("chart.empty_no_columns")).weak());
        });
        return Vec::new();
    }
    if !has_numeric_column(&tab.table) {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new(t("chart.no_numeric")).weak());
        });
        return Vec::new();
    }
    seed_defaults(tab);

    let colors = ThemeColors::for_mode(theme_mode);
    draw_controls(ui, tab, &colors);

    ui.separator();

    let cfg = tab.chart_config.clone();
    // A chart from a database tab draws what the database answered.
    let show = tab
        .chart_server
        .as_ref()
        .map(|cs| cs.show(&ChartKey::of(&cfg, limits)));
    draw_server_status(ui, tab, show.as_ref());
    let mut prep = match show {
        Some(ChartShow::Drawn(prep)) => prep,
        Some(ChartShow::Waiting) => return Vec::new(),
        Some(ChartShow::Local | ChartShow::NotExpressible) | None => {
            let filtered: Vec<usize> = tab.filtered_rows.clone();
            build_chart(&tab.table, &filtered, &cfg, limits)
        }
    };

    // Trend and forecast: Line charts over dates or numbers only. The
    // overlays are appended to the chart's own series, so the legend and
    // every export carry them with no further change.
    let mut new_tabs = Vec::new();
    let mut bands: Vec<Band> = Vec::new();
    if cfg.kind == ChartKind::Line {
        let base = match &prep {
            Ok(p) => match &p.data {
                ChartData::Lines {
                    categories: None,
                    series,
                } => Some((series.clone(), p.x_axis_kind)),
                _ => None,
            },
            Err(_) => None,
        };
        let mut overlay_error = None;
        if let Some((series, kind)) = &base
            && (cfg.trend != TrendKind::None || cfg.forecast_periods > 0)
        {
            let key = overlay_key(&cfg, series, *kind);
            if tab
                .chart_overlay_cache
                .as_ref()
                .is_none_or(|(k, _)| *k != key)
            {
                let computed = overlays(
                    series,
                    *kind,
                    cfg.trend,
                    cfg.forecast_periods,
                    cfg.forecast_season,
                );
                tab.chart_overlay_cache = Some((key, computed));
            }
            match tab.chart_overlay_cache.as_ref().map(|(_, r)| r) {
                Some(Ok(o)) => {
                    if let Ok(p) = prep.as_mut()
                        && let ChartData::Lines { series, .. } = &mut p.data
                    {
                        series.extend(o.series.iter().cloned());
                    }
                    bands = o.bands.clone();
                }
                Some(Err(e)) => overlay_error = Some(t(e.i18n_key())),
                None => {}
            }
        }
        let clicked = forecast_controls(ui, tab, overlay_error.as_deref(), base.is_some());
        if clicked.forecast
            && let Some((series, kind)) = &base
        {
            let periods = tab.chart_config.forecast_periods;
            let season = tab.chart_config.forecast_season;
            for s in series {
                if let Ok(f) = forecast(&s.points, *kind, periods, season) {
                    new_tabs.push(ChartTab {
                        label: format!("{} - {}", s.name, t("chart.forecast_tab")),
                        table: forecast_table(&f, *kind),
                        hint: t("chart.forecast_to_table_hint"),
                        chart: None,
                    });
                }
            }
        }
        if clicked.seasonality
            && let Some((series, kind)) = &base
        {
            // The trend and forecast lines are overlays, not data: split the
            // chart's own series only.
            tab.chart_buffers.seasonality_error = None;
            for s in series {
                match decompose(&s.points, *kind, tab.chart_config.forecast_season) {
                    Ok(d) => new_tabs.push(ChartTab {
                        label: format!("{} - {}", s.name, t("chart.seasonality_tab")),
                        hint: t("seasonality.summary")
                            .replace("{season}", &d.season.to_string())
                            .replace("{percent}", &format!("{:.0}", d.strength * 100.0)),
                        table: decomposition_table(&d, *kind),
                        chart: Some(format!("{} - {}", s.name, t("chart.seasonality_chart_tab"))),
                    }),
                    Err(e) => tab.chart_buffers.seasonality_error = Some(e.message()),
                }
            }
        }
        if let Some(err) = &overlay_error {
            ui.colored_label(colors.warning, err);
        }
        if let Some(err) = &tab.chart_buffers.seasonality_error {
            octa::ui::message::selectable_message(ui, colors.warning, err);
        }
    }

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
                            "{} {} / {} {}",
                            t("chart.sampled"),
                            fmt_count(prep.used_rows),
                            fmt_count(prep.total_rows),
                            t("status_bar.rows")
                        ))
                        .small()
                        .color(colors.warning),
                    )
                    .on_hover_text(t("chart.sampled_hint"));
                } else {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {}",
                            fmt_count(prep.total_rows),
                            t("status_bar.rows")
                        ))
                        .small()
                        .weak(),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    draw_export_buttons(ui, &prep, &cfg);
                });
            });

            match &prep.data {
                ChartData::Heatmap { .. } => {
                    draw_heatmap(ui, &prep);
                    return new_tabs;
                }
                ChartData::Calendar { days } => {
                    draw_calendar(ui, days, &prep.y_label);
                    return new_tabs;
                }
                _ => {}
            }

            let x_axis_label = pick_label(&cfg.x_label_override, &prep.x_label);
            let y_axis_label_base = pick_label(&cfg.y_label_override, &prep.y_label);
            let y_axis_label = if cfg.y_log_scale {
                format!("{y_axis_label_base} (log10)")
            } else {
                y_axis_label_base
            };
            let plot_id = format!("chart_plot_{:?}", cfg.kind);

            // X-axis tick formatter selection:
            //  - Categorical -> look up the category name at the integer
            //    tick position.
            //  - Date -> days since 1970-01-01 -> YYYY-MM-DD.
            //  - DateTime -> seconds since the Unix epoch -> YYYY-MM-DD HH:MM:SS.
            //  - Numeric -> leave egui_plot's default formatter alone.
            let categories = prep.data.x_axis_categories().unwrap_or_default();
            let x_axis_kind = prep.x_axis_kind;
            let mut plot = Plot::new(&plot_id)
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
                    XAxisKind::Time | XAxisKind::Duration => {
                        plot = plot.x_axis_formatter(move |mark, _range| {
                            format_clock_tick(x_axis_kind, mark.value, mark.step_size)
                        });
                        // The whole day; bounds the user set below win.
                        if let Some((lo, hi)) = default_x_range(x_axis_kind) {
                            plot = plot.default_x_bounds(lo, hi);
                        }
                    }
                    XAxisKind::Numeric => {}
                }
            }
            // X-axis bounds: same half-set semantics as the Y axis. For
            // categorical Bar / Box charts the user-facing values are
            // category indices (0, 1, 2, ...), so the bound numbers there
            // are just visible-range slices - still useful when you want
            // to zoom into a slice of bars.
            if let (Some(x_min), Some(x_max)) = (cfg.x_min, cfg.x_max)
                && x_min < x_max
                && x_min.is_finite()
                && x_max.is_finite()
            {
                plot = plot.default_x_bounds(x_min, x_max);
            }
            // X grid spacer: emit ticks every `step` units in the visible
            // range. Honoured by all chart kinds.
            if let Some(step) = cfg.x_step
                && step > 0.0
            {
                plot = plot.x_grid_spacer(move |input| {
                    let mut marks = Vec::new();
                    let mut v = (input.bounds.0 / step).ceil() * step;
                    let mut emitted = 0i64;
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
            // Y-axis bounds: when both min and max are set we force them as
            // the default bounds. Half-set is ignored - partial bounds make
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
            // egui_plot reads the default bounds only while its saved view is
            // fresh, so an edited min / max was ignored after the first frame.
            // Reset the saved view whenever the requested bounds change.
            let bounds_key =
                [cfg.x_min, cfg.x_max, cfg.y_min, cfg.y_max].map(|v| v.map(f64::to_bits));
            let bounds_key = (bounds_key, cfg.y_log_scale);
            // Keyed by this ui, as the plot itself is: two tiled charts each
            // remember their own bounds.
            let bounds_mem_id = ui.make_persistent_id(("chart_plot_bounds", &plot_id));
            let bounds_changed = ui.data_mut(|d| {
                let prev = d.get_temp::<([Option<u64>; 4], bool)>(bounds_mem_id);
                d.insert_temp(bounds_mem_id, bounds_key);
                prev.is_some_and(|p| p != bounds_key)
            });
            if bounds_changed {
                plot = plot.reset();
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
            // Y axis tick formatter - integer rounding when `y_integer_only`,
            // or 10^N notation when log-scaled (the formatter takes log10
            // values and converts back to the original magnitude).
            let log = cfg.y_log_scale;
            let integer_only = cfg.y_integer_only;
            let y_kind = prep.y_axis_kind;
            if !log && matches!(y_kind, XAxisKind::Time | XAxisKind::Duration) {
                plot = plot.y_axis_formatter(move |mark, _range| {
                    format_clock_tick(y_kind, mark.value, mark.step_size)
                });
            } else if log || integer_only {
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
            let band_fill = colors.accent;
            plot.show(ui, |plot_ui| {
                // Log scale has no place for a range that dips below zero,
                // so the bands are left out there (the 95% lines stay).
                if !cfg.y_log_scale {
                    for band in &bands {
                        let mut ring = band.lo.clone();
                        ring.extend(band.hi.iter().rev());
                        let alpha = if band.level == 95 { 30.0 } else { 55.0 };
                        plot_ui.polygon(
                            egui_plot::Polygon::new(
                                format!("{}%", band.level),
                                PlotPoints::from(ring),
                            )
                            .fill_color(band_fill.gamma_multiply(alpha / 255.0))
                            .stroke(egui::Stroke::NONE),
                        );
                    }
                }
                draw_plot_items(plot_ui, &prep.data, &cfg);
                // A Pareto chart's running share, drawn over the bars on a
                // scale where 100 % is the tallest bar. Skipped under a log
                // scale, like the forecast bands: a share is linear.
                if cfg.kind == ChartKind::Pareto
                    && !cfg.y_log_scale
                    && let Some(share) = octa::data::chart::pareto_share(&prep.data)
                    && let ChartData::Bars { series, .. } = &prep.data
                {
                    let top = series
                        .first()
                        .map(|s| s.points.iter().map(|p| p[1]).fold(0.0_f64, f64::max))
                        .unwrap_or(0.0);
                    let pts: Vec<[f64; 2]> = share
                        .iter()
                        .enumerate()
                        .map(|(i, s)| [i as f64, s / 100.0 * top])
                        .collect();
                    // ponytail: no percent axis on the right, egui_plot draws
                    // one Y scale; the labels at the points carry the number.
                    // A second axis via custom_y_axes if the labels crowd.
                    if pts.len() <= PARETO_LABELS {
                        for (p, s) in pts.iter().zip(&share) {
                            let at = egui_plot::PlotPoint::new(p[0], p[1]);
                            let label = egui_plot::Text::new("", at, format!("{s:.0}%"));
                            plot_ui.text(label.anchor(egui::Align2::CENTER_BOTTOM));
                        }
                    }
                    plot_ui.line(
                        Line::new(t("chart.pareto_share"), PlotPoints::from(pts))
                            .color(colors.error),
                    );
                }
            });
        }
    }
    new_tabs
}

/// What the overlays depend on, for `TabState::chart_overlay_cache`.
fn overlay_key(cfg: &ChartConfig, series: &[ChartSeries], kind: XAxisKind) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (
        cfg.trend as u8,
        cfg.forecast_periods,
        cfg.forecast_season,
        kind as u8,
    )
        .hash(&mut h);
    for s in series {
        s.name.hash(&mut h);
        for p in &s.points {
            p[0].to_bits().hash(&mut h);
            p[1].to_bits().hash(&mut h);
        }
    }
    h.finish()
}

fn trend_label(k: TrendKind) -> (String, String) {
    let key = match k {
        TrendKind::None => "chart.trend_none",
        TrendKind::Straight => "chart.trend_straight",
        TrendKind::MovingAverage => "chart.trend_moving",
    };
    (t(key), t(&format!("{key}_hint")))
}

/// Which of the Trend / Forecast row's buttons was clicked.
#[derive(Default)]
struct RowClicks {
    forecast: bool,
    seasonality: bool,
}

/// The Line chart's Trend / Forecast row. `has_series` is whether the chart
/// draws lines over dates or numbers, which a seasonality split needs.
fn forecast_controls(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    overlay_error: Option<&str>,
    has_series: bool,
) -> RowClicks {
    let mut clicks = RowClicks::default();
    control_row(ui, |ui| {
        ui.label(t("chart.trend"))
            .on_hover_text(t("chart.trend_hint"));
        let (current, current_hint) = trend_label(tab.chart_config.trend);
        egui::ComboBox::from_id_salt("chart_trend_combo")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for k in [
                    TrendKind::None,
                    TrendKind::Straight,
                    TrendKind::MovingAverage,
                ] {
                    let (label, hint) = trend_label(k);
                    ui.selectable_value(&mut tab.chart_config.trend, k, label)
                        .on_hover_text(hint);
                }
            })
            .response
            .on_hover_text(current_hint);

        ui.separator();
        ui.label(t("chart.forecast"))
            .on_hover_text(t("chart.forecast_hint"));
        let buf = &mut tab.chart_buffers.forecast_periods;
        if buf.is_empty() && tab.chart_config.forecast_periods > 0 {
            *buf = tab.chart_config.forecast_periods.to_string();
        }
        let response = control_text_edit(
            ui,
            60.0,
            egui::TextEdit::singleline(buf)
                .id_salt("chart_forecast_periods")
                .hint_text("0"),
        )
        .on_hover_text(t("chart.forecast_hint"));
        if response.changed() {
            let trimmed = buf.trim();
            if trimmed.is_empty() {
                tab.chart_config.forecast_periods = 0;
            } else if let Ok(n) = trimmed.parse::<usize>() {
                tab.chart_config.forecast_periods = n.min(500);
            }
        }

        let can_table = tab.chart_config.forecast_periods > 0 && overlay_error.is_none();
        if ui
            .add_enabled(can_table, egui::Button::new(t("chart.forecast_to_table")))
            .on_hover_text(t("chart.forecast_to_table_hint"))
            .on_disabled_hover_text(
                overlay_error.map_or_else(|| t("chart.forecast_to_table_disabled"), str::to_string),
            )
            .clicked()
        {
            clicks.forecast = true;
        }
        if ui
            .add_enabled(
                has_series,
                egui::Button::new(t("chart.seasonality_to_table")),
            )
            .on_hover_text(t("chart.seasonality_to_table_hint"))
            .on_disabled_hover_text(t("chart.seasonality_to_table_disabled"))
            .clicked()
        {
            clicks.seasonality = true;
        }
    });
    egui::CollapsingHeader::new(t("chart.forecast_advanced"))
        .id_salt("chart_forecast_advanced")
        .default_open(false)
        .show(ui, |ui| {
            control_row(ui, |ui| {
                let mut auto = tab.chart_config.forecast_season.is_none();
                if ui
                    .checkbox(&mut auto, t("chart.season_auto"))
                    .on_hover_text(t("chart.season_auto_hint"))
                    .changed()
                {
                    tab.chart_config.forecast_season = if auto { None } else { Some(12) };
                    tab.chart_buffers.forecast_season = tab
                        .chart_config
                        .forecast_season
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                }
                ui.label(t("chart.season_len"))
                    .on_hover_text(t("chart.season_len_hint"));
                let buf = &mut tab.chart_buffers.forecast_season;
                let response = ui
                    .add_enabled_ui(!auto, |ui| {
                        control_text_edit(
                            ui,
                            60.0,
                            egui::TextEdit::singleline(buf).id_salt("chart_forecast_season"),
                        )
                    })
                    .inner
                    .on_hover_text(t("chart.season_len_hint"))
                    .on_disabled_hover_text(t("chart.season_auto_hint"));
                if response.changed()
                    && let Ok(n) = buf.trim().parse::<usize>()
                {
                    tab.chart_config.forecast_season = Some(n.clamp(2, 1000));
                }
            });
        });
    clicks
}

/// A database chart's state above the plot: fetching, refused (Try again /
/// Use the loaded rows), drawn from the copied rows by choice (Try again),
/// or drawn from them because the engine cannot (the reason).
fn draw_server_status(ui: &mut egui::Ui, tab: &mut TabState, show: Option<&ChartShow>) {
    let Some(cs) = tab.chart_server.as_mut() else {
        return;
    };
    let loaded = fmt_count(cs.loaded);
    if matches!(show, Some(ChartShow::Waiting))
        && !cs.local
        && cs.pending.is_none()
        && cs.error.is_none()
        && cs.shown.is_none()
    {
        // The first sync ran before the defaults were seeded: look again.
        // Waiting means a complete key, so the next sync sends the query and
        // pending turns this off; an incomplete key never gets here.
        ui.ctx().request_repaint();
    }
    if cs.local {
        control_row(ui, |ui| {
            octa::ui::message::partial_note_label(
                ui,
                &t("pushdown.chart_local").replace("{loaded}", &loaded),
            );
            if ui
                .button(t("dbview.retry"))
                .on_hover_text(t("pushdown.chart_retry_hint"))
                .clicked()
            {
                cs.local = false;
            }
        });
        return;
    }
    if let Some((_, e)) = cs.error.clone() {
        let colour = ui.visuals().error_fg_color;
        octa::ui::message::selectable_message(ui, colour, &e);
        control_row(ui, |ui| {
            if ui
                .button(t("dbview.retry"))
                .on_hover_text(t("pushdown.chart_retry_hint"))
                .clicked()
            {
                cs.error = None;
            }
            if ui
                .button(t("pushdown.use_loaded"))
                .on_hover_text(t("pushdown.chart_use_loaded_hint").replace("{loaded}", &loaded))
                .clicked()
            {
                cs.error = None;
                cs.local = true;
            }
        });
    } else if cs.pending.is_some() {
        control_row(ui, |ui| {
            ui.spinner();
            ui.weak(t("pushdown.chart_updating"));
        });
    }
    if matches!(show, Some(ChartShow::NotExpressible)) {
        let reason = t("pushdown.chart_box_local")
            .replace("{loaded}", &loaded)
            .replace("{engine}", cs.src.engine().label());
        ui.horizontal_wrapped(|ui| octa::ui::message::partial_note_label(ui, &reason));
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
    if tab.chart_config.kind == ChartKind::Heatmap && tab.chart_config.row_col.is_none() {
        tab.chart_config.row_col = tab.table.columns.iter().enumerate().position(|(i, c)| {
            Some(i) != tab.chart_config.x_col && octa::data::shapes::is_text_type(&c.data_type)
        });
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
        .position(|c| is_chart_numeric(&c.data_type))
}

fn first_numeric_col_excluding(tab: &TabState, excluded: Option<usize>) -> Option<usize> {
    tab.table
        .columns
        .iter()
        .enumerate()
        .find_map(|(i, c)| (Some(i) != excluded && is_chart_numeric(&c.data_type)).then_some(i))
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
/// `positive_only` filters values `<= 0` - used for the Y step where zero /
/// negative either no-ops or upsets the grid spacer.
fn optional_f64_input(
    ui: &mut egui::Ui,
    id_salt: &str,
    buffer: &mut String,
    value: &mut Option<f64>,
    positive_only: bool,
) {
    let response = control_text_edit(
        ui,
        80.0,
        egui::TextEdit::singleline(buffer)
            .id_salt(id_salt)
            .hint_text(t("chart.auto_placeholder")),
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
            // Leave value unchanged - the user's typing transient bytes
            // ("1.2e", "-", etc.) that aren't yet parseable. They'll
            // finish typing and the next changed event lands.
        }
    }
}

fn draw_controls(ui: &mut egui::Ui, tab: &mut TabState, colors: &ThemeColors) {
    // Row 1: kind + X/Y pickers + agg / bin picker.
    control_row(ui, |ui| {
        ui.label(
            egui::RichText::new(t("chart.chart_label"))
                .color(colors.text_primary)
                .strong(),
        );
        let kind_before = tab.chart_config.kind;
        egui::ComboBox::from_id_salt("chart_kind_combo")
            .selected_text(tab.chart_config.kind.display_name())
            .show_ui(ui, |ui| {
                for &k in ChartKind::ALL {
                    let r = ui.selectable_value(&mut tab.chart_config.kind, k, k.display_name());
                    if k == ChartKind::Pareto {
                        r.on_hover_text(t("chart.pareto_hint"));
                    }
                }
            })
            .response
            .on_hover_text(t("chart.kind_hint"));
        if kind_before != tab.chart_config.kind && !tab.chart_config.kind.needs_y() {
            tab.chart_config.y_cols.clear();
        }

        ui.separator();
        ui.label(t("chart.x"));
        let column_names: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();
        col_picker(
            ui,
            "chart_x_combo",
            &mut tab.chart_config.x_col,
            &column_names,
        );

        if tab.chart_config.kind.needs_y() {
            ui.separator();
            ui.label(t("chart.y"));
            y_picker(ui, &mut tab.chart_config.y_cols, &column_names);
        }

        if tab.chart_config.kind == ChartKind::Heatmap {
            ui.separator();
            ui.label(t("chart.rows"))
                .on_hover_text(t("chart.rows_hint"));
            col_picker(
                ui,
                "chart_rows_combo",
                &mut tab.chart_config.row_col,
                &column_names,
            )
            .on_hover_text(t("chart.rows_hint"));
        }

        match tab.chart_config.kind {
            ChartKind::Bar | ChartKind::Pareto | ChartKind::Heatmap | ChartKind::Calendar => {
                ui.separator();
                ui.label(t("chart.agg"));
                egui::ComboBox::from_id_salt("chart_agg_combo")
                    .selected_text(tab.chart_config.agg.display_name())
                    .show_ui(ui, |ui| {
                        for &a in Aggregation::ALL {
                            ui.selectable_value(&mut tab.chart_config.agg, a, a.display_name());
                        }
                    });
            }
            ChartKind::Histogram => {
                ui.separator();
                ui.label(t("dialog.vf_bins"))
                    .on_hover_text(t("chart.bins_hint"));
                let mut auto = tab.chart_config.hist_bins.is_none();
                if ui.checkbox(&mut auto, t("chart.auto_sturges")).changed() {
                    if auto {
                        tab.chart_config.hist_bins = None;
                        tab.chart_buffers.hist_bins.clear();
                    } else {
                        tab.chart_config.hist_bins = Some(20);
                        tab.chart_buffers.hist_bins = "20".to_string();
                    }
                }
                if !auto {
                    let response = control_text_edit(
                        ui,
                        80.0,
                        egui::TextEdit::singleline(&mut tab.chart_buffers.hist_bins)
                            .id_salt("chart_hist_bins")
                            .hint_text("20"),
                    );
                    if response.changed() {
                        let trimmed = tab.chart_buffers.hist_bins.trim();
                        if let Ok(n) = trimmed.parse::<usize>() {
                            tab.chart_config.hist_bins = Some(n.clamp(1, MAX_HIST_BINS));
                        }
                        // Mid-typing transients (empty / "0" / non-digits)
                        // leave hist_bins unchanged - the user'll finish
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
    egui::CollapsingHeader::new(t("chart.customise"))
        .id_salt("chart_customize_collapsible")
        .default_open(false)
        .show(ui, |ui| {
            // Group A - Labels + legend + grid toggle. One row.
            control_row(ui, |ui| {
                ui.label(t("chart.title"));
                control_text_edit(
                    ui,
                    160.0,
                    egui::TextEdit::singleline(&mut tab.chart_config.title)
                        .hint_text(t("chart.placeholder_none")),
                );
                ui.separator();
                ui.label(t("chart.x_label"));
                control_text_edit(
                    ui,
                    120.0,
                    egui::TextEdit::singleline(&mut tab.chart_config.x_label_override)
                        .hint_text(t("chart.placeholder_auto")),
                );
                ui.separator();
                ui.label(t("chart.y_label"));
                control_text_edit(
                    ui,
                    120.0,
                    egui::TextEdit::singleline(&mut tab.chart_config.y_label_override)
                        .hint_text(t("chart.placeholder_auto")),
                );
                ui.separator();
                ui.label(t("chart.legend"));
                egui::ComboBox::from_id_salt("chart_legend_combo")
                    .selected_text(tab.chart_config.legend.display_name())
                    .show_ui(ui, |ui| {
                        for &p in LegendPosition::ALL {
                            ui.selectable_value(&mut tab.chart_config.legend, p, p.display_name());
                        }
                    });
                ui.separator();
                ui.checkbox(&mut tab.chart_config.show_grid, t("chart.show_grid"));
            });

            // Group B - X axis. One row, mirrors the Y axis controls below
            // so users can clamp either dimension. For categorical Bar / Box
            // charts the bounds are interpreted as category indices.
            ui.add_space(2.0);
            control_row(ui, |ui| {
                ui.label(egui::RichText::new(t("chart.x_axis")).strong());
                ui.label(t("chart.min"))
                    .on_hover_text(t("chart.x_min_hint"));
                optional_f64_input(
                    ui,
                    "chart_x_min",
                    &mut tab.chart_buffers.x_min,
                    &mut tab.chart_config.x_min,
                    false,
                );
                ui.label(t("chart.max"));
                optional_f64_input(
                    ui,
                    "chart_x_max",
                    &mut tab.chart_buffers.x_max,
                    &mut tab.chart_config.x_max,
                    false,
                );
                ui.label(t("chart.step"))
                    .on_hover_text(t("chart.x_step_hint"));
                optional_f64_input(
                    ui,
                    "chart_x_step",
                    &mut tab.chart_buffers.x_step,
                    &mut tab.chart_config.x_step,
                    true,
                );
            });

            // Group C - Y axis. One row.
            ui.add_space(2.0);
            control_row(ui, |ui| {
                ui.label(egui::RichText::new(t("chart.y_axis")).strong());
                ui.label(t("chart.min"))
                    .on_hover_text(t("chart.y_min_hint"));
                optional_f64_input(
                    ui,
                    "chart_y_min",
                    &mut tab.chart_buffers.y_min,
                    &mut tab.chart_config.y_min,
                    false,
                );
                ui.label(t("chart.max"));
                optional_f64_input(
                    ui,
                    "chart_y_max",
                    &mut tab.chart_buffers.y_max,
                    &mut tab.chart_config.y_max,
                    false,
                );
                ui.label(t("chart.step"))
                    .on_hover_text(t("chart.y_step_hint"));
                optional_f64_input(
                    ui,
                    "chart_y_step",
                    &mut tab.chart_buffers.y_step,
                    &mut tab.chart_config.y_step,
                    true,
                );
                ui.separator();
                ui.checkbox(
                    &mut tab.chart_config.y_integer_only,
                    t("chart.integers_only"),
                )
                .on_hover_text(t("chart.integers_hint"));
                ui.checkbox(&mut tab.chart_config.y_log_scale, t("chart.log_scale"))
                    .on_hover_text(t("chart.log_hint"));
            });
            // Group D - Series. Each Y-column gets a single horizontal
            // row (column name -> label override -> color picker). Wrapped
            // so multi-Y charts still fit a narrow window.
            if tab.chart_config.kind.needs_y() && !tab.chart_config.y_cols.is_empty() {
                ui.add_space(2.0);
                control_row(ui, |ui| {
                    ui.label(egui::RichText::new(t("chart.series")).strong());
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
                        control_text_edit(
                            ui,
                            120.0,
                            egui::TextEdit::singleline(&mut style.display_name)
                                .hint_text(&col_name),
                        );
                        let mut on = style.color.is_some();
                        if ui
                            .checkbox(&mut on, "")
                            .on_hover_text(t("chart.custom_color"))
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
    // Build options once - the three buttons all reuse the same SVG.
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
    )
    .with_pareto(cfg.kind == ChartKind::Pareto);

    // The actual write happens on a click; the SVG/PNG/PDF byte buffer is
    // built lazily so a chart with thousands of points isn't re-encoded
    // every frame.
    if ui.button(t("chart.export_pdf")).clicked() {
        save_export(
            "pdf",
            &format!("{} (PDF)", t("chart.export_dialog")),
            &["pdf"],
            || {
                let svg = chart_export::to_svg(prep, &opts);
                chart_export::to_pdf(&svg)
            },
        );
    }
    if ui.button(t("chart.export_png")).clicked() {
        save_export(
            "png",
            &format!("{} (PNG)", t("chart.export_dialog")),
            &["png"],
            || {
                let svg = chart_export::to_svg(prep, &opts);
                chart_export::to_png(&svg, 2.0)
            },
        );
    }
    if ui.button(t("chart.export_svg")).clicked() {
        save_export(
            "svg",
            &format!("{} (SVG)", t("chart.export_dialog")),
            &["svg"],
            || Ok::<Vec<u8>, String>(chart_export::to_svg(prep, &opts).into_bytes()),
        );
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
) -> egui::Response {
    let current_label = selected
        .and_then(|i| column_names.get(i).cloned())
        .unwrap_or_else(|| t("chart.pick"));
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(current_label)
        .show_ui(ui, |ui| {
            for (i, name) in column_names.iter().enumerate() {
                ui.selectable_value(selected, Some(i), name);
            }
        })
        .response
}

fn y_picker(ui: &mut egui::Ui, y_cols: &mut Vec<usize>, column_names: &[String]) {
    let label = match y_cols.len() {
        0 => t("chart.pick"),
        1 => column_names.get(y_cols[0]).cloned().unwrap_or_default(),
        n => format!("{n} {}", t("status_bar.columns")),
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
            let mut chart = BarChart::new(t("status_bar.count"), bars);
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
                    .radius(2.5_f32)
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
        ChartData::Heatmap { .. } | ChartData::Calendar { .. } => {}
    }
}

fn heat_color(rgb: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// Whole numbers without decimals, else two.
fn heat_text(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
    }
}

/// Heatmap: one coloured cell per (Rows, X) pair, labels in two gutters.
fn draw_heatmap(ui: &mut egui::Ui, prep: &octa::data::chart::ChartPrep) {
    let ChartData::Heatmap {
        x_categories: xs,
        y_categories: ys,
        values,
        counts,
    } = &prep.data
    else {
        return;
    };
    let (nx, ny) = (xs.len(), ys.len());
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
    let left = 160.0_f32.min(rect.width() * 0.25);
    let grid = egui::Rect::from_min_max(
        rect.min + egui::vec2(left, 0.0),
        rect.max - egui::vec2(0.0, 22.0),
    );
    if nx == 0 || ny == 0 || grid.width() < 1.0 || grid.height() < 1.0 {
        return;
    }
    let (cw, ch) = (grid.width() / nx as f32, grid.height() / ny as f32);
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in values.iter().flatten().flatten() {
        min = min.min(*v);
        max = max.max(*v);
    }
    let painter = ui.painter_at(rect);
    let font = egui::FontId::proportional(11.0);
    let text_color = ui.visuals().text_color();
    for (yi, row) in values.iter().enumerate() {
        for (xi, cell) in row.iter().enumerate() {
            let Some(v) = cell else { continue };
            let r = egui::Rect::from_min_size(
                grid.min + egui::vec2(xi as f32 * cw, yi as f32 * ch),
                egui::vec2(cw, ch),
            );
            let fill = heat_rgb(*v, min, max);
            painter.rect_filled(r.shrink(0.5), 0.0, heat_color(fill));
            if cw >= 34.0 && ch >= 16.0 {
                painter.text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    heat_text(*v),
                    font.clone(),
                    heat_color(text_on(fill)),
                );
            }
        }
    }
    let rows_clip = egui::Rect::from_min_max(rect.min, egui::pos2(grid.left(), grid.bottom()));
    let rows_painter = painter.with_clip_rect(rows_clip);
    let row_step = (14.0 / ch).ceil().max(1.0) as usize;
    for (yi, name) in ys.iter().enumerate().step_by(row_step) {
        rows_painter.text(
            egui::pos2(grid.left() - 4.0, grid.top() + (yi as f32 + 0.5) * ch),
            egui::Align2::RIGHT_CENTER,
            name,
            font.clone(),
            text_color,
        );
    }
    let x_clip = egui::Rect::from_min_max(egui::pos2(grid.left(), grid.bottom()), rect.max);
    let x_painter = painter.with_clip_rect(x_clip);
    let x_step = (60.0 / cw).ceil().max(1.0) as usize;
    for (xi, name) in xs.iter().enumerate().step_by(x_step) {
        x_painter.text(
            egui::pos2(grid.left() + (xi as f32 + 0.5) * cw, grid.bottom() + 3.0),
            egui::Align2::CENTER_TOP,
            name,
            font.clone(),
            text_color,
        );
    }
    if let Some(pos) = response.hover_pos().filter(|p| grid.contains(*p)) {
        let xi = (((pos.x - grid.left()) / cw) as usize).min(nx - 1);
        let yi = (((pos.y - grid.top()) / ch) as usize).min(ny - 1);
        let value_text = values[yi][xi].map_or_else(|| "-".to_string(), heat_text);
        response.on_hover_ui_at_pointer(|ui| {
            ui.label(format!(
                "{}: {}\n{}: {}\n{}\n{}",
                prep.x_label,
                xs[xi],
                prep.y_label,
                ys[yi],
                value_text,
                t("chart.hover_rows").replace("{n}", &counts[yi][xi].to_string())
            ));
        });
    }
}

/// Calendar: one band per year, a column per week, a row per weekday.
fn draw_calendar(ui: &mut egui::Ui, days: &[(i64, f64)], value_label: &str) {
    use chrono::Datelike;
    use std::collections::{BTreeSet, HashMap};
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    for (_, v) in days {
        min = min.min(*v);
        max = max.max(*v);
    }
    let years: BTreeSet<i32> = days
        .iter()
        .filter_map(|(d, _)| calendar_cell(*d).map(|c| c.0))
        .collect();
    let weekdays: Vec<String> = t("chart.weekdays").split(',').map(String::from).collect();
    let months: Vec<String> = t("chart.months").split(',').map(String::from).collect();
    let cell = ((ui.available_width() - 40.0) / 54.0).clamp(8.0, 18.0);
    let font = egui::FontId::proportional(11.0);
    let text_color = ui.visuals().text_color();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for &year in years.iter().rev() {
                ui.label(egui::RichText::new(year.to_string()).strong());
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(40.0 + 54.0 * cell, 16.0 + 7.0 * cell),
                    egui::Sense::hover(),
                );
                let painter = ui.painter_at(rect);
                let origin = rect.min + egui::vec2(40.0, 16.0);
                for row in [0, 2, 4] {
                    if let Some(name) = weekdays.get(row) {
                        painter.text(
                            egui::pos2(origin.x - 4.0, origin.y + (row as f32 + 0.5) * cell),
                            egui::Align2::RIGHT_CENTER,
                            name,
                            font.clone(),
                            text_color,
                        );
                    }
                }
                for (m, name) in months.iter().enumerate() {
                    let first = chrono::NaiveDate::from_ymd_opt(year, m as u32 + 1, 1)
                        .map(|d| i64::from(d.num_days_from_ce()) - 719_163);
                    if let Some((_, col, _)) = first.and_then(calendar_cell) {
                        painter.text(
                            egui::pos2(origin.x + col as f32 * cell, rect.top()),
                            egui::Align2::LEFT_TOP,
                            name,
                            font.clone(),
                            text_color,
                        );
                    }
                }
                let mut cells: HashMap<(usize, usize), (i64, f64)> = HashMap::new();
                for &(d, v) in days {
                    if let Some((y, col, row)) = calendar_cell(d)
                        && y == year
                    {
                        cells.insert((col, row), (d, v));
                        let r = egui::Rect::from_min_size(
                            origin + egui::vec2(col as f32 * cell, row as f32 * cell),
                            egui::vec2(cell, cell),
                        );
                        painter.rect_filled(r.shrink(1.0), 2.0, heat_color(heat_rgb(v, min, max)));
                    }
                }
                if let Some(pos) = response
                    .hover_pos()
                    .filter(|p| p.x >= origin.x && p.y >= origin.y)
                {
                    let col = ((pos.x - origin.x) / cell) as usize;
                    let row = ((pos.y - origin.y) / cell) as usize;
                    if let Some(&(d, v)) = cells.get(&(col, row)) {
                        response.on_hover_ui_at_pointer(|ui| {
                            ui.label(format!(
                                "{}\n{}: {}",
                                format_days_as_date(d as f64),
                                value_label,
                                heat_text(v)
                            ));
                        });
                    }
                }
                ui.add_space(6.0);
            }
        });
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
        // A comma before every third digit after the first group. `i` is
        // checked first: below the first group the subtraction would wrap
        // (a panic in debug, `2,4` for 24 in release).
        if i >= first_chunk && (i - first_chunk).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

#[cfg(test)]
#[path = "chart_tests.rs"]
mod tests;
