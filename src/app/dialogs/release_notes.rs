//! Release-notes window: what a release brought.
//!
//! Two cases, one window. Either the release is one the user does not have
//! yet - then it offers to install it - or it is the version they are running,
//! which is what makes an upgrade announce itself instead of the window
//! waiting for the *next* release to exist. The current-version form drops the
//! install button; there is nothing to install.
//!
//! Raised once per version by the startup update check (see
//! `update_loop::drain_startup_update_check`), never by the manual
//! Help -> Check for updates path, which has its own dialog.
//!
//! The notes are the GitHub release body verbatim, so they are Markdown and
//! render through the same `render_pulldown` the Markdown view and the
//! in-app documentation use. Nothing here fetches: the body arrived with the
//! version in the one request the check already made.

use eframe::egui;
use egui::RichText;

use crate::view_modes::markdown::render_pulldown;

use super::super::state::OctaApp;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) fn render_release_notes_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some((version, notes)) = app.pending_release_notes.clone() else {
        return;
    };
    let mut close = false;
    let mut update = false;
    // Checked means "stop showing me these", so it is the inverse of the
    // setting it writes.
    let mut hide = !app.settings.show_release_notes;
    let mut hide_changed = false;
    // A Store (MSIX) copy cannot replace its own binary, so it is told who
    // will do the updating instead of being offered a button that fails.
    let store = octa::platform::is_store_packaged();
    // Notes for the running version - "what your upgrade brought" rather than
    // "what you are missing". Same window, minus the offer to install what is
    // already installed.
    let current = version == VERSION;

    // Centred on first show via `default_pos`, NOT `anchor`: an anchored egui
    // window is pinned to that spot and ignores title-bar drags, so the notes
    // could be resized but never moved out of the way. `default_pos` only
    // places it the first time; egui remembers where the user drags it after.
    let size = egui::vec2(600.0, 460.0);
    let default_pos = ctx.viewport_rect().center() - size * 0.5;

    // The current-version window says everything it needs to in its title, so
    // it drops the "you are running X" line the available-version one carries.
    let title = if current {
        octa::i18n::t("release.whats_new").replace("{version}", &version)
    } else {
        octa::i18n::t("dialog.ud_new_avail")
    };

    egui::Window::new(title)
        .collapsible(false)
        .resizable(true)
        .default_width(size.x)
        .default_height(size.y)
        .min_width(380.0)
        .min_height(240.0)
        .default_pos(default_pos)
        .show(ctx, |ui| {
            if !current {
                ui.label(
                    RichText::new(
                        octa::i18n::t("release.available")
                            .replace("{version}", &version)
                            .replace("{current}", VERSION),
                    )
                    .strong(),
                );
                if store {
                    ui.add_space(4.0);
                    ui.label(octa::i18n::t("release.store"));
                }
                ui.add_space(8.0);
            }

            // Footer first: the notes get whatever height is left, so a long
            // changelog can never push the buttons off the window.
            egui::Panel::bottom("release_notes_footer")
                .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .checkbox(&mut hide, octa::i18n::t("release.dont_show"))
                            .on_hover_text(octa::i18n::t("release.dont_show_hint"))
                            .changed()
                        {
                            hide_changed = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(octa::i18n::t("common.close")).clicked() {
                                close = true;
                            }
                            if !store
                                && !current
                                && ui.button(octa::i18n::t("dialog.ud_update_now")).clicked()
                            {
                                update = true;
                            }
                        });
                    });
                });

            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("release_notes_body")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if notes.is_empty() {
                            ui.weak(octa::i18n::t("release.no_notes"));
                        } else {
                            render_pulldown(ui, &notes, None);
                        }
                    });
            });
        });

    if hide_changed {
        app.settings.show_release_notes = !hide;
    }
    if close || update {
        // Remember the version either way: the user has seen these notes, and
        // starting the update does not mean they want them again next launch.
        app.settings.last_release_notes_version = version.clone();
        app.pending_release_notes = None;
    }
    if hide_changed || close || update {
        app.settings.save();
    }
    if update {
        // Hand over to the regular update dialog, which owns the download,
        // the elevation prompt and the restart notice.
        app.show_update_dialog = true;
        app.perform_update(&version, ctx);
    }
}
