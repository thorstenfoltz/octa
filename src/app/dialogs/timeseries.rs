//! Time series dialog. Two reshapes over a time column, both dropped into a
//! fresh detached tab (same pattern as Pivot and the Summary tab):
//!
//! - **Time buckets** (resample): group rows into one bucket per minute / hour /
//!   day / week / month / quarter / year and aggregate the value columns.
//! - **Rolling window**: add a column holding the aggregate of the last N rows,
//!   in a chosen order, e.g. a 7-day moving average.
//!
//! The SQL comes from the pure `octa::data::timeseries` builders, which the CLI
//! and the MCP tools use too, so all three surfaces emit identical queries.

use eframe::egui;
use egui::RichText;

use octa::data::timeseries::{
    Interval, ResampleSpec, RollingSpec, TimeAgg, build_resample_sql, build_rolling_sql,
    explain_resample, explain_rolling,
};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::widgets::{
    PREVIEW_RESULT_ROWS, PREVIEW_SOURCE_ROWS, col_combo, col_combo_ordered, multi_col_picker,
    render_preview_grid,
};
use crate::app::pushdown::{PushdownKind, ReshapeJob, ReshapeSpec, SourceKey, TaskPoll};
use crate::app::state::{OctaApp, RollingAsk, TimeseriesKind, TimeseriesState};

