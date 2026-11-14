//! "Funnel" dialog: how many people get through a series of steps in order,
//! and where they drop out. The work is `octa::data::funnel`, shared with
//! `--funnel` and the MCP tool `funnel`. A database tab whose When column is
//! a typed date runs on the server (`db::pushdown::funnel`); otherwise the
//! loaded rows are used.

use std::time::Instant;

use eframe::egui;
use egui::RichText;

use octa::data::funnel::{self, Counts, FunnelSpec};
use octa::i18n::t;
use octa::ui::control_row::{control_grid, control_line, control_row, control_text_edit};
use octa::ui::menu_entry::disabled_tip;
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

use super::super::pushdown::{ServerTask, TaskPoll};
use super::super::state::{OctaApp, TabState};
use super::widgets::col_combo;

/// How many distinct event values the step list offers.
const MAX_VALUES: usize = 1000;

#[derive(Default)]
pub(crate) struct FunnelState {
    pub(crate) who: Option<usize>,
    pub(crate) event: Option<usize>,
    pub(crate) when: Option<usize>,
    /// The ticked events, in funnel order.
    pub(crate) steps: Vec<String>,
    /// The event column's values, read once per column pick.
    values: Vec<String>,
    values_for: Option<usize>,
    capped: bool,
    pub(crate) within_buf: String,
    /// The server run, while it is going.
    pub(crate) task: Option<ServerTask<Counts>>,
    pub(crate) error: Option<String>,
    size: DialogSize,
}

/// Whether a column's declared type is a date or a timestamp.
fn is_date_type(data_type: &str) -> bool {
    let t = data_type.to_ascii_lowercase();
    t.contains("date") || t.contains("timestamp")
}

/// The Within days field: empty is no limit, else a whole number from 1.
fn parse_within(buf: &str) -> Result<Option<u32>, ()> {
    match buf.trim() {
        "" => Ok(None),
        s => s
            .parse::<u32>()
            .ok()
            .filter(|d| *d >= 1)
            .map(Some)
            .ok_or(()),
    }
}

impl OctaApp {
    /// Analyse > Funnel... and the shortcut. Preselects the first date
    /// column for When.
    pub(crate) fn open_funnel(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let table = &tab.table;
        if table.col_count() == 0 {
            return;
        }
        self.funnel_dialog = Some(FunnelState {
            when: (0..table.col_count()).find(|&c| is_date_type(&table.columns[c].data_type)),
            ..FunnelState::default()
        });
    }

    /// Open the funnel table as a new tab.
    pub(crate) fn open_funnel_result(&mut self, counts: &Counts, note: Option<String>) {
        let source_label = self.tabs[self.active_tab].result_source_label();
        let mut tab = TabState::new(self.settings.default_search_mode);
        tab.table_state.header_tooltips = [
            "funnel.hint_step",
            "funnel.hint_users",
            "funnel.hint_share_first",
            "funnel.hint_share_prev",
            "funnel.hint_dropped",
        ]
        .into_iter()
        .map(t)
        .collect();
        tab.custom_tab_label = Some(format!("{} - {source_label}", t("funnel.title")));
        tab.table = funnel::table(counts);
        tab.filter_dirty = true;
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        let mut msg = t("funnel.done");
        if counts.skipped > 0 {
            msg = format!(
                "{msg} {}",
                t("cohort.skipped").replace("{n}", &counts.skipped.to_string())
            );
        }
        if let Some(n) = note {
            msg = format!("{msg} {n}");
        }
        self.status_message = Some((msg, Instant::now()));
    }
}

