//! Find lookup tables dialog (**Analyse -> Find lookup tables...**). Scans
//! the active table on a worker for columns that always follow another
//! column - `customer_id -> customer_name, city` - so a flat export can be
//! split back into a lookup table and a slim main table.
//!
//! Modelled on `find_fuzzy_duplicates.rs`: a background worker with polled
//! `Arc<Mutex<Option<..>>>` slots, `running`/`cancel` `AtomicBool`s, and
//! `FlagOnDrop` so a panicked worker never wedges the spinner on. The scan
//! itself is the pure [`octa::data::lookups::find_lookups`].
//!
//! On a live-database tab that does not hold every row the scan runs on the
//! server (`octa::db::pushdown::lookups`), and Show breaking rows / Split out
//! then ask whether to use the loaded rows or fetch from the database.

use std::sync::atomic::Ordering;

use eframe::egui;
use egui::RichText;

use octa::ui::control_row::control_row;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

use crate::app::pushdown::{DialogNote, ServerUi, TaskPoll};
use crate::app::state::{LookupAct, LookupsState, OctaApp};

/// Rows fetched from the server for an action; the `bool` says the row cap
/// cut them.
pub(crate) enum LookupFetch {
    // Boxed: a table is several hundred bytes inline.
    Breaking(Box<octa::data::DataTable>, bool),
    Split(Box<octa::data::lookups::Split>, bool),
}

#[derive(Debug, PartialEq)]
enum Route {
    Ask(LookupAct),
    Now(LookupAct),
}

/// After a server result the counts cover rows the tab does not hold, so
/// the user picks where the rows come from.
fn route(act: LookupAct, from_server: bool) -> Route {
    if from_server {
        Route::Ask(act)
    } else {
        Route::Now(act)
    }
}

/// Spawn the scan worker on a clone of the tab's table (edits applied).
/// Called right after the state is built (toolbar handler + shortcut
/// dispatch, and **Scan again**), so the dialog opens already scanning.
pub(crate) fn start_scan(app: &OctaApp, st: &mut LookupsState) {
    start_scan_on(app, st, true);
}

/// `server_ok: false` is Run on loaded rows after a server error or Cancel.
fn start_scan_on(app: &OctaApp, st: &mut LookupsState, server_ok: bool) {
    st.server = None;
    st.fetch = None;
    st.server_error = None;
    st.note = None;
    st.total = None;
    st.prompt = None;
    let tab = &app.tabs[st.tab];
    let src = server_ok
        .then(|| {
            crate::app::pushdown::server_source_for(
                tab,
                app.settings.db_pushdown,
                &app.settings.db_connections,
            )
        })
        .flatten();
    if let Some(src) = src {
        let columns = tab.table.columns.clone();
        let min = st.min_consistency_pct / 100.0;
        st.findings = None;
        st.note = Some(DialogNote {
            unsaved: tab.table.is_modified(),
            ..Default::default()
        });
        st.server = Some(app.spawn_server_task(
            src.conn.clone(),
            octa::i18n::t("lookups.title"),
            move |c, stop| {
                let total = octa::db::pushdown::count_rows(c, &src, stop)?;
                let found = octa::db::pushdown::lookups::run(c, &src, &columns, total, min, stop)?;
                Ok((found, total))
            },
        ));
        return;
    }
    let mut table = app.tabs[st.tab].table.clone();
    table.apply_edits();
    let min = st.min_consistency_pct / 100.0;
    let (result, running, cancel) = (st.result.clone(), st.running.clone(), st.cancel.clone());
    cancel.store(false, Ordering::Relaxed);
    running.store(true, Ordering::Relaxed);
    st.findings = None;
    std::thread::spawn(move || {
        // Clears `running` however the worker ends: a panic here used to
        // wedge the flag true for the rest of the session.
        let _running = crate::app::flag_guard::FlagOnDrop::new(running, false);
        let found = octa::data::lookups::find_lookups(&table, min, &cancel);
        if let Ok(mut slot) = result.lock() {
            *slot = Some(found);
        }
    });
}

