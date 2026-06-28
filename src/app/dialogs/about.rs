//! Simple "About Octa" dialog showing version, author, and repo link.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const EMAIL: &str = "thorsten.foltz@live.com";

/// Strip the `<email>` suffix Cargo embeds in `CARGO_PKG_AUTHORS` so the
/// dialog shows just the name. The email is rendered separately as a
/// `mailto:` link below.
fn author_names(raw: &str) -> String {
    raw.split(',')
        .map(|entry| {
            let trimmed = entry.trim();
            match trimmed.find('<') {
                Some(idx) => trimmed[..idx].trim().to_string(),
                None => trimmed.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Open `mailto:<addr>` via the platform's default URL handler. egui's
/// built-in `hyperlink_to` routes through the `webbrowser` crate, which on
/// some platforms ignores the `mailto:` scheme and falls back to opening
/// the address as a web URL - so we shell out to the OS handler directly.
fn open_mailto(email: &str) {
    let url = format!("mailto:{}", email);
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", &url])
        .spawn();
}

/// Easter egg: clicking the "Octa" title eight times (one per tentacle)
/// reveals a hidden line. Counter is kept in egui's transient memory store
/// keyed by this id, so it survives frames but resets on app restart.
const TENTACLE_CLICK_ID: &str = "about_dialog_tentacle_clicks";

pub(crate) fn render_about_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_about_dialog {
        return;
    }
    let screen_center = ctx.content_rect().center();
    let default_pos = screen_center - egui::vec2(160.0, 100.0);
    let mut close = false;

    let dialog_id = egui::Id::new("octa_about_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;

    let window = egui::Window::new("octa_about")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false)
        .default_pos(default_pos);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| w.resizable(false));

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("about_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.about_title"))
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

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                let title_id = egui::Id::new(TENTACLE_CLICK_ID);
                let mut clicks: u8 = ui.data(|d| d.get_temp::<u8>(title_id).unwrap_or(0));
                let title = ui
                    .add(
                        egui::Label::new(RichText::new("Octa").strong().size(20.0))
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if title.clicked() {
                    clicks = clicks.saturating_add(1);
                    ui.data_mut(|d| d.insert_temp(title_id, clicks));
                }
                ui.add_space(4.0);
                ui.label(format!(
                    "{} {}",
                    octa::i18n::t("dialog.about_version"),
                    VERSION
                ));
                ui.add_space(8.0);
                ui.label(format!(
                    "{} {}",
                    octa::i18n::t("dialog.about_author"),
                    author_names(AUTHORS)
                ));
                ui.add_space(4.0);
                let email_link = ui
                    .add(egui::Link::new(EMAIL))
                    .on_hover_text(format!("mailto:{}", EMAIL));
                if email_link.clicked() {
                    open_mailto(EMAIL);
                }
                ui.add_space(4.0);
                if ui
                    .hyperlink_to(octa::i18n::t("dialog.about_repo"), REPOSITORY)
                    .clicked()
                {
                    // egui opens the link automatically
                }
                if clicks >= 8 {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "\u{1f419} Eight tentacles, eight clicks. \
                             You found the kraken's lair.",
                        )
                        .italics()
                        .size(11.0),
                    );
                }
                ui.add_space(12.0);
                if ui.button(octa::i18n::t("common.close")).clicked() {
                    app.show_about_dialog = false;
                    ui.data_mut(|d| d.remove::<u8>(title_id));
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
            if close || !app.show_about_dialog {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if close {
        app.show_about_dialog = false;
        ctx.data_mut(|d| d.remove::<u8>(egui::Id::new(TENTACLE_CLICK_ID)));
    }
}
