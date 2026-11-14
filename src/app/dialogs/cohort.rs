//! "Cohorts" dialog: who first showed up in which period, and how many of
//! them (or how much of a number column) came back in each later period.
//! The work is `octa::data::cohort`, shared with `--cohort` and the MCP tool
//! `cohort_table`. A database tab whose When column is a typed date runs on
//! the server (`db::pushdown::cohort`); otherwise the loaded rows are used.

use std::time::Instant;

use eframe::egui;
use egui::RichText;

use octa::data::cohort::{self, Cells, CohortSpec, Period, Show};
use octa::i18n::t;
use octa::ui::control_row::{control_grid, control_row};
use octa::ui::menu_entry::disabled_tip;
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

use super::super::pushdown::{ServerTask, TaskPoll};
use super::super::state::{OctaApp, TabState};
use super::timeseries::time_column_order;
use super::widgets::{col_combo, col_combo_ordered};

#[derive(Default)]
pub(crate) struct CohortState {
    pub(crate) who: Option<usize>,
    pub(crate) when: Option<usize>,
    pub(crate) period: Period,
    pub(crate) show: Show,
    pub(crate) sum_col: Option<usize>,
    pub(crate) heatmap: bool,
    /// The server run, while it is going.
    pub(crate) task: Option<ServerTask<Cells>>,
    pub(crate) error: Option<String>,
    size: DialogSize,
}

/// Whether a column's declared type is a date or a timestamp.
fn is_date_type(data_type: &str) -> bool {
    let t = data_type.to_ascii_lowercase();
    t.contains("date") || t.contains("timestamp")
}

impl OctaApp {
    /// Analyse > Cohorts... and the shortcut. Preselects the first date
    /// column for When (or a text column whose first value reads as a date).
    pub(crate) fn open_cohorts(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let table = &tab.table;
        if table.col_count() == 0 {
            return;
        }
        let when = (0..table.col_count())
            .find(|&c| is_date_type(&table.columns[c].data_type))
            .or_else(|| {
                (0..table.col_count()).find(|&c| {
                    (0..table.row_count())
                        .filter_map(|r| table.get(r, c))
                        .find(|v| !matches!(v, octa::data::CellValue::Null))
                        .and_then(cohort::date_of)
                        .is_some()
                })
            });
        self.cohort_dialog = Some(CohortState {
            when,
            heatmap: true,
            ..CohortState::default()
        });
    }

    /// Open the cohort table (and its heatmap) as new tabs.
    pub(crate) fn open_cohort_result(
        &mut self,
        cells: &Cells,
        heatmap: bool,
        note: Option<String>,
    ) {
        let source_label = self.tabs[self.active_tab].result_source_label();
        let table = cohort::wide(cells);
        let periods = table.col_count().saturating_sub(2);
        let mut tab = TabState::new(self.settings.default_search_mode);
        tab.table_state.header_tooltips = [t("cohort.hint_cohort"), t("cohort.hint_size")]
            .into_iter()
            .chain(std::iter::repeat_n(t("cohort.hint_period"), periods))
            .collect();
        tab.custom_tab_label = Some(format!("{} - {source_label}", t("cohort.tab_label")));
        tab.table = table;
        tab.filter_dirty = true;
        let long = heatmap.then(|| cohort::long_from_wide(&tab.table));
        self.tabs.push(tab);
        let result_idx = self.tabs.len() - 1;
        if let Some(long) = long {
            let mut chart = TabState::new(self.settings.default_search_mode);
            chart.filtered_rows = (0..long.row_count()).collect();
            chart.table = long;
            chart.is_chart_tab = true;
            chart.chart_tab_label = Some(format!("{} - {source_label}", t("cohort.heatmap_label")));
            chart.view_mode = octa::data::ViewMode::Chart;
            chart.chart_config = octa::data::chart::ChartConfig {
                kind: octa::data::chart::ChartKind::Heatmap,
                x_col: Some(1),
                row_col: Some(0),
                y_cols: vec![2],
                agg: octa::data::chart::Aggregation::Sum,
                ..Default::default()
            };
            self.tabs.push(chart);
        }
        self.active_tab = result_idx;
        let mut msg = t("cohort.done");
        if cells.skipped > 0 {
            msg = format!(
                "{msg} {}",
                t("cohort.skipped").replace("{n}", &cells.skipped.to_string())
            );
        }
        if let Some(n) = note {
            msg = format!("{msg} {n}");
        }
        self.status_message = Some((msg, Instant::now()));
    }
}