pub(crate) fn render_timeseries_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.timeseries_dialog.is_none() {
        return;
    }

    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let time_order = time_column_order(&app.tabs[app.active_tab].table);

    let mut close = false;
    let mut run = false;
    // Work on a clone so the closure doesn't borrow `app` twice; written back
    // after the window closes.
    let mut st = app.timeseries_dialog.take().unwrap();

    // Refresh the bounded preview only when the inputs changed (never per
    // frame, never against the full table).
    let key = preview_key(&st);
    if st.preview_key != key {
        st.preview = compute_preview(app, &st, &col_names);
        st.preview_key = key;
    }

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;
    // Time buckets over a typed time column run on the server for a
    // database tab; the preview does not.
    let on_server = app.active_server_source().is_some();
    // The preview reads at most PREVIEW_SOURCE_ROWS of them.
    let loaded_rows = app.tabs[app.active_tab]
        .table
        .rows
        .len()
        .min(PREVIEW_SOURCE_ROWS);
    let col_types: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.data_type.clone())
        .collect();

    let dialog_id = egui::Id::new("octa_timeseries_dialog");
    let window = egui::Window::new("octa_timeseries")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(560.0)
            .default_height(560.0)
            .min_width(360.0)
            .min_height(200.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("timeseries_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("timeseries.title"))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close = true;
                        }
                    });
                });
            });

        if minimized {
            return;
        }

        egui::Panel::bottom("timeseries_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let ready = build_sql(&st, &col_names);
                    // One Rolling question at a time.
                    let asking = st.rolling_ask.is_some();
                    let b = ui.add_enabled(
                        ready.is_ok() && !asking,
                        egui::Button::new(octa::i18n::t("timeseries.apply")),
                    );
                    let b = if asking {
                        b.on_disabled_hover_text(octa::i18n::t("pushdown.rolling_title"))
                    } else {
                        b
                    };
                    if b.clicked() {
                        run = true;
                    }
                    // The reason Create tab is disabled, rather than a generic
                    // "pick some columns": the builders already say exactly
                    // what is missing.
                    if let Err(why) = ready {
                        ui.label(
                            RichText::new(why)
                                .size(10.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("common.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut st.kind,
                    TimeseriesKind::Resample,
                    octa::i18n::t("timeseries.resample"),
                )
                .on_hover_text(octa::i18n::t("timeseries.resample_hint"));
                ui.selectable_value(
                    &mut st.kind,
                    TimeseriesKind::Rolling,
                    octa::i18n::t("timeseries.rolling"),
                )
                .on_hover_text(octa::i18n::t("timeseries.rolling_hint"));
            });
            ui.separator();
            match st.kind {
                TimeseriesKind::Resample => resample_body(ui, &mut st, &col_names, &time_order),
                TimeseriesKind::Rolling => rolling_body(ui, &mut st, &col_names, &time_order),
            }

            // Plain-language explanation + bounded live preview, so the user
            // can see what the reshape does without already knowing the concept.
            ui.add_space(6.0);
            ui.separator();
            ui.label(RichText::new(explain_text(&st, &col_names)).italics());
            ui.add_space(4.0);
            match &st.preview {
                Some(Ok(table)) => {
                    ui.label(
                        RichText::new(octa::i18n::t("timeseries.preview"))
                            .strong()
                            .size(11.0),
                    );
                    let typed = st
                        .time_col
                        .and_then(|i| col_types.get(i))
                        .is_some_and(|t| octa::db::pushdown::dialect::is_time_type(t));
                    if on_server && st.kind == TimeseriesKind::Resample && typed {
                        ui.weak(octa::i18n::t("pushdown.preview_loaded").replace(
                            "{loaded}",
                            &octa::ui::status_bar::format_number(loaded_rows),
                        ));
                    }
                    render_preview_grid(ui, "ts", table);
                }
                Some(Err(e)) => {
                    ui.label(
                        RichText::new(e)
                            .color(ui.visuals().error_fg_color)
                            .size(10.0),
                    );
                }
                None => {
                    ui.label(
                        RichText::new(octa::i18n::t("timeseries.preview_none"))
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                }
            }
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if run && execute_timeseries(app, ctx, &mut st, &col_names) {
        return; // dialog dropped (st not written back)
    }
    if st.rolling_ask.is_some() && render_rolling_ask(app, ctx, &mut st) {
        return; // answered: the job started, or the loaded rows ran
    }
    if !close {
        app.timeseries_dialog = Some(st);
    }
}

fn resample_body(
    ui: &mut egui::Ui,
    st: &mut TimeseriesState,
    cols: &[String],
    time_order: &[usize],
) {
    egui::Grid::new("timeseries_resample_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(octa::i18n::t("timeseries.time_col"));
            col_combo_ordered(ui, "ts_time", &mut st.time_col, cols, Some(time_order));
            ui.end_row();

            ui.label(octa::i18n::t("timeseries.interval"));
            egui::ComboBox::from_id_salt("ts_interval")
                .selected_text(interval_label(st.interval))
                .show_ui(ui, |ui| {
                    for &i in Interval::ALL {
                        ui.selectable_value(&mut st.interval, i, interval_label(i));
                    }
                });
            ui.end_row();

            ui.label(octa::i18n::t("timeseries.agg"));
            agg_combo(ui, "ts_agg_resample", &mut st.agg);
            ui.end_row();
        });

    ui.add_space(6.0);
    ui.label(RichText::new(octa::i18n::t("timeseries.value_cols")).strong());
    multi_col_picker(ui, "ts_values", &mut st.value_cols, cols);

    ui.add_space(6.0);
    ui.label(RichText::new(octa::i18n::t("timeseries.group_by")).strong());
    multi_col_picker(ui, "ts_group", &mut st.group_cols, cols);
}

fn rolling_body(
    ui: &mut egui::Ui,
    st: &mut TimeseriesState,
    cols: &[String],
    time_order: &[usize],
) {
    egui::Grid::new("timeseries_rolling_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(octa::i18n::t("timeseries.order_col"));
            col_combo_ordered(ui, "ts_order", &mut st.order_col, cols, Some(time_order));
            ui.end_row();

            ui.label(octa::i18n::t("timeseries.value_col"));
            col_combo(ui, "ts_roll_value", &mut st.roll_value_col, cols);
            ui.end_row();

            ui.label(octa::i18n::t("timeseries.window"));
            ui.add(egui::TextEdit::singleline(&mut st.window_text).desired_width(80.0));
            ui.end_row();

            ui.label(octa::i18n::t("timeseries.agg"));
            agg_combo(ui, "ts_agg_rolling", &mut st.agg);
            ui.end_row();
        });

    ui.add_space(6.0);
    ui.label(RichText::new(octa::i18n::t("timeseries.partition_by")).strong());
    multi_col_picker(ui, "ts_partition", &mut st.partition_cols, cols);
}