/// Fetch the rows behind `act` from the server.
fn start_fetch(app: &OctaApp, st: &mut LookupsState, act: LookupAct) {
    let tab = &app.tabs[st.tab];
    let Some(src) = crate::app::pushdown::server_source_for(
        tab,
        app.settings.db_pushdown,
        &app.settings.db_connections,
    ) else {
        return;
    };
    let i = match act {
        LookupAct::Breaking(i) | LookupAct::Split(i) => i,
    };
    let Some(f) = st.findings.as_ref().and_then(|v| v.get(i)) else {
        return;
    };
    let name = |c: usize| tab.table.columns[c].name.clone();
    let key = name(f.key);
    // `ticked` holds positions in `f.dependents`; the engine wants columns.
    let deps: Vec<String> = st.ticked[i]
        .iter()
        .filter_map(|&di| f.dependents.get(di).map(|d| name(d.col)))
        .collect();
    let cap = octa::db::pushdown::lookups::fetch_cap(octa::formats::initial_load_rows());
    st.server_error = None;
    st.fetch_key = key.clone();
    st.fetch = Some(app.spawn_server_task(
        src.conn.clone(),
        octa::i18n::t("lookups.title"),
        move |c, stop| {
            Ok(match act {
                LookupAct::Breaking(_) => {
                    let (t, cut) = octa::db::pushdown::lookups::breaking_rows(
                        c, &src, &key, &deps, cap, stop,
                    )?;
                    LookupFetch::Breaking(Box::new(t), cut)
                }
                LookupAct::Split(_) => {
                    let (s, cut) =
                        octa::db::pushdown::lookups::split_out(c, &src, &key, &deps, cap, stop)?;
                    LookupFetch::Split(Box::new(s), cut)
                }
            })
        },
    ));
}

