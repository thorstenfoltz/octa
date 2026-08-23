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

/// How many names a load banner spells out before it starts counting.
pub const BANNER_LIST_MAX: usize = 6;

/// Join a list of names for a one-line banner, naming at most `max` of them
/// and counting the rest.
///
/// A load banner lists the columns it touched, and a wide file can hand it
/// two hundred. Spelled out in full, that one label is wider than any screen,
/// and since it sits *before* the banner's Okay / Dismiss buttons it pushes
/// them off the edge of a window that does not scroll sideways: the banner
/// becomes unanswerable and the file effectively unusable. Callers show the
/// full list on hover, so nothing is lost.
///
/// Returns the text; ask `items.len() > max` yourself if you need to know
/// whether anything was left out.
pub fn elide_list(items: &[String], max: usize) -> String {
    let max = max.max(1);
    if items.len() <= max {
        return items.join(", ");
    }
    format!(
        "{}, {}",
        items[..max].join(", "),
        crate::i18n::t("banner.and_more").replace("{n}", &(items.len() - max).to_string())
    )
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_list_is_spelled_out_in_full() {
        let items: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(elide_list(&items, 6), "a, b, c");
        // Exactly at the cap is still the whole list, not "and 0 more".
        assert_eq!(elide_list(&items, 3), "a, b, c");
    }

    /// The bug this exists for: a JSON file with dozens of date columns made
    /// one banner label wider than the screen, pushing its Okay / Dismiss
    /// buttons out of reach in a panel that does not scroll sideways.
    #[test]
    fn a_long_list_is_cut_to_a_bounded_length() {
        let items: Vec<String> = (0..200).map(|i| format!("column_{i}")).collect();
        let text = elide_list(&items, BANNER_LIST_MAX);
        assert!(text.starts_with("column_0, column_1"));
        assert!(text.contains("194"), "the remainder is not counted: {text}");
        assert!(
            text.len() < 200,
            "still long enough to push the buttons off screen: {} chars",
            text.len()
        );
    }

    /// `max: 0` would otherwise produce a label that names nothing at all.
    #[test]
    fn at_least_one_name_is_always_shown() {
        let items: Vec<String> = ["only", "these"].iter().map(|s| s.to_string()).collect();
        assert!(elide_list(&items, 0).starts_with("only"));
    }

    #[test]
    fn an_empty_list_is_empty() {
        assert_eq!(elide_list(&[], 6), "");
    }
}