fn agg_combo(ui: &mut egui::Ui, id: &str, agg: &mut TimeAgg) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(agg_label(*agg))
        .show_ui(ui, |ui| {
            for &a in TimeAgg::ALL {
                ui.selectable_value(agg, a, agg_label(a));
            }
        });
}

fn interval_label(i: Interval) -> String {
    octa::i18n::t(match i {
        Interval::Minute => "timeseries.int_minute",
        Interval::Hour => "timeseries.int_hour",
        Interval::Day => "timeseries.int_day",
        Interval::Week => "timeseries.int_week",
        Interval::Month => "timeseries.int_month",
        Interval::Quarter => "timeseries.int_quarter",
        Interval::Year => "timeseries.int_year",
    })
}

fn agg_label(a: TimeAgg) -> String {
    octa::i18n::t(match a {
        TimeAgg::Sum => "timeseries.agg_sum",
        TimeAgg::Mean => "timeseries.agg_mean",
        TimeAgg::Min => "timeseries.agg_min",
        TimeAgg::Max => "timeseries.agg_max",
        TimeAgg::Count => "timeseries.agg_count",
        TimeAgg::First => "timeseries.agg_first",
        TimeAgg::Last => "timeseries.agg_last",
    })
}

/// Column indices with the date-like ones first, so the time picker opens on
/// something plausible instead of column 0.
fn time_column_order(table: &octa::data::DataTable) -> Vec<usize> {
    let (mut dated, mut rest): (Vec<usize>, Vec<usize>) = (0..table.col_count()).partition(|&c| {
        table
            .columns
            .get(c)
            .map(|info| info.data_type.contains("Date") || info.data_type.contains("Timestamp"))
            .unwrap_or(false)
    });
    dated.append(&mut rest);
    dated
}

/// Resolve the state's column indices to names and defer to the pure builders.
/// The `Err` text doubles as the "why is Create tab disabled" note.
fn build_sql(st: &TimeseriesState, cols: &[String]) -> Result<String, String> {
    let name = |i: usize| cols.get(i).cloned().unwrap_or_default();
    match st.kind {
        TimeseriesKind::Resample => {
            let spec = ResampleSpec {
                time_col: name(
                    st.time_col
                        .ok_or_else(|| octa::i18n::t("timeseries.need_time"))?,
                ),
                value_cols: st.value_cols.iter().map(|&i| name(i)).collect(),
                interval: st.interval,
                agg: st.agg,
                group_by: st.group_cols.iter().map(|&i| name(i)).collect(),
            };
            build_resample_sql(&spec, cols).map_err(|e| e.to_string())
        }
        TimeseriesKind::Rolling => {
            let window: usize = st
                .window_text
                .trim()
                .replace([',', '.', ' '], "")
                .parse()
                .map_err(|_| octa::i18n::t("timeseries.bad_window"))?;
            let spec = RollingSpec {
                order_col: name(
                    st.order_col
                        .ok_or_else(|| octa::i18n::t("timeseries.need_order"))?,
                ),
                value_col: name(
                    st.roll_value_col
                        .ok_or_else(|| octa::i18n::t("timeseries.need_value"))?,
                ),
                window,
                agg: st.agg,
                partition_by: st.partition_cols.iter().map(|&i| name(i)).collect(),
            };
            build_rolling_sql(&spec, cols).map_err(|e| e.to_string())
        }
    }
}

