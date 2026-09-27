//! Find lookup tables dialog (**Analyse -> Find lookup tables...**). Scans
//! the active table on a worker for columns that always follow another
//! column - `customer_id -> customer_name, city` - so a flat export can be
//! split back into a lookup table and a slim main table.
//!
//! Modelled on `find_fuzzy_duplicates.rs`: a background worker with polled
//! `Arc<Mutex<Option<..>>>` slots, `running`/`cancel` `AtomicBool`s, and
//! `FlagOnDrop` so a panicked worker never wedges the spinner on. The scan
//! itself is the pure [`octa::data::lookups::find_lookups`].

use std::sync::atomic::Ordering;

use eframe::egui;
use egui::RichText;

use octa::ui::control_row::control_row;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{LookupsState, OctaApp};

/// Spawn the scan worker on a clone of the tab's table (edits applied).
/// Called right after the state is built (toolbar handler + shortcut
/// dispatch, and **Scan again**), so the dialog opens already scanning.
pub(crate) fn start_scan(app: &OctaApp, st: &mut LookupsState) {
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

/// Deferred action from a finding's footer buttons, applied after the window
/// closure so the borrow on `app` used to build it has ended.
enum Act {
    Breaking(usize),
    Split(usize),
}

pub(crate) fn render_lookups_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.lookups_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut rescan = false;
    let mut cancel = false;
    let mut act: Option<Act> = None;
    let mut st = app.lookups_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;
    let mut is_running = st.running.load(Ordering::Relaxed);

    // Drain a finished scan into `st.findings`, seeding every dependent as
    // ticked (Show breaking rows / Split out start from "use everything
    // found").
    if !is_running && let Some(found) = st.result.lock().ok().and_then(|mut s| s.take()) {
        st.ticked = found
            .iter()
            .map(|f| (0..f.dependents.len()).collect())
            .collect();
        st.findings = Some(found);
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
    let partial_note = source.table.partial_note();

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
                if let Some((loaded, known_total)) = partial_note {
                    octa::ui::message::partial_note(ui, loaded, known_total);
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            let Some(findings) = &st.findings else {
                ui.label(
                    RichText::new(octa::i18n::t("lookups.running"))
                        .color(ui.visuals().weak_text_color()),
                );
                return;
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
                                if ui
                                    .add_enabled(
                                        any_breaking,
                                        egui::Button::new(octa::i18n::t("lookups.show_breaking")),
                                    )
                                    .on_hover_text(octa::i18n::t("lookups.show_breaking_hint"))
                                    .on_disabled_hover_text(octa::i18n::t(
                                        "lookups.show_breaking_disabled",
                                    ))
                                    .clicked()
                                {
                                    act = Some(Act::Breaking(i));
                                }
                                if ui
                                    .add_enabled(
                                        any_ticked,
                                        egui::Button::new(octa::i18n::t("lookups.split")),
                                    )
                                    .on_hover_text(octa::i18n::t("lookups.split_hint"))
                                    .on_disabled_hover_text(octa::i18n::t("lookups.split_disabled"))
                                    .clicked()
                                {
                                    act = Some(Act::Split(i));
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

    if cancel {
        st.cancel.store(true, Ordering::Relaxed);
    }
    if rescan {
        start_scan(app, &mut st);
        is_running = true;
    }
    // Keep repainting while the worker runs so the spinner animates and the
    // result is picked up promptly.
    if is_running {
        ctx.request_repaint();
    }

    match act {
        Some(Act::Breaking(i)) => app.open_lookup_breaking_tab(&st, i),
        Some(Act::Split(i)) => app.open_lookup_split_tabs(&st, i),
        None => {}
    }

    if close {
        st.cancel.store(true, Ordering::Relaxed);
        app.lookups_dialog = None;
        return;
    }
    app.lookups_dialog = Some(st);
}
