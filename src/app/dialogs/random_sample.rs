//! "Random sample" dialog. Asks for a row count, then opens a detached tab of
//! that many randomly chosen rows from the active table. On a partial database
//! tab the database picks them from the whole table (Exact, or Fast block
//! sampling where the engine has it). Driven by `OctaApp.random_sample_dialog`.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

use super::super::state::OctaApp;

/// Where a database sample came from: a Fast one that it is approximate,
/// one asked for as Fast that ran exactly (`fell_back`) why, and a count cut
/// to the row cap.
fn server_sample_note(
    asked: usize,
    capped: bool,
    ran: octa::db::pushdown::sample::SampleMethod,
    fell_back: bool,
    filtered: bool,
) -> String {
    let fmt = octa::ui::status_bar::format_number;
    let mut note = match (ran, filtered) {
        (octa::db::pushdown::sample::SampleMethod::Fast, _) => {
            octa::i18n::t("dbview.sample_note_fast").replace("{n}", &fmt(asked))
        }
        (_, true) => octa::i18n::t("dbview.sample_note_filtered"),
        (_, false) => octa::i18n::t("dbview.sample_note"),
    };
    if fell_back {
        note.push(' ');
        note.push_str(&octa::i18n::t("dbview.sample_fast_fell_back"));
    }
    if capped {
        note.push(' ');
        note.push_str(
            &octa::i18n::t("dbview.sample_capped")
                .replace("{asked}", &fmt(asked))
                .replace("{cap}", &fmt(octa::formats::initial_load_rows())),
        );
    }
    note
}

/// The rows asked for: comma-, dot- and space-tolerant, at least 1, 100 when
/// unreadable.
fn parse_n(buf: &str) -> usize {
    buf.trim()
        .replace([',', '.', ' '], "")
        .parse::<usize>()
        .unwrap_or(100)
        .max(1)
}