/// Plain-language sentence describing the configured reshape, deferring to the
/// pure `octa::data::timeseries` explain helpers.
fn explain_text(st: &TimeseriesState, cols: &[String]) -> String {
    let name = |i: usize| cols.get(i).cloned().unwrap_or_default();
    match st.kind {
        TimeseriesKind::Resample => {
            let Some(time_col) = st.time_col else {
                return octa::i18n::t("timeseries.need_time");
            };
            explain_resample(&ResampleSpec {
                time_col: name(time_col),
                value_cols: st.value_cols.iter().map(|&i| name(i)).collect(),
                interval: st.interval,
                agg: st.agg,
                group_by: st.group_cols.iter().map(|&i| name(i)).collect(),
            })
        }
        TimeseriesKind::Rolling => {
            let (Some(order_col), Some(value_col)) = (st.order_col, st.roll_value_col) else {
                return octa::i18n::t("timeseries.need_order");
            };
            explain_rolling(&RollingSpec {
                order_col: name(order_col),
                value_col: name(value_col),
                window: st.window_text.trim().parse().unwrap_or(0),
                agg: st.agg,
                partition_by: st.partition_cols.iter().map(|&i| name(i)).collect(),
            })
        }
    }
}

/// Hash of the inputs the preview depends on, so it is recomputed only when one
/// of them changes.
fn preview_key(st: &TimeseriesState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    matches!(st.kind, TimeseriesKind::Resample).hash(&mut h);
    st.time_col.hash(&mut h);
    st.value_cols.hash(&mut h);
    st.interval.unit().hash(&mut h);
    st.agg.sql_fn().hash(&mut h);
    st.group_cols.hash(&mut h);
    st.order_col.hash(&mut h);
    st.roll_value_col.hash(&mut h);
    st.window_text.hash(&mut h);
    st.partition_cols.hash(&mut h);
    h.finish()
}

/// Run the reshape on a capped sample of the active table and return the first
/// rows, for the dialog preview. `None` when the inputs aren't sufficient yet
/// (so no SQL runs); `Err` carries the failure text.
fn compute_preview(
    app: &OctaApp,
    st: &TimeseriesState,
    cols: &[String],
) -> Option<Result<octa::data::DataTable, String>> {
    let sql = build_sql(st, cols).ok()?;
    let mut snap = app.tabs[app.active_tab].table.clone();
    snap.apply_edits();
    snap.rows.truncate(PREVIEW_SOURCE_ROWS);
    snap.source_path = None;
    snap.total_rows = None;
    match octa::sql::run_query(&snap, &sql) {
        Ok(outcome) => {
            let mut table = outcome.table;
            table.rows.truncate(PREVIEW_RESULT_ROWS);
            Some(Ok(table))
        }
        Err(e) => Some(Err(e.to_string())),
    }
}

