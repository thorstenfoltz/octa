//! "Customer segments (RFM)" dialog: every customer marked 1 to 5 for how
//! recently, how often and how much they bought, and named. The work is
//! `octa::data::rfm`, shared with `--rfm` and the MCP tool `rfm_segments`. A
//! database tab whose Date column is a typed date adds up on the server
//! (`db::pushdown::rfm`); otherwise the loaded rows are used.

use std::time::Instant;

use eframe::egui;
use egui::RichText;

use octa::data::rfm::{self, Names, RfmSpec, Totals};
use octa::i18n::t;
use octa::ui::control_row::{control_grid, control_row, control_text_edit};
use octa::ui::menu_entry::disabled_tip;
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

use super::super::pushdown::{ServerTask, TaskPoll};
use super::super::state::{OctaApp, TabState};
use super::widgets::col_combo;

#[derive(Default)]
pub(crate) struct RfmState {
    pub(crate) customer: Option<usize>,
    pub(crate) date: Option<usize>,
    pub(crate) amount: Option<usize>,
    pub(crate) order: Option<usize>,
    pub(crate) as_of_buf: String,
    pub(crate) names: Names,
    /// The server run, while it is going.
    pub(crate) task: Option<ServerTask<Totals>>,
    pub(crate) error: Option<String>,
    size: DialogSize,
}

/// Whether a column's declared type is a date or a timestamp.
fn is_date_type(data_type: &str) -> bool {
    let t = data_type.to_ascii_lowercase();
    t.contains("date") || t.contains("timestamp")
}

/// The As of field: empty is the latest date in the data.
fn parse_as_of(buf: &str) -> Result<Option<chrono::NaiveDate>, ()> {
    match buf.trim() {
        "" => Ok(None),
        s => octa::data::time_calc::parse_date(s).map(Some).ok_or(()),
    }
}

impl OctaApp {
    /// Analyse > Customer segments (RFM)... and the shortcut. Preselects the
    /// first date column.
    pub(crate) fn open_rfm(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let table = &tab.table;
        if table.col_count() == 0 {
            return;
        }
        self.rfm_dialog = Some(RfmState {
            date: (0..table.col_count()).find(|&c| is_date_type(&table.columns[c].data_type)),
            ..RfmState::default()
        });
    }

    /// Mark and name the customers and open them as a new tab.
    pub(crate) fn open_rfm_result(
        &mut self,
        totals: &Totals,
        as_of: Option<chrono::NaiveDate>,
        names: Names,
        note: Option<String>,
    ) -> Result<(), String> {
        let (table, as_of) = rfm::table(totals, as_of, names).map_err(|e| e.message())?;
        let source_label = self.tabs[self.active_tab].result_source_label();
        let mut tab = TabState::new(self.settings.default_search_mode);
        tab.table_state.header_tooltips = rfm::COLUMNS
            .iter()
            .take(table.col_count())
            .map(|(_, key)| t(key))
            .collect();
        // The segment names are stable English; hovering one says what it
        // means, in the user's language.
        let mut cell_tooltips = vec![std::collections::HashMap::new(); table.col_count()];
        if let Some(seg) = cell_tooltips.get_mut(8) {
            *seg = rfm::VALUE_HINTS
                .iter()
                .map(|(name, key)| ((*name).to_string(), t(key)))
                .collect();
        }
        tab.table_state.cell_tooltips = cell_tooltips;
        tab.custom_tab_label = Some(format!("{} - {source_label}", t("rfm.title")));
        tab.table = table;
        tab.filter_dirty = true;
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        let mut msg = t("rfm.done").replace("{date}", &as_of.format("%Y-%m-%d").to_string());
        if totals.skipped > 0 {
            msg = format!(
                "{msg} {}",
                t("rfm.skipped").replace("{n}", &totals.skipped.to_string())
            );
        }
        if let Some(n) = note {
            msg = format!("{msg} {n}");
        }
        self.status_message = Some((msg, Instant::now()));
        Ok(())
    }
}