pub(crate) fn render_random_sample_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.random_sample_dialog.is_none() {
        return;
    }
    let mut state = app.random_sample_dialog.take().unwrap();
    let mut close = false;
    let mut apply = false;

    use octa::db::pushdown::sample::{SampleMethod, fast_available};
    // On a partial database tab the rows come from the whole table on the
    // server (or from what its database filter keeps).
    let server_src = {
        let tab = &app.tabs[app.active_tab];
        crate::app::db_view::view_source_for(
            tab,
            app.settings.db_pushdown,
            &app.settings.db_connections,
        )
        .map(|mut src| {
            src.filter = tab
                .server_view
                .as_ref()
                .and_then(|v| v.where_sql(src.engine()));
            src
        })
    };
    let filtered = server_src.as_ref().is_some_and(|s| s.filter.is_some());
    let engine_has_fast = server_src
        .as_ref()
        .is_some_and(|s| fast_available(s.engine()));
    let fast_ok = engine_has_fast && !filtered;
    if !fast_ok {
        state.fast = false;
    }
    let loaded = app.tabs[app.active_tab].table.row_count();
    let mut run_local = false;
    let mut cancel_server = false;
    // A finished server sample opens its tab and closes the dialog.
    if let Some(task) = &state.server {
        match task.poll() {
            crate::app::pushdown::TaskPoll::Pending => ctx.request_repaint(),
            crate::app::pushdown::TaskPoll::Ready((table, capped, ran)) => {
                let (asked, fast, filtered, label) = state.server_asked.take().unwrap_or_default();
                let fell_back = fast && ran == SampleMethod::Exact;
                let note = server_sample_note(asked, capped, ran, fell_back, filtered);
                app.open_server_sample_tab(table, &label, note);
                return;
            }
            crate::app::pushdown::TaskPoll::Cancelled => {
                state.server = None;
                state.server_error = Some(octa::i18n::t("pushdown.cancelled"));
            }
            crate::app::pushdown::TaskPoll::Failed(e) => {
                state.server = None;
                state.server_error = Some(e);
            }
        }
    }
    let running = state.server.is_some();

    let dialog_id = egui::Id::new("octa_random_sample_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(state.size));
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(340.0, 320.0));
    let window = egui::Window::new("octa_random_sample")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true).default_width(340.0).default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("random_sample_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("sample.title"))
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

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("sample.hint"))
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            octa::ui::control_row::control_row(ui, |ui| {
                ui.label(octa::i18n::t("sample.rows"));
                let resp = octa::ui::control_row::control_text_edit(
                    ui,
                    100.0,
                    egui::TextEdit::singleline(&mut state.n_buf).hint_text("100"),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
            });
            if let Some(src) = &server_src {
                octa::ui::control_row::control_row(ui, |ui| {
                    ui.label(octa::i18n::t("dbview.sample_how"));
                    ui.radio_value(&mut state.fast, false, octa::i18n::t("dbview.sample_exact"))
                        .on_hover_text(octa::i18n::t("dbview.sample_exact_hint"));
                    let fast_reason = if !engine_has_fast {
                        octa::i18n::t("dbview.sample_fast_no_engine_hint")
                            .replace("{engine}", src.engine().label())
                    } else {
                        octa::i18n::t("dbview.sample_fast_filtered_hint")
                    };
                    ui.add_enabled_ui(fast_ok, |ui| {
                        ui.radio_value(&mut state.fast, true, octa::i18n::t("dbview.sample_fast"))
                            .on_hover_text(octa::i18n::t("dbview.sample_fast_hint"))
                            .on_disabled_hover_text(&fast_reason);
                    });
                });
            }
            if running {
                octa::ui::control_row::control_row(ui, |ui| {
                    ui.spinner();
                    ui.label(octa::i18n::t("dbview.sample_running"));
                    if ui
                        .button(octa::i18n::t("common.cancel"))
                        .on_hover_text(octa::i18n::t("pushdown.cancel_hint"))
                        .clicked()
                    {
                        cancel_server = true;
                    }
                });
            } else if let Some(e) = &state.server_error {
                octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
                if ui
                    .button(octa::i18n::t("dbview.use_loaded"))
                    .on_hover_text(
                        octa::i18n::t("dbview.sample_loaded_hint")
                            .replace("{loaded}", &octa::ui::status_bar::format_number(loaded)),
                    )
                    .clicked()
                {
                    run_local = true;
                }
            }

            ui.add_space(8.0);
            octa::ui::control_row::control_row(ui, |ui| {
                if ui
                    .add_enabled(!running, egui::Button::new(octa::i18n::t("sample.create")))
                    .on_disabled_hover_text(octa::i18n::t("dbview.sample_running"))
                    .clicked()
                {
                    apply = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(octa::i18n::t("common.cancel")).clicked() {
                        close = true;
                    }
                });
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if close || apply {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if cancel_server {
        // Dropping the task cancels the statement on the server.
        state.server = None;
        state.server_error = Some(octa::i18n::t("pushdown.cancelled"));
    }
    // Zero and huge values are clamped by the builder against the row count.
    let n = parse_n(&state.n_buf);
    if run_local {
        app.open_random_sample_tab(n);
        return; // state consumed
    }
    if apply && state.server.is_none() {
        match server_src {
            Some(src) => {
                let method = if state.fast {
                    SampleMethod::Fast
                } else {
                    SampleMethod::Exact
                };
                let cap = octa::formats::initial_load_rows();
                state.server_error = None;
                state.server_asked = Some((
                    n,
                    state.fast,
                    filtered,
                    app.tabs[app.active_tab].result_source_label(),
                ));
                state.server = Some(app.spawn_server_task(
                    src.conn.clone(),
                    octa::i18n::t("sample.title"),
                    move |c, stop| octa::db::pushdown::sample::run(c, &src, n, method, cap, stop),
                ));
            }
            None => {
                app.open_random_sample_tab(n);
                return; // state consumed
            }
        }
    }
    // Closing drops a running task, which cancels it.
    if !close {
        state.size = size;
        app.random_sample_dialog = Some(state);
    }
}
