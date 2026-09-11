//! "Open URL": read a file straight from a web address.
//!
//! The download runs on a worker thread and the result is polled per frame,
//! because a slow or unreachable host must not freeze the window.
//!
//! Redirects are the reason this dialog is more than a text box. A link can
//! bounce you to a different address than the one you typed, and the file you
//! get is the one at the end of that chain. `fetch_http_to_temp` follows the
//! chain itself and reports where it ended up, so when the destination differs
//! the user is asked before the file is opened. That confirmation is the only
//! place the change is visible, which is why it is on by default and why
//! turning it off in Settings makes you read what you are giving up.

use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::RichText;

use octa::cloud::{FetchOutcome, UrlTrust, fetch_http_to_temp, is_http_url};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

use super::super::state::{OctaApp, OpenUrlState};

/// Result of one download attempt, handed back from the worker.
pub(crate) type UrlSlot = Arc<Mutex<Option<Result<FetchOutcome, String>>>>;

impl OctaApp {
    pub(crate) fn open_url_dialog(&mut self) {
        self.open_url_dialog = Some(OpenUrlState {
            url: String::new(),
            error: None,
            running: false,
            slot: Arc::new(Mutex::new(None)),
            pending_redirect: None,
        });
    }

    /// Start the download on a worker thread.
    fn start_url_fetch(&mut self, ctx: &egui::Context) {
        let Some(state) = self.open_url_dialog.as_mut() else {
            return;
        };
        let url = state.url.trim().to_string();
        if !is_http_url(&url) {
            state.error = Some(t("url.needs_scheme"));
            return;
        }
        state.error = None;
        state.running = true;
        let slot = state.slot.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            // The address came from the person at the keyboard, so it is not
            // confined to public hosts: reading from a local server is a
            // normal thing to want.
            let outcome =
                fetch_http_to_temp(&url, UrlTrust::UserSupplied).map_err(|e| format!("{e:#}"));
            if let Ok(mut s) = slot.lock() {
                *s = Some(outcome);
            }
            ctx.request_repaint();
        });
    }

    /// Pick up a finished download; called once per frame from the update loop.
    pub(crate) fn drain_open_url(&mut self, ctx: &egui::Context) {
        let Some(state) = self.open_url_dialog.as_mut() else {
            return;
        };
        if !state.running {
            return;
        }
        let outcome = match state.slot.lock() {
            Ok(mut s) => s.take(),
            Err(_) => Some(Err("download worker panicked".to_string())),
        };
        let Some(outcome) = outcome else {
            return;
        };
        state.running = false;
        match outcome {
            Err(e) => state.error = Some(e),
            Ok(fetched) => match &fetched.redirected_to {
                // Went somewhere other than the address that was typed, and
                // the user asked to be told.
                Some(_) if self.settings.confirm_url_redirects => {
                    state.pending_redirect = Some(fetched);
                }
                _ => {
                    let path = fetched.path.clone();
                    self.open_url_dialog = None;
                    self.load_file_in_new_tab(path);
                }
            },
        }
        ctx.request_repaint();
    }
}

pub(crate) fn render_open_url_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.open_url_dialog.is_none() {
        return;
    }
    // The redirect question takes over the dialog while it is pending: there
    // is nothing else to decide until it is answered.
    if app
        .open_url_dialog
        .as_ref()
        .is_some_and(|s| s.pending_redirect.is_some())
    {
        render_redirect_confirm(app, ctx);
        return;
    }

    let mut close = false;
    let mut go = false;

    let dialog_id = egui::Id::new("octa_open_url_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(520.0, 320.0));
    let window = egui::Window::new("octa_open_url")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .min_width(420.0)
            .default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("open_url_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("url.title")).strong().size(16.0));
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

        let running = app.open_url_dialog.as_ref().is_some_and(|s| s.running);

        egui::Panel::bottom("open_url_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let has_text = app
                        .open_url_dialog
                        .as_ref()
                        .is_some_and(|s| !s.url.trim().is_empty());
                    let btn =
                        ui.add_enabled(has_text && !running, egui::Button::new(t("url.open")));
                    if btn.on_hover_text(t("url.open_hint")).clicked() {
                        go = true;
                    }
                    if ui.button(t("common.cancel")).clicked() {
                        close = true;
                    }
                    if running {
                        ui.spinner();
                        ui.label(t("url.downloading"));
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            let Some(state) = app.open_url_dialog.as_mut() else {
                return;
            };
            ui.label(t("url.field")).on_hover_text(t("url.field_hint"));
            let resp = ui.add_enabled(
                !state.running,
                egui::TextEdit::singleline(&mut state.url)
                    .hint_text("https://example.org/data.csv")
                    .desired_width(f32::INFINITY),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                go = true;
            }
            if let Some(err) = state.error.clone() {
                ui.add_space(6.0);
                octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, &err);
            }
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| d.insert_temp(size_key, if close { DialogSize::Normal } else { size }));

    if go {
        app.start_url_fetch(ctx);
    }
    if close {
        app.open_url_dialog = None;
    }
}

/// "That address sent you somewhere else. Open it anyway?"
fn render_redirect_confirm(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(state) = app.open_url_dialog.as_ref() else {
        return;
    };
    let Some(fetched) = state.pending_redirect.as_ref() else {
        return;
    };
    let asked = state.url.trim().to_string();
    let landed = fetched.redirected_to.clone().unwrap_or_default();

    let mut open_it = false;
    let mut cancel = false;
    let dialog_id = egui::Id::new("octa_url_redirect_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(540.0, 300.0));
    let window = egui::Window::new("octa_url_redirect")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(540.0)
            .default_height(300.0)
            .min_width(360.0)
            .min_height(180.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("url_redirect_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(t("url.redirect_title"))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            chrome_close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.set_max_width(520.0);
            ui.label(t("url.redirect_body"));
            ui.add_space(6.0);
            // Both addresses selectable: the whole point is to read them, and
            // a long one may need copying out to compare.
            ui.label(RichText::new(t("url.redirect_asked")).strong());
            octa::ui::message::selectable_message(ui, ui.visuals().text_color(), &asked);
            ui.add_space(4.0);
            ui.label(RichText::new(t("url.redirect_landed")).strong());
            octa::ui::message::selectable_message(ui, ui.visuals().warn_fg_color, &landed);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .button(t("url.redirect_open"))
                    .on_hover_text(t("url.redirect_open_hint"))
                    .clicked()
                {
                    open_it = true;
                }
                if ui.button(t("common.cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    });
    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if chrome_close {
                DialogSize::Normal
            } else {
                size
            },
        )
    });
    if chrome_close {
        cancel = true;
    }

    if open_it {
        let path = app
            .open_url_dialog
            .as_mut()
            .and_then(|s| s.pending_redirect.take())
            .map(|f| f.path);
        app.open_url_dialog = None;
        if let Some(path) = path {
            app.load_file_in_new_tab(path);
        }
    } else if cancel {
        // The file is already downloaded to a temp, but it is not opened and
        // the address is not remembered: declining means declining.
        app.open_url_dialog = None;
    }
}