pub(crate) fn render_funnel_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.funnel_dialog.take() else {
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
    st.event = st.event.filter(|&c| c < n);
    st.when = st.when.filter(|&c| c < n);
    if st.values_for != st.event {
        (st.values, st.capped) = st
            .event
            .map(|c| funnel::event_values(table, c, MAX_VALUES))
            .unwrap_or_default();
        st.values_for = st.event;
        st.steps.clear();
    }
    let server = app.active_server_source();
    let when_is_date = st
        .when
        .is_some_and(|c| is_date_type(&table.columns[c].data_type));
    let partial = tab.source_has_more();

    // A server run that finished since the last frame.
    let mut finished: Option<Counts> = None;
    if let Some(task) = &st.task {
        let done = match task.poll() {
            TaskPoll::Pending => {
                ctx.request_repaint();
                false
            }
            TaskPoll::Ready(counts) => {
                finished = Some(counts);
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
    if let Some(counts) = finished {
        app.open_funnel_result(&counts, None);
        return;
    }

    let running = st.task.is_some();
    let within = parse_within(&st.within_buf);
    let why_not: Option<String> = if running {
        Some(t("cohort.running"))
    } else if st.who.is_none() || st.event.is_none() || st.steps.len() < 2 {
        Some(t("funnel.run_why"))
    } else if within.is_err() {
        Some(t("funnel.bad_within"))
    } else {
        None
    };

    let mut close = false;
    let mut run = false;
    let dialog_id = egui::Id::new("octa_funnel_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(st.size));
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(480.0, 520.0));
    let window = egui::Window::new("octa_funnel")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(520.0)
            .min_width(360.0)
            .default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "funnel_header",
            RichText::new(t("funnel.title")),
            &mut size,
        ) {
            close = true;
        }
        if minimized {
            return;
        }

        egui::Panel::bottom("funnel_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                control_row(ui, |ui| {
                    let b = ui.add_enabled(why_not.is_none(), egui::Button::new(t("cohort.run")));
                    if b.clicked() {
                        run = true;
                    }
                    match &why_not {
                        None => {
                            b.on_hover_text(t("funnel.run_hint"));
                        }
                        Some(w) => {
                            b.on_disabled_hover_text(disabled_tip(&t("funnel.run_hint"), w));
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
                    "funnel.on_loaded"
                };
                octa::ui::message::partial_note_label(ui, &t(key));
                ui.add_space(6.0);
            }
            control_grid(ui, "funnel_grid", |ui| {
                ui.label(t("cohort.who"))
                    .on_hover_text(t("cohort.who_hint"));
                col_combo(ui, "funnel_who", &mut st.who, &col_names)
                    .on_hover_text(t("cohort.who_hint"));
                ui.end_row();

                ui.label(t("funnel.event"))
                    .on_hover_text(t("funnel.event_hint"));
                col_combo(ui, "funnel_event", &mut st.event, &col_names)
                    .on_hover_text(t("funnel.event_hint"));
                ui.end_row();

                ui.label(t("cohort.when"))
                    .on_hover_text(t("funnel.when_hint"));
                let text = st
                    .when
                    .and_then(|i| col_names.get(i).cloned())
                    .unwrap_or_else(|| t("funnel.no_when"));
                egui::ComboBox::from_id_salt("funnel_when")
                    .selected_text(text)
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut st.when, None, t("funnel.no_when"));
                        for (i, name) in col_names.iter().enumerate() {
                            ui.selectable_value(&mut st.when, Some(i), name);
                        }
                    })
                    .response
                    .on_hover_text(t("funnel.when_hint"));
                ui.end_row();

                ui.label(t("funnel.within"))
                    .on_hover_text(t("funnel.within_hint"));
                let has_when = st.when.is_some();
                let r = ui
                    .add_enabled_ui(has_when, |ui| {
                        control_text_edit(ui, 80.0, egui::TextEdit::singleline(&mut st.within_buf));
                    })
                    .response;
                if has_when {
                    r.on_hover_text(t("funnel.within_hint"));
                } else {
                    st.within_buf.clear();
                    r.on_disabled_hover_text(disabled_tip(
                        &t("funnel.within_hint"),
                        &t("funnel.within_why"),
                    ));
                }
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.label(RichText::new(t("funnel.steps")).strong())
                .on_hover_text(t("funnel.steps_hint"));
            if let Some(e) = &st.error {
                octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
            }
            egui::ScrollArea::vertical()
                .id_salt("funnel_steps")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if st.values.is_empty() {
                        ui.weak(t("funnel.steps_empty"));
                    }
                    // The ticked steps first, in funnel order, with arrows.
                    let mut swap: Option<(usize, usize)> = None;
                    let mut untick: Option<usize> = None;
                    let last = st.steps.len().saturating_sub(1);
                    for (i, step) in st.steps.iter().enumerate() {
                        control_line(ui, |ui| {
                            let mut on = true;
                            if ui
                                .checkbox(&mut on, format!("{}. {step}", i + 1))
                                .on_hover_text(t("funnel.steps_hint"))
                                .changed()
                            {
                                untick = Some(i);
                            }
                            if ui
                                .add_enabled(i > 0, egui::Button::new("^"))
                                .on_hover_text(t("funnel.up_hint"))
                                .on_disabled_hover_text(t("funnel.up_hint"))
                                .clicked()
                            {
                                swap = Some((i, i - 1));
                            }
                            if ui
                                .add_enabled(i < last, egui::Button::new("v"))
                                .on_hover_text(t("funnel.down_hint"))
                                .on_disabled_hover_text(t("funnel.down_hint"))
                                .clicked()
                            {
                                swap = Some((i, i + 1));
                            }
                        });
                    }
                    if let Some((a, b)) = swap {
                        st.steps.swap(a, b);
                    }
                    if let Some(i) = untick {
                        st.steps.remove(i);
                    }
                    if !st.steps.is_empty() {
                        ui.separator();
                    }
                    let mut tick: Option<String> = None;
                    for v in st.values.iter().filter(|v| !st.steps.contains(v)) {
                        let mut on = false;
                        if ui
                            .checkbox(&mut on, v)
                            .on_hover_text(t("funnel.steps_hint"))
                            .changed()
                        {
                            tick = Some(v.clone());
                        }
                    }
                    if let Some(v) = tick {
                        st.steps.push(v);
                    }
                    if st.capped {
                        ui.weak(t("funnel.steps_capped").replace("{n}", &MAX_VALUES.to_string()));
                    }
                });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;
    ctx.data_mut(|d| d.insert_temp(size_key, size));

    if run && let (Some(w), Some(e), Ok(within_days)) = (st.who, st.event, within) {
        st.error = None;
        let tab = &app.tabs[app.active_tab];
        match (server, st.when) {
            (Some(src), Some(c)) if when_is_date => {
                let names = &col_names;
                let (who, event, when) = (names[w].clone(), names[e].clone(), names[c].clone());
                let steps = st.steps.clone();
                let s2 = src.clone();
                st.task = Some(app.spawn_server_task(
                    src.conn.clone(),
                    t("funnel.title"),
                    move |conn, stop| {
                        let spec = octa::db::pushdown::funnel::ServerFunnel {
                            who: &who,
                            event: &event,
                            when: &when,
                            steps: &steps,
                            within_days,
                        };
                        octa::db::pushdown::funnel::run(conn, &s2, &spec, stop)
                    },
                ));
            }
            _ => {
                let mut snap = tab.table.clone();
                snap.apply_edits();
                let spec = FunnelSpec {
                    who: w,
                    event: e,
                    when: st.when,
                    steps: st.steps.clone(),
                    within_days,
                };
                match funnel::collect(&snap, &spec) {
                    Ok(counts) => {
                        let note = partial.then(|| t("cohort.loaded_note"));
                        app.open_funnel_result(&counts, note);
                        return;
                    }
                    Err(e) => st.error = Some(t(e.key())),
                }
            }
        }
    }
    if !close {
        app.funnel_dialog = Some(st);
    }
}

#[cfg(test)]
#[path = "funnel_tests.rs"]
mod tests;
