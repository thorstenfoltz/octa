//! "Auto-refresh" dialog: read a database or API tab again at an interval.
//! Driven by `OctaApp.auto_refresh_dialog`; the reading itself is
//! `OctaApp::drive_live_tabs` (`src/app/live.rs`).

use std::time::{Duration, Instant};

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

use super::super::live::{AutoRefresh, check_interval, is_billed};
use super::super::state::OctaApp;

/// State of the open dialog.
pub(crate) struct AutoRefreshDialog {
    pub tab: usize,
    pub buf: String,
    pub size: DialogSize,
}

pub(crate) fn render_auto_refresh_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut d) = app.auto_refresh_dialog.take() else {
        return;
    };
    if d.tab >= app.tabs.len() {
        return;
    }
    let t = octa::i18n::t;
    let engine = app.tab_db_engine(d.tab);
    let billed = engine.is_some_and(is_billed);
    let warn = engine.filter(|e| billed && !app.billed_warned.contains(e));
    let running = app.tabs[d.tab].auto_refresh.is_some();
    let verdict = d.buf.trim().parse::<u64>().map_err(|_| None).and_then(|n| {
        check_interval(
            n,
            billed,
            app.settings.auto_refresh_min_secs,
            app.settings.auto_refresh_min_billed_secs,
        )
        .map_err(Some)
    });
    let (mut close, mut start, mut stop) = (false, false, false);

    let dialog_id = egui::Id::new("octa_auto_refresh_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|x| x.get_temp::<DialogSize>(size_key).unwrap_or(d.size));
    let minimized = size == DialogSize::Minimized;
    let center = center_on_first_show(ctx, egui::vec2(420.0, 220.0));
    let window = egui::Window::new("octa_auto_refresh")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true).default_width(420.0).default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "auto_refresh_header",
            RichText::new(t("live.auto_refresh").trim_end_matches("...").to_string()),
            &mut size,
        ) {
            close = true;
        }
        if minimized {
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            octa::ui::control_row::control_line(ui, |ui| {
                ui.label(t("live.interval"))
                    .on_hover_text(t("live.interval_hint"));
                ui.add(egui::TextEdit::singleline(&mut d.buf).desired_width(80.0))
                    .on_hover_text(t("live.interval_hint"));
            });
            if let Err(Some(min)) = verdict {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    t("live.interval_too_short").replace("{secs}", &min.to_string()),
                );
            }
            if let Some(e) = warn {
                ui.add_space(4.0);
                ui.label(t("live.billed_warning").replace("{engine}", e.label()));
            }
            ui.add_space(8.0);
            octa::ui::control_row::control_line(ui, |ui| {
                let why = match verdict {
                    Ok(_) => String::new(),
                    Err(Some(min)) => {
                        t("live.interval_too_short").replace("{secs}", &min.to_string())
                    }
                    Err(None) => t("live.interval_hint"),
                };
                if ui
                    .add_enabled(verdict.is_ok(), egui::Button::new(t("live.start")))
                    .on_hover_text(t("live.start_hint"))
                    .on_disabled_hover_text(why)
                    .clicked()
                {
                    start = true;
                }
                if ui
                    .add_enabled(running, egui::Button::new(t("live.stop")))
                    .on_hover_text(t("live.stop_hint"))
                    .on_disabled_hover_text(t("live.auto_refresh_pill_hint"))
                    .clicked()
                {
                    stop = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t("common.cancel")).clicked() {
                        close = true;
                    }
                });
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    let done = close || start || stop;
    ctx.data_mut(|x| x.insert_temp(size_key, if done { DialogSize::Normal } else { size }));

    if start && let Ok(secs) = verdict {
        let every = Duration::from_secs(secs);
        app.tabs[d.tab].auto_refresh = Some(AutoRefresh {
            every,
            next_at: Instant::now() + every,
            before: None,
            return_to: None,
        });
        if let Some(e) = engine {
            app.billed_warned.insert(e);
        }
    }
    if stop {
        app.tabs[d.tab].auto_refresh = None;
    }
    if !done {
        d.size = size;
        app.auto_refresh_dialog = Some(d);
    }
}