/// Run the reshape. On a database tab that does not hold every row:
/// Resample on a typed time column goes to the server, on a text one it
/// runs on the loaded rows and says why; Rolling asks first (see
/// [`render_rolling_ask`]). A file tab runs as before. Returns whether the
/// dialog is done.
fn execute_timeseries(
    app: &mut OctaApp,
    ctx: &egui::Context,
    st: &mut TimeseriesState,
    cols: &[String],
) -> bool {
    let sql = match build_sql(st, cols) {
        Ok(sql) => sql,
        Err(e) => {
            app.status_message = Some((e, std::time::Instant::now()));
            return true;
        }
    };
    let verb = match st.kind {
        TimeseriesKind::Resample => octa::i18n::t("timeseries.resample"),
        TimeseriesKind::Rolling => octa::i18n::t("timeseries.rolling"),
    };
    let Some(src) = app.active_server_source() else {
        app.open_reshape_tab_local(&sql, &verb, "timeseries.failed", None);
        return true;
    };
    let tab = &app.tabs[app.active_tab];
    let loaded = octa::ui::status_bar::format_number(tab.table.rows.len());
    let name = |i: Option<usize>| i.and_then(|i| cols.get(i)).cloned().unwrap_or_default();
    let names = |v: &[usize]| {
        v.iter()
            .filter_map(|&i| cols.get(i).cloned())
            .collect::<Vec<_>>()
    };
    match st.kind {
        TimeseriesKind::Resample => {
            let time_type = st
                .time_col
                .and_then(|i| tab.table.columns.get(i))
                .map(|c| c.data_type.clone())
                .unwrap_or_default();
            if !octa::db::pushdown::dialect::is_time_type(&time_type) {
                let reason = octa::i18n::t("pushdown.time_text_loaded")
                    .replace("{loaded}", &loaded)
                    .replace("{column}", &name(st.time_col));
                app.open_reshape_tab_local(&sql, &verb, "timeseries.failed", Some(reason));
                return true;
            }
            let spec = ResampleSpec {
                time_col: name(st.time_col),
                value_cols: names(&st.value_cols),
                interval: st.interval,
                agg: st.agg,
                group_by: names(&st.group_cols),
            };
            let job = ReshapeJob {
                spec: ReshapeSpec::Resample {
                    columns: cols.to_vec(),
                    spec,
                },
                local_sql: sql,
                label: verb,
                failed_key: "timeseries.failed",
            };
            app.start_pushdown(ctx, PushdownKind::Reshape(Box::new(job)), src);
            true
        }
        TimeseriesKind::Rolling => {
            // build_sql above has parsed it already.
            let window = st
                .window_text
                .trim()
                .replace([',', '.', ' '], "")
                .parse()
                .unwrap_or(1);
            let spec = RollingSpec {
                order_col: name(st.order_col),
                value_col: name(st.roll_value_col),
                window,
                agg: st.agg,
                partition_by: names(&st.partition_cols),
            };
            let Some(origin) = tab.db_origin.as_ref() else {
                return true;
            };
            let source = SourceKey::of(origin);
            let tie = match origin.identity.as_ref() {
                Some(octa::db::write_back::RowIdentity::Key(k)) => k.clone(),
                _ => Vec::new(),
            };
            let server_ok = octa::db::pushdown::timeseries::rolling_supported(src.engine(), st.agg);
            let count_src = src.clone();
            let count = app.spawn_server_task(src.conn.clone(), verb.clone(), move |c, stop| {
                octa::db::pushdown::count_rows(c, &count_src, stop)
            });
            st.rolling_ask = Some(RollingAsk {
                count: Some(count),
                total: None,
                job: ReshapeJob {
                    spec: ReshapeSpec::Rolling {
                        spec,
                        tie,
                        total: 0,
                    },
                    local_sql: sql,
                    label: verb,
                    failed_key: "timeseries.failed",
                },
                src,
                source,
                server_ok,
            });
            false
        }
    }
}

