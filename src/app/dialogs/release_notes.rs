//! Release-notes window: what the version you are running brought.
//!
//! The notes are baked into the binary from `release_notes.md`, the same file
//! CI hands to `gh release create --notes-file`, so what this window shows and
//! what the GitHub release page shows cannot disagree. Nothing here fetches:
//! the window is a fact about the build, not about what GitHub has published,
//! so it works offline, behind a firewall, and on a Microsoft Store copy.
//!
//! Raised at startup by [`super::super::init`] once per version, independent of
//! the update check - turning "check for updates at start" off does not silence
//! it. The manual Help -> Check for Updates path has its own dialog.
//!
//! The notes are Markdown and render through the same `render_pulldown` the
//! Markdown view and the in-app documentation use.

use eframe::egui;

use crate::view_modes::markdown::render_pulldown;

use super::super::state::OctaApp;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The release notes, baked in at compile time.
const NOTES: &str = include_str!("../../../release_notes.md");

/// Whether the window should open on this launch.
///
/// `last_seen` is the version whose notes were dismissed with "Do not show
/// these notes again", so a later release opens the window again. A dev build
/// stays silent: `release_notes.md` describes the last real release, not
/// whatever is in the working tree.
pub(crate) fn should_show(show_setting: bool, last_seen: &str, version: &str) -> bool {
    show_setting && last_seen != version && version != "0.0.0-dev"
}

pub(crate) fn render_release_notes_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.pending_release_notes {
        return;
    }
    let mut close = false;
    // Checked means "I have read these", recorded against this version only.
    //
    // Derived from the setting rather than kept in a local: this function runs
    // once per frame, so a plain `let mut hide = false` un-ticked the box on
    // the very next frame. Ticking it writes straight away, which also means
    // closing the window with the `x` cannot lose the answer.
    let mut hide = app.settings.last_release_notes_version == VERSION;

    // Centred on first show via `default_pos`, NOT `anchor`: an anchored egui
    // window is pinned to that spot and ignores title-bar drags, so the notes
    // could be resized but never moved out of the way. `default_pos` only
    // places it the first time; egui remembers where the user drags it after.
    let size = egui::vec2(600.0, 460.0);
    let default_pos = ctx.viewport_rect().center() - size * 0.5;

    let title = octa::i18n::t("release.whats_new").replace("{version}", VERSION);

    egui::Window::new(title)
        .collapsible(false)
        .resizable(true)
        .default_width(size.x)
        .default_height(size.y)
        .min_width(380.0)
        .min_height(240.0)
        .default_pos(default_pos)
        .show(ctx, |ui| {
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
                            app.settings.last_release_notes_version = if hide {
                                VERSION.to_string()
                            } else {
                                String::new()
                            };
                            app.settings.save();
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(octa::i18n::t("common.close")).clicked() {
                                close = true;
                            }
                        });
                    });
                });

            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("release_notes_body")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if NOTES.trim().is_empty() {
                            ui.weak(octa::i18n::t("release.no_notes"));
                        } else {
                            render_pulldown(ui, NOTES, None);
                        }
                    });
            });
        });

    if close {
        // The tick already recorded itself. Closing without it means "not
        // now", so the window comes back next start.
        app.pending_release_notes = false;
    }
}

#[cfg(test)]
mod tests {
    use super::should_show;

    #[test]
    fn a_fresh_install_sees_its_notes() {
        assert!(should_show(true, "", "0.17.1"));
    }

    #[test]
    fn an_acknowledged_version_stays_silent() {
        assert!(!should_show(true, "0.17.1", "0.17.1"));
    }

    #[test]
    fn the_next_release_opens_the_window_again() {
        assert!(should_show(true, "0.17.1", "0.17.2"));
    }

    #[test]
    fn the_setting_wins_over_everything() {
        assert!(!should_show(false, "", "0.17.1"));
    }

    #[test]
    fn a_dev_build_stays_silent() {
        assert!(!should_show(true, "", "0.0.0-dev"));
    }
}
