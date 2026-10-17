//! The status message row at the top of the central panel.

use eframe::egui;

use octa::ui;

use crate::app::state::OctaApp;

impl OctaApp {
    /// Status message - auto-fades after `status_message_secs`, the same span
    /// for every message. Confirmations and failures used to differ (10s vs
    /// 60s) so an error could be read and copied; the hover pause below does
    /// that job now without making every other message outstay its welcome.
    pub(super) fn render_status_message(&mut self, ui: &mut egui::Ui) {
        let message_lifetime = self.settings.status_message_duration();
        let mut dismiss_status = false;
        if let Some((ref msg, instant)) = self.status_message
            && instant.elapsed() < message_lifetime
        {
            let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
            let color = if is_success_message(msg) {
                colors.success
            } else if msg.starts_with('\u{1f419}') {
                // Easter-egg messages (kraken, etc.) get the accent.
                colors.accent
            } else {
                colors.error
            };
            // Selectable + right-click Copy: a failure message here is
            // often the whole reason a save or a connection did not work.
            let msg = msg.clone();
            let row = ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui::message::selectable_message_sized(ui, color, &msg, Some(12.0));
                // Dismiss now, for anyone who has read it and wants the
                // room back before the timer runs out.
                if ui
                    .small_button("x")
                    .on_hover_text(octa::i18n::t("status_bar.dismiss_hint"))
                    .clicked()
                {
                    dismiss_status = true;
                }
            });
            // Pause the countdown while the pointer is over the message.
            // Implemented by pushing the stored start forward by one
            // frame rather than by tracking paused time separately, so
            // `elapsed()` simply stops growing and the ~30 places that
            // set `status_message` keep working unchanged.
            // `contains_pointer`, not `hovered`: the message itself is a
            // selectable (interactive) label, so it takes the hover from
            // the row that wraps it and the pause never fired.
            if row.response.contains_pointer()
                && let Some((_, ref mut started)) = self.status_message
            {
                let dt = ui.ctx().input(|i| i.stable_dt).min(0.25);
                if let Some(pushed) = started.checked_add(std::time::Duration::from_secs_f32(dt)) {
                    *started = pushed;
                }
                // Keep animating while hovered so the pause is honoured
                // even when nothing else asks for a repaint.
                ui.ctx().request_repaint();
            } else {
                // egui only paints when something asks it to, so a message
                // whose time is up stays on screen until the next unrelated
                // event repaints the frame - which read as "the timeout in
                // Settings does nothing". Book the frame that removes it.
                ui.ctx()
                    .request_repaint_after(message_lifetime.saturating_sub(instant.elapsed()));
            }
            ui.add_space(4.0);
        }
        if dismiss_status {
            self.status_message = None;
        }
    }
}

/// A confirmation, as opposed to a failure. Only the prefix Octa itself
/// writes is recognised; anything else is treated as a failure, which is the
/// safe way round (a failure shown for too long beats one that vanishes).
fn is_success_message(msg: &str) -> bool {
    msg.starts_with("Saved")
}

#[cfg(test)]
#[path = "status_message_tests.rs"]
mod status_message_tests;