/// Where should the rolling window run: on the database (exact; every row,
/// or the first `cap` in time order) or on the loaded rows (with the
/// reason). Returns true once answered with one of those two.
fn render_rolling_ask(app: &mut OctaApp, ctx: &egui::Context, st: &mut TimeseriesState) -> bool {
    let Some(ask) = st.rolling_ask.as_mut() else {
        return false;
    };
    if let Some(task) = &ask.count {
        match task.poll() {
            TaskPoll::Pending => ctx.request_repaint(),
            TaskPoll::Ready(n) => {
                ask.count = None;
                ask.total = Some(Ok(n));
            }
            TaskPoll::Cancelled => {
                ask.count = None;
                ask.total = Some(Err(octa::i18n::t("pushdown.cancelled")));
            }
            TaskPoll::Failed(e) => {
                ask.count = None;
                ask.total = Some(Err(e));
            }
        }
    }
    let n = octa::ui::status_bar::format_number;
    let cap = octa::db::pushdown::lookups::fetch_cap(octa::formats::initial_load_rows());
    let engine = ask.src.engine();
    enum Answer {
        Server,
        Loaded,
        Cancel,
    }
    let mut answer: Option<Answer> = None;
    egui::Window::new("octa_rolling_ask")
        .id(egui::Id::new("octa_rolling_ask"))
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .order(egui::Order::Foreground)
        .default_pos(octa::ui::settings::center_on_first_show(
            ctx,
            egui::vec2(440.0, 200.0),
        ))
        .default_width(440.0)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("pushdown.rolling_title"))
                    .strong()
                    .size(16.0),
            );
            ui.add_space(6.0);
            match &ask.total {
                None => {
                    octa::ui::control_row::control_row(ui, |ui| {
                        ui.spinner();
                        ui.weak(octa::i18n::t("dbview.facet_counting"));
                    });
                }
                Some(Ok(total)) => {
                    ui.label(octa::i18n::t("pushdown.rolling_body").replace("{total}", &n(*total)));
                    if *total > cap {
                        ui.label(
                            octa::i18n::t("pushdown.rolling_capped").replace("{cap}", &n(cap)),
                        );
                    }
                }
                Some(Err(e)) => {
                    let colour = ui.visuals().error_fg_color;
                    octa::ui::message::selectable_message(
                        ui,
                        colour,
                        &format!("{} {e}", octa::i18n::t("pushdown.rolling_count_failed")),
                    );
                }
            }
            ui.add_space(8.0);
            octa::ui::control_row::control_row(ui, |ui| {
                let counted = matches!(ask.total, Some(Ok(_)));
                let b = ui.add_enabled(
                    counted && ask.server_ok,
                    egui::Button::new(octa::i18n::t("pushdown.rolling_server")),
                );
                let b = if !ask.server_ok {
                    b.on_disabled_hover_text(
                        octa::i18n::t("pushdown.rolling_engine_hint")
                            .replace("{engine}", engine.label()),
                    )
                } else if counted {
                    b.on_hover_text(octa::i18n::t("pushdown.rolling_server_hint"))
                } else if ask.total.is_none() {
                    b.on_disabled_hover_text(octa::i18n::t("dbview.facet_counting"))
                } else {
                    b.on_disabled_hover_text(octa::i18n::t("pushdown.rolling_count_failed"))
                };
                if b.clicked() {
                    answer = Some(Answer::Server);
                }
                if ui
                    .button(octa::i18n::t("pushdown.use_loaded"))
                    .on_hover_text(octa::i18n::t("pushdown.rolling_loaded_hint"))
                    .clicked()
                {
                    answer = Some(Answer::Loaded);
                }
                if ui
                    .button(octa::i18n::t("common.cancel"))
                    .on_hover_text(octa::i18n::t("pushdown.prompt_cancel_hint"))
                    .clicked()
                {
                    answer = Some(Answer::Cancel);
                }
            });
        });
    let answer = match answer {
        None => return false,
        Some(Answer::Cancel) => {
            // Dropping a count still running cancels it on the server.
            st.rolling_ask = None;
            return false;
        }
        Some(a) => a,
    };
    let Some(mut ask) = st.rolling_ask.take() else {
        return false;
    };
    // Back to the tab the question was asked for.
    let Some(idx) = ask.source.find(&app.tabs) else {
        app.status_message = Some((
            octa::i18n::t("pushdown.source_closed_hint"),
            std::time::Instant::now(),
        ));
        return false;
    };
    app.active_tab = idx;
    match answer {
        Answer::Cancel => false,
        Answer::Loaded => {
            let loaded = app.tabs[idx].table.rows.len();
            let reason = octa::i18n::t("pushdown.rolling_local").replace("{loaded}", &n(loaded));
            app.open_reshape_tab_local(
                &ask.job.local_sql,
                &ask.job.label,
                ask.job.failed_key,
                Some(reason),
            );
            true
        }
        Answer::Server => {
            if let (ReshapeSpec::Rolling { total, .. }, Some(Ok(n))) =
                (&mut ask.job.spec, &ask.total)
            {
                *total = *n;
            }
            app.start_pushdown(ctx, PushdownKind::Reshape(Box::new(ask.job)), ask.src);
            true
        }
    }
}