pub(crate) fn render_lookups_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.lookups_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut rescan = false;
    let mut cancel = false;
    let mut act: Option<LookupAct> = None;
    let mut run_local = false;
    let mut fetched: Option<LookupFetch> = None;
    let mut st = app.lookups_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;
    let local_running = st.running.load(Ordering::Relaxed);

    // Seeds every dependent as ticked (Show breaking rows / Split out start
    // from "use everything found").
    let seed = |st: &mut LookupsState, found: Vec<octa::data::lookups::LookupFinding>| {
        st.ticked = found
            .iter()
            .map(|f| (0..f.dependents.len()).collect())
            .collect();
        st.findings = Some(found);
    };
    match st.server.as_ref().map(|s| s.poll()) {
        Some(TaskPoll::Ready((found, total))) => {
            st.server = None;
            st.total = Some(total);
            seed(&mut st, found);
        }
        Some(TaskPoll::Cancelled) => {
            st.server = None;
            st.note = None;
            st.server_error = Some(octa::i18n::t("pushdown.cancelled"));
        }
        Some(TaskPoll::Failed(e)) => {
            st.server = None;
            st.note = None;
            st.server_error = Some(e);
        }
        Some(TaskPoll::Pending) | None => {}
    }
    match st.fetch.as_ref().map(|s| s.poll()) {
        Some(TaskPoll::Ready(done)) => {
            st.fetch = None;
            fetched = Some(done);
        }
        Some(TaskPoll::Cancelled) => {
            st.fetch = None;
            st.server_error = Some(octa::i18n::t("pushdown.cancelled"));
        }
        Some(TaskPoll::Failed(e)) => {
            st.fetch = None;
            st.server_error = Some(e);
        }
        Some(TaskPoll::Pending) | None => {}
    }
    let mut is_running = local_running || st.server.is_some() || st.fetch.is_some();

    // Drain a finished local scan into `st.findings`.
    if !local_running && let Some(found) = st.result.lock().ok().and_then(|mut s| s.take()) {
        seed(&mut st, found);
    }

    // Column names + the partial-source note are read off `app` before the
    // window closure, matching `referential.rs` / `distribution_compare.rs`:
    // the closure below needs no borrow of `app`, so `act` can drive
    // `app.open_lookup_*` afterwards without fighting the borrow checker.
    let Some(source) = app.tabs.get(st.tab) else {
        app.lookups_dialog = None;
        return;
    };
    let col_names: Vec<String> = source
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let partial_note = source.partial_note();
    let source_rows = source.table.rows.len();
    // A server result whose note is showing; the actions then ask first.
    let from_server = st.note.is_some() && st.findings.is_some();

    let dialog_id = egui::Id::new("octa_lookups_dialog");
    let window = egui::Window::new("octa_lookups")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .default_height(480.0)
            .min_width(400.0)
            .min_height(260.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("lookups_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("lookups.title"))
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

        egui::Panel::top("lookups_controls")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(octa::i18n::t("lookups.intro"))
                        .size(10.0)
                        .color(ui.visuals().weak_text_color()),
                );
                ui.add_space(4.0);
                control_row(ui, |ui| {
                    ui.label(octa::i18n::t("lookups.min_label"));
                    ui.add(
                        egui::DragValue::new(&mut st.min_consistency_pct)
                            .range(80.0..=100.0)
                            .suffix("%"),
                    )
                    .on_hover_text(octa::i18n::t("lookups.min_hint"));
                    if ui
                        .add_enabled(
                            !is_running,
                            egui::Button::new(octa::i18n::t("lookups.rescan")),
                        )
                        .on_hover_text(octa::i18n::t("lookups.rescan_hint"))
                        .on_disabled_hover_text(octa::i18n::t("lookups.running"))
                        .clicked()
                    {
                        rescan = true;
                    }
                    if is_running {
                        ui.add(egui::Spinner::new());
                        ui.label(octa::i18n::t("lookups.running"));
                        if ui
                            .button(octa::i18n::t("lookups.cancel"))
                            .on_hover_text(octa::i18n::t("lookups.cancel_hint"))
                            .clicked()
                        {
                            cancel = true;
                        }
                    }
                });
                // `running: false`: the row above already shows the spinner
                // and Cancel.
                let offer_local = st.findings.is_none();
                if let ServerUi::RunLocal = crate::app::pushdown::server_status_ui(
                    ui,
                    false,
                    st.server_error.as_deref(),
                    offer_local,
                ) {
                    run_local = true;
                }
                if let Some(note) = &st.note {
                    if from_server {
                        crate::app::pushdown::dialog_note_ui(ui, note);
                    }
                } else if let Some((loaded, known_total)) = partial_note {
                    octa::ui::message::partial_note(ui, loaded, known_total);
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            let Some(findings) = &st.findings else {
                if is_running {
                    ui.label(
                        RichText::new(octa::i18n::t("lookups.running"))
                            .color(ui.visuals().weak_text_color()),
                    );
                }
                return;
            };
            // One question at a time: a second click must not stack prompts.
            let busy = if st.prompt.is_some() {
                Some(octa::i18n::t("pushdown.fetch_title"))
            } else {
                st.fetch
                    .is_some()
                    .then(|| octa::i18n::t("pushdown.running"))
            };
            if findings.is_empty() {
                ui.label(
                    RichText::new(octa::i18n::t("lookups.none"))
                        .color(ui.visuals().weak_text_color()),
                );
                return;
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (i, finding) in findings.iter().enumerate() {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            let key_name = col_names
                                .get(finding.key)
                                .map(String::as_str)
                                .unwrap_or("?");
                            ui.label(
                                RichText::new(
                                    octa::i18n::t("lookups.finding")
                                        .replace("{key}", key_name)
                                        .replace("{count}", &finding.dependents.len().to_string()),
                                )
                                .strong(),
                            );
                            for (di, dep) in finding.dependents.iter().enumerate() {
                                let dep_name =
                                    col_names.get(dep.col).map(String::as_str).unwrap_or("?");
                                let mut on = st.ticked[i].contains(&di);
                                let label = octa::i18n::t("lookups.dependent")
                                    .replace("{name}", dep_name)
                                    .replace("{pct}", &format!("{:.0}", dep.consistency * 100.0))
                                    .replace("{rows}", &dep.breaking_rows.to_string());
                                if ui
                                    .checkbox(&mut on, label)
                                    .on_hover_text(octa::i18n::t("lookups.dependent_hint"))
                                    .changed()
                                {
                                    if on {
                                        st.ticked[i].insert(di);
                                    } else {
                                        st.ticked[i].remove(&di);
                                    }
                                }
                            }
                            let any_breaking = st.ticked[i]
                                .iter()
                                .any(|&di| finding.dependents[di].breaking_rows > 0);
                            let any_ticked = !st.ticked[i].is_empty();
                            control_row(ui, |ui| {
                                let why =
                                    |own: &str| busy.clone().unwrap_or_else(|| octa::i18n::t(own));
                                if ui
                                    .add_enabled(
                                        any_breaking && busy.is_none(),
                                        egui::Button::new(octa::i18n::t("lookups.show_breaking")),
                                    )
                                    .on_hover_text(octa::i18n::t("lookups.show_breaking_hint"))
                                    .on_disabled_hover_text(why("lookups.show_breaking_disabled"))
                                    .clicked()
                                {
                                    act = Some(LookupAct::Breaking(i));
                                }
                                if ui
                                    .add_enabled(
                                        any_ticked && busy.is_none(),
                                        egui::Button::new(octa::i18n::t("lookups.split")),
                                    )
                                    .on_hover_text(octa::i18n::t("lookups.split_hint"))
                                    .on_disabled_hover_text(why("lookups.split_disabled"))
                                    .clicked()
                                {
                                    act = Some(LookupAct::Split(i));
                                }
                            });
                        });
                        ui.add_space(6.0);
                    }
                });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    // The prompt, its own small window so it sits above the dialog.
    let mut answer: Option<bool> = None; // Some(true) = fetch, Some(false) = loaded rows
    let mut dismiss = false;
    if st.prompt.is_some() {
        let loaded = source_rows;
        let total = st.total.unwrap_or(loaded);
        egui::Window::new("octa_lookups_fetch_prompt")
            .id(egui::Id::new("octa_lookups_fetch_prompt"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Foreground)
            .default_pos(center_on_first_show(ctx, egui::vec2(420.0, 200.0)))
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(octa::i18n::t("pushdown.fetch_title"))
                        .strong()
                        .size(16.0),
                );
                ui.add_space(6.0);
                for line in crate::app::pushdown::fetch_prompt_lines(loaded, total) {
                    ui.label(line);
                }
                ui.add_space(8.0);
                control_row(ui, |ui| {
                    let n = octa::ui::status_bar::format_number(loaded);
                    if ui
                        .button(octa::i18n::t("pushdown.use_loaded"))
                        .on_hover_text(
                            octa::i18n::t("pushdown.use_loaded_hint").replace("{loaded}", &n),
                        )
                        .clicked()
                    {
                        answer = Some(false);
                    }
                    if ui
                        .button(octa::i18n::t("pushdown.fetch_server"))
                        .on_hover_text(octa::i18n::t("pushdown.fetch_server_hint"))
                        .clicked()
                    {
                        answer = Some(true);
                    }
                    if ui
                        .button(octa::i18n::t("common.cancel"))
                        .on_hover_text(octa::i18n::t("pushdown.prompt_cancel_hint"))
                        .clicked()
                    {
                        dismiss = true;
                    }
                });
            });
    }

    if cancel {
        st.cancel.store(true, Ordering::Relaxed);
        // Say so at once: Oracle cannot cancel one running statement, so the
        // worker may take a while to stop. Dropping a task cancels it.
        if st.server.take().is_some() {
            st.note = None;
            st.server_error = Some(octa::i18n::t("pushdown.cancelled"));
        }
        if st.fetch.take().is_some() {
            st.server_error = Some(octa::i18n::t("pushdown.cancelled"));
        }
    }
    if rescan {
        start_scan(app, &mut st);
        is_running = true;
    }
    if run_local {
        start_scan_on(app, &mut st, false);
        is_running = true;
    }
    // Keep repainting while the worker runs so the spinner animates and the
    // result is picked up promptly.
    if is_running {
        ctx.request_repaint();
    }

    if let Some(a) = act {
        match route(a, from_server) {
            Route::Ask(a) => st.prompt = Some(a),
            Route::Now(LookupAct::Breaking(i)) => app.open_lookup_breaking_tab(&st, i),
            Route::Now(LookupAct::Split(i)) => app.open_lookup_split_tabs(&st, i),
        }
    }
    if dismiss {
        st.prompt = None;
    }
    match (answer, st.prompt.take()) {
        (Some(true), Some(a)) => start_fetch(app, &mut st, a),
        (Some(false), Some(LookupAct::Breaking(i))) => app.open_lookup_breaking_tab(&st, i),
        (Some(false), Some(LookupAct::Split(i))) => app.open_lookup_split_tabs(&st, i),
        (None, Some(a)) => st.prompt = Some(a),
        _ => {}
    }
    if let Some(done) = fetched {
        let key = st.fetch_key.clone();
        let cut = match done {
            LookupFetch::Breaking(t, cut) => {
                app.push_lookup_breaking_tab(*t, &key);
                cut
            }
            LookupFetch::Split(s, cut) => {
                app.push_lookup_split_tabs(*s, &key);
                cut
            }
        };
        if cut {
            let n = octa::ui::status_bar::format_number(octa::db::pushdown::lookups::fetch_cap(
                octa::formats::initial_load_rows(),
            ));
            app.status_message = Some((
                octa::i18n::t("pushdown.fetch_capped").replace("{n}", &n),
                std::time::Instant::now(),
            ));
        }
    }

    if close {
        st.cancel.store(true, Ordering::Relaxed);
        app.lookups_dialog = None;
        return;
    }
    app.lookups_dialog = Some(st);
}

#[cfg(test)]
mod tests {
    use crate::app::state::LookupAct;

    /// After a server result the actions ask first; otherwise they act.
    #[test]
    fn server_results_ask_before_acting() {
        assert_eq!(
            super::route(LookupAct::Split(2), true),
            super::Route::Ask(LookupAct::Split(2))
        );
        assert_eq!(
            super::route(LookupAct::Breaking(0), false),
            super::Route::Now(LookupAct::Breaking(0))
        );
    }
}