pub(crate) fn render_cohort_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.cohort_dialog.take() else {
        return;
    };
    let Some(tab) = app.tabs.get(app.active_tab) else {
        return;
    };
    let table = &tab.table;
    let col_names: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();
    let n = col_names.len();
    // A column deleted while the dialog was open drops out of the pick.
    st.who = st.who.filter(|&c| c < n);
    st.when = st.when.filter(|&c| c < n);
    st.sum_col = st.sum_col.filter(|&c| c < n);
    let time_order = time_column_order(table);
    let server = app.active_server_source();
    let when_is_date = st
        .when
        .is_some_and(|c| is_date_type(&table.columns[c].data_type));
    let partial = tab.source_has_more();

    // A server run that finished since the last frame.
    let mut finished: Option<Cells> = None;
    if let Some(task) = &st.task {
        let done = match task.poll() {
            TaskPoll::Pending => {
                ctx.request_repaint();
                false
            }
            TaskPoll::Ready(cells) => {
                finished = Some(cells);
                true
            }
            TaskPoll::Failed(e) => {
                st.error = Some(e);
                true
            }
            TaskPoll::Cancelled => {
                st.error = Some(t("pushdown.cancelled"));
                true
            }
        };
        if done {
            st.task = None;
        }
    }
    if let Some(cells) = finished {
        app.open_cohort_result(&cells, st.heatmap, None);
        return;
    }

    let running = st.task.is_some();
    let need_sum = st.show == Show::Sum && st.sum_col.is_none();
    let why_not: Option<String> = if running {
        Some(t("cohort.running"))
    } else if st.who.is_none() || st.when.is_none() {
        Some(t("cohort.run_why"))
    } else if need_sum {
        Some(t("cohort.err_no_sum"))
    } else {
        None
    };

    let mut close = false;
    let mut run = false;
    let dialog_id = egui::Id::new("octa_cohort_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(st.size));
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(480.0, 380.0));
    let window = egui::Window::new("octa_cohort")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(380.0)
            .min_width(360.0)
            .default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "cohort_header",
            RichText::new(t("cohort.title")),
            &mut size,
        ) {
            close = true;
        }
        if minimized {
            return;
        }

        egui::Panel::bottom("cohort_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                control_row(ui, |ui| {
                    let b = ui.add_enabled(why_not.is_none(), egui::Button::new(t("cohort.run")));
                    if b.clicked() {
                        run = true;
                    }
                    match &why_not {
                        None => {
                            b.on_hover_text(t("cohort.run_hint"));
                        }
                        Some(w) => {
                            b.on_disabled_hover_text(disabled_tip(&t("cohort.run_hint"), w));
                        }
                    }
                    if running {
                        ui.spinner();
                    }
                    if ui
                        .button(t("common.cancel"))
                        .on_hover_text(t("cohort.cancel_hint"))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            if server.is_some() {
                let key = if when_is_date {
                    "cohort.on_server"
                } else {
                    "cohort.on_loaded"
                };
                octa::ui::message::partial_note_label(ui, &t(key));
                ui.add_space(6.0);
            }
            control_grid(ui, "cohort_grid", |ui| {
                ui.label(t("cohort.who"))
                    .on_hover_text(t("cohort.who_hint"));
                col_combo(ui, "cohort_who", &mut st.who, &col_names)
                    .on_hover_text(t("cohort.who_hint"));
                ui.end_row();

                ui.label(t("cohort.when"))
                    .on_hover_text(t("cohort.when_hint"));
                col_combo_ordered(
                    ui,
                    "cohort_when",
                    &mut st.when,
                    &col_names,
                    Some(&time_order),
                )
                .on_hover_text(t("cohort.when_hint"));
                ui.end_row();

                ui.label(t("cohort.period"))
                    .on_hover_text(t("cohort.period_hint"));
                egui::ComboBox::from_id_salt("cohort_period")
                    .selected_text(t(st.period.key()))
                    .show_ui(ui, |ui| {
                        for p in Period::ALL {
                            ui.selectable_value(&mut st.period, p, t(p.key()));
                        }
                    })
                    .response
                    .on_hover_text(t("cohort.period_hint"));
                ui.end_row();

                ui.label(t("cohort.show"))
                    .on_hover_text(t("cohort.show_hint"));
                egui::ComboBox::from_id_salt("cohort_show")
                    .selected_text(t(st.show.key()))
                    .show_ui(ui, |ui| {
                        for s in Show::ALL {
                            ui.selectable_value(&mut st.show, s, t(s.key()));
                        }
                    })
                    .response
                    .on_hover_text(t("cohort.show_hint"));
                ui.end_row();

                ui.label(t("cohort.sum_col"))
                    .on_hover_text(t("cohort.sum_col_hint"));
                let sum = st.show == Show::Sum;
                let r = ui
                    .add_enabled_ui(sum, |ui| {
                        col_combo(ui, "cohort_sum", &mut st.sum_col, &col_names);
                    })
                    .response;
                if sum {
                    r.on_hover_text(t("cohort.sum_col_hint"));
                } else {
                    r.on_disabled_hover_text(disabled_tip(
                        &t("cohort.sum_col_hint"),
                        &t("cohort.sum_why"),
                    ));
                }
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.checkbox(&mut st.heatmap, t("cohort.heatmap"))
                .on_hover_text(t("cohort.heatmap_hint"));
            if let Some(e) = &st.error {
                octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
            }
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;
    ctx.data_mut(|d| d.insert_temp(size_key, size));

    if run && let (Some(w), Some(c)) = (st.who, st.when) {
        st.error = None;
        let tab = &app.tabs[app.active_tab];
        match server {
            Some(src) if when_is_date => {
                let names = &col_names;
                let (who, when) = (names[w].clone(), names[c].clone());
                let sum = st.sum_col.map(|i| names[i].clone());
                let sum_type = st
                    .sum_col
                    .and_then(|i| tab.table.columns.get(i))
                    .map(|c| c.data_type.clone())
                    .unwrap_or_default();
                let (period, show) = (st.period, st.show);
                let s2 = src.clone();
                st.task = Some(app.spawn_server_task(
                    src.conn.clone(),
                    t("cohort.title"),
                    move |conn, stop| {
                        let spec = octa::db::pushdown::cohort::ServerCohort {
                            who: &who,
                            when: &when,
                            sum: sum.as_deref(),
                            sum_type: &sum_type,
                            period,
                            show,
                        };
                        octa::db::pushdown::cohort::run(conn, &s2, &spec, stop)
                    },
                ));
            }
            _ => {
                let mut snap = tab.table.clone();
                snap.apply_edits();
                let spec = CohortSpec {
                    who: w,
                    when: c,
                    period: st.period,
                    show: st.show,
                    sum_col: st.sum_col,
                };
                match cohort::collect(&snap, &spec) {
                    Ok(cells) => {
                        let note = partial.then(|| t("cohort.loaded_note"));
                        app.open_cohort_result(&cells, st.heatmap, note);
                        return;
                    }
                    Err(e) => st.error = Some(t(e.key())),
                }
            }
        }
    }
    if !close {
        app.cohort_dialog = Some(st);
    }
}