pub(crate) fn render_rfm_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.rfm_dialog.take() else {
        return;
    };
    let Some(tab) = app.tabs.get(app.active_tab) else {
        return;
    };
    let table = &tab.table;
    let col_names: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();
    let n = col_names.len();
    // A column deleted while the dialog was open drops out of the pick.
    for pick in [
        &mut st.customer,
        &mut st.date,
        &mut st.amount,
        &mut st.order,
    ] {
        *pick = pick.filter(|&c| c < n);
    }
    let server = app.active_server_source();
    let date_is_typed = st
        .date
        .is_some_and(|c| is_date_type(&table.columns[c].data_type));
    let partial = tab.source_has_more();
    let as_of = parse_as_of(&st.as_of_buf);

    // A server run that finished since the last frame.
    let mut finished: Option<Totals> = None;
    if let Some(task) = &st.task {
        let done = match task.poll() {
            TaskPoll::Pending => {
                ctx.request_repaint();
                false
            }
            TaskPoll::Ready(totals) => {
                finished = Some(totals);
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
    if let Some(totals) = finished {
        match app.open_rfm_result(&totals, as_of.ok().flatten(), st.names, None) {
            Ok(()) => return,
            Err(e) => st.error = Some(e),
        }
    }

    let running = st.task.is_some();
    let why_not: Option<String> = if running {
        Some(t("cohort.running"))
    } else if st.customer.is_none() || st.date.is_none() || st.amount.is_none() {
        Some(t("rfm.run_why"))
    } else if as_of.is_err() {
        Some(t("rfm.as_of_bad"))
    } else {
        None
    };

    let mut close = false;
    let mut run = false;
    let dialog_id = egui::Id::new("octa_rfm_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(st.size));
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(480.0, 360.0));
    let window = egui::Window::new("octa_rfm")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(360.0)
            .min_width(360.0)
            .default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(ui, "rfm_header", RichText::new(t("rfm.title")), &mut size) {
            close = true;
        }
        if minimized {
            return;
        }

        egui::Panel::bottom("rfm_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                control_row(ui, |ui| {
                    let b = ui.add_enabled(why_not.is_none(), egui::Button::new(t("cohort.run")));
                    if b.clicked() {
                        run = true;
                    }
                    match &why_not {
                        None => {
                            b.on_hover_text(t("rfm.run_hint"));
                        }
                        Some(w) => {
                            b.on_disabled_hover_text(disabled_tip(&t("rfm.run_hint"), w));
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
                let key = if date_is_typed {
                    "cohort.on_server"
                } else {
                    "rfm.on_loaded"
                };
                octa::ui::message::partial_note_label(ui, &t(key));
                ui.add_space(6.0);
            }
            control_grid(ui, "rfm_grid", |ui| {
                for (label, hint, id, pick) in [
                    (
                        "rfm.customer",
                        "rfm.customer_hint",
                        "rfm_customer",
                        &mut st.customer,
                    ),
                    ("rfm.date", "rfm.date_hint", "rfm_date", &mut st.date),
                    (
                        "rfm.amount",
                        "rfm.amount_hint",
                        "rfm_amount",
                        &mut st.amount,
                    ),
                ] {
                    ui.label(t(label)).on_hover_text(t(hint));
                    col_combo(ui, id, pick, &col_names).on_hover_text(t(hint));
                    ui.end_row();
                }

                ui.label(t("rfm.order")).on_hover_text(t("rfm.order_hint"));
                let text = st
                    .order
                    .and_then(|i| col_names.get(i).cloned())
                    .unwrap_or_else(|| t("rfm.no_order"));
                egui::ComboBox::from_id_salt("rfm_order")
                    .selected_text(text)
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut st.order, None, t("rfm.no_order"));
                        for (i, name) in col_names.iter().enumerate() {
                            ui.selectable_value(&mut st.order, Some(i), name);
                        }
                    })
                    .response
                    .on_hover_text(t("rfm.order_hint"));
                ui.end_row();

                ui.label(t("rfm.as_of")).on_hover_text(t("rfm.as_of_hint"));
                control_text_edit(
                    ui,
                    120.0,
                    egui::TextEdit::singleline(&mut st.as_of_buf).hint_text("2024-12-31"),
                )
                .on_hover_text(t("rfm.as_of_hint"));
                ui.end_row();

                ui.label(t("rfm.names")).on_hover_text(t("rfm.names_hint"));
                egui::ComboBox::from_id_salt("rfm_names")
                    .selected_text(t(st.names.label_key()))
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for names in Names::ALL {
                            ui.selectable_value(&mut st.names, names, t(names.label_key()));
                        }
                    })
                    .response
                    .on_hover_text(t("rfm.names_hint"));
                ui.end_row();
            });
            if as_of.is_err() {
                octa::ui::message::selectable_message(
                    ui,
                    ui.visuals().warn_fg_color,
                    &t("rfm.as_of_bad"),
                );
            }
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

    if run && let (Some(c), Some(d), Some(a), Ok(as_of)) = (st.customer, st.date, st.amount, as_of)
    {
        st.error = None;
        let tab = &app.tabs[app.active_tab];
        match server {
            Some(src) if date_is_typed => {
                let names = &col_names;
                let (customer, date, amount) =
                    (names[c].clone(), names[d].clone(), names[a].clone());
                let amount_type = tab.table.columns[a].data_type.clone();
                let order = st.order.map(|o| names[o].clone());
                let s2 = src.clone();
                st.task = Some(app.spawn_server_task(
                    src.conn.clone(),
                    t("rfm.title"),
                    move |conn, stop| {
                        let spec = octa::db::pushdown::rfm::ServerRfm {
                            customer: &customer,
                            date: &date,
                            amount: &amount,
                            amount_type: &amount_type,
                            order: order.as_deref(),
                            as_of,
                        };
                        octa::db::pushdown::rfm::run(conn, &s2, &spec, stop)
                    },
                ));
            }
            _ => {
                let mut snap = tab.table.clone();
                snap.apply_edits();
                let spec = RfmSpec {
                    customer: c,
                    date: d,
                    amount: a,
                    order: st.order,
                    as_of,
                };
                let totals = rfm::collect(&snap, &spec);
                let note = partial.then(|| t("cohort.loaded_note"));
                match app.open_rfm_result(&totals, as_of, st.names, note) {
                    Ok(()) => return,
                    Err(e) => st.error = Some(e),
                }
            }
        }
    }
    if !close {
        app.rfm_dialog = Some(st);
    }
}

#[cfg(test)]
#[path = "rfm_tests.rs"]
mod tests;
