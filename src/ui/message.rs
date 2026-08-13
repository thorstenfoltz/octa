//! Rendering a status or error message the user can actually act on.
//!
//! An error is the one piece of text in the app most worth copying: it goes
//! into a bug report, a search box, or a message to whoever runs the server.
//! Two things get in the way of that by default, and this module handles
//! both in one place so no caller has to remember them.
//!
//! First, the Settings dialog sets `interaction.selectable_labels = false`
//! for its whole body (row captions should not show a text I-beam), which
//! silently takes selection away from the errors inside it too. Second, even
//! where selection works, dragging across a long one-line message is fiddly,
//! so a right-click **Copy** is offered alongside - the same affordance the
//! table and the text views already provide.

use eframe::egui;

/// Draw a selectable message with a right-click **Copy**.
///
/// Selection is re-enabled locally rather than by changing the caller's
/// style, so a Settings row label next to it keeps its plain pointer.
pub fn selectable_message(ui: &mut egui::Ui, colour: egui::Color32, text: &str) {
    selectable_message_sized(ui, colour, text, None);
}

/// As [`selectable_message`], with an explicit text size.
pub fn selectable_message_sized(
    ui: &mut egui::Ui,
    colour: egui::Color32,
    text: &str,
    size: Option<f32>,
) {
    let mut rich = egui::RichText::new(text).color(colour);
    if let Some(s) = size {
        rich = rich.size(s);
    }
    ui.scope(|ui| {
        ui.style_mut().interaction.selectable_labels = true;
        ui.add(egui::Label::new(rich).wrap()).context_menu(|ui| {
            if ui.button(crate::i18n::t("chat.copy")).clicked() {
                ui.ctx().copy_text(text.to_string());
                ui.close();
            }
        });
    });
}
