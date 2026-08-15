//! The Shortcuts section of the Settings dialog: one editable row per
//! [`ShortcutAction`], grouped by [`ShortcutGroup`]. Split out of the main
//! dialog module purely for navigability - no behaviour change.

use egui;

use crate::ui::settings::{SettingsDialog, ShortcutTakeover};
use crate::ui::shortcuts::{KeyCombo, ShortcutAction};

impl SettingsDialog {
    /// One grid row per [`ShortcutAction`]: name, current combo, Record/Clear/Reset.
    pub(super) fn draw_shortcuts_grid(&mut self, ui: &mut egui::Ui) {
        use strum::IntoEnumIterator;
        // If the user is recording a binding, capture the next real key press.
        if let Some(action) = self.recording {
            let captured = ui.input(capture_combo);
            if let Some(CaptureResult::Cancel) = captured {
                self.recording = None;
            } else if let Some(CaptureResult::Combo(combo)) = captured {
                // Reject combos already bound to another action so two
                // functions can never share a shortcut.
                let conflict = self
                    .draft
                    .shortcuts
                    .bindings
                    .iter()
                    .find(|(other, existing)| **other != action && **existing == combo)
                    .map(|(other, _)| *other);
                if let Some(other) = conflict {
                    self.shortcut_conflict = Some(format!(
                        "{} is already bound to \"{}\".",
                        combo.label(),
                        other.label(),
                    ));
                    self.shortcut_takeover = Some(ShortcutTakeover {
                        action,
                        combo,
                        previous: other,
                    });
                } else {
                    self.draft.shortcuts.set(action, combo);
                    self.shortcut_conflict = None;
                    self.shortcut_takeover = None;
                }
                self.recording = None;
            }
        }

        // One collapsible sub-section per group, in `ShortcutGroup::ALL`
        // order, so the section opens as a short scannable list of group
        // names instead of one very long grid. Rows use fixed column widths
        // so the action / combo columns line up across every group.
        const LABEL_W: f32 = 250.0;
        const COMBO_W: f32 = 160.0;
        for group in crate::ui::shortcuts::ShortcutGroup::ALL {
            let actions: Vec<ShortcutAction> = ShortcutAction::iter()
                .filter(|a| a.group() == *group)
                .collect();
            if actions.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(
                egui::RichText::new(crate::i18n::t(group.i18n_key()))
                    .strong()
                    .size(14.0),
            )
            .id_salt(("settings_shortcuts_group", group.i18n_key()))
            .show(ui, |ui| {
                let row_h = ui.spacing().interact_size.y;
                for action in actions {
                    ui.horizontal(|ui| {
                        // Fixed-width columns kept for cross-row alignment, but the
                        // action and combo text are left-aligned within them (a bare
                        // `add_sized` centres its content).
                        ui.allocate_ui_with_layout(
                            egui::vec2(LABEL_W, row_h),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(LABEL_W);
                                ui.add(
                                    egui::Label::new(action.label())
                                        .wrap_mode(egui::TextWrapMode::Truncate),
                                );
                            },
                        );
                        let combo = self.draft.shortcuts.combo(action);
                        let label_text = if self.recording == Some(action) {
                            egui::RichText::new("Press any key...").italics()
                        } else {
                            egui::RichText::new(combo.label()).monospace()
                        };
                        ui.allocate_ui_with_layout(
                            egui::vec2(COMBO_W, row_h),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(COMBO_W);
                                ui.add(egui::Label::new(label_text));
                            },
                        );
                        if self.recording == Some(action) {
                            if ui.button(crate::i18n::t("settings.sc_stop")).clicked() {
                                self.recording = None;
                            }
                        } else if ui.button(crate::i18n::t("settings.sc_record")).clicked() {
                            self.recording = Some(action);
                            // A pending offer belongs to the row it came from.
                            self.shortcut_conflict = None;
                            self.shortcut_takeover = None;
                        }
                        if ui.button(crate::i18n::t("settings.clear")).clicked() {
                            self.draft.shortcuts.set(action, KeyCombo::UNBOUND);
                        }
                        if ui.button(crate::i18n::t("settings.reset")).clicked() {
                            self.draft.shortcuts.reset(action);
                        }
                    });

                    // The "that key is taken" answer belongs under the row the
                    // user is editing. At the top of the section it was
                    // usually scrolled out of sight, so a refused rebind
                    // looked like nothing had happened at all.
                    if let Some(pending) = self.shortcut_takeover.filter(|t| t.action == action) {
                        let msg = self.shortcut_conflict.clone().unwrap_or_default();
                        ui.horizontal(|ui| {
                            ui.add_space(LABEL_W * 0.1);
                            ui.colored_label(egui::Color32::from_rgb(0xd9, 0x53, 0x4f), msg);
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(LABEL_W * 0.1);
                            // Offer the move rather than only refusing it: two
                            // actions still cannot share a combo, but the
                            // previous owner is left unbound instead of the
                            // user having to go and clear it first.
                            if ui
                                .button(crate::i18n::t("settings.sc_takeover"))
                                .on_hover_text(
                                    crate::i18n::t("settings.sc_takeover_hint")
                                        .replace("{action}", pending.previous.label()),
                                )
                                .clicked()
                            {
                                self.draft
                                    .shortcuts
                                    .set(pending.previous, KeyCombo::UNBOUND);
                                self.draft.shortcuts.set(pending.action, pending.combo);
                                self.shortcut_conflict = None;
                                self.shortcut_takeover = None;
                            }
                            if ui.button(crate::i18n::t("common.cancel")).clicked() {
                                self.shortcut_conflict = None;
                                self.shortcut_takeover = None;
                            }
                        });
                        ui.add_space(4.0);
                    }
                }
            });
        }
    }
}

/// Whether this key is only a modifier, and so cannot be a shortcut by itself.
fn is_modifier_key(key: egui::Key) -> bool {
    matches!(
        key,
        egui::Key::ShiftLeft
            | egui::Key::ShiftRight
            | egui::Key::ControlLeft
            | egui::Key::ControlRight
            | egui::Key::AltLeft
            | egui::Key::AltRight
            | egui::Key::SuperLeft
            | egui::Key::SuperRight
    )
}

/// Result of a single-frame shortcut capture.
enum CaptureResult {
    Cancel,
    Combo(KeyCombo),
}

/// While recording, watch for a non-modifier key press and return it with the
/// current modifier state. Esc cancels.
fn capture_combo(input: &egui::InputState) -> Option<CaptureResult> {
    if input.key_pressed(egui::Key::Escape) {
        return Some(CaptureResult::Cancel);
    }
    let mods = input.modifiers;
    for ev in &input.events {
        // The clipboard trio never arrives as a key: egui-winit turns
        // Ctrl+C / X / V into these events and returns, so recording one of
        // them used to look like the key press had vanished. Map them back to
        // the key they were, which at least lets the grid answer (usually with
        // "already bound to Copy").
        let clipboard_key = match ev {
            egui::Event::Copy => Some(egui::Key::C),
            egui::Event::Cut => Some(egui::Key::X),
            egui::Event::Paste(_) => Some(egui::Key::V),
            _ => None,
        };
        if let Some(key) = clipboard_key {
            return Some(CaptureResult::Combo(KeyCombo {
                key: Some(key),
                ctrl: true,
                shift: mods.shift,
                alt: mods.alt,
            }));
        }
        if let egui::Event::Key {
            key,
            pressed: true,
            repeat: false,
            ..
        } = ev
        {
            if matches!(key, egui::Key::Escape) {
                return Some(CaptureResult::Cancel);
            }
            // Modifiers arrive as ordinary key events of their own
            // (`ControlLeft`, `ShiftRight`, ...). Capturing one ended the
            // recording immediately with a binding of "Ctrl + the Ctrl key",
            // which is why no combination could be recorded at all: wait for
            // the key the modifier is being held for.
            if is_modifier_key(*key) {
                continue;
            }
            return Some(CaptureResult::Combo(KeyCombo {
                key: Some(*key),
                ctrl: mods.command,
                shift: mods.shift,
                alt: mods.alt,
            }));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::is_modifier_key;

    #[test]
    fn modifiers_are_not_bindable_keys_on_their_own() {
        // egui reports these as ordinary key events, and treating one as the
        // recorded key is what made every Ctrl/Shift/Alt combination
        // impossible to bind (it recorded "Ctrl + the Ctrl key").
        for key in [
            egui::Key::ControlLeft,
            egui::Key::ControlRight,
            egui::Key::ShiftLeft,
            egui::Key::ShiftRight,
            egui::Key::AltLeft,
            egui::Key::AltRight,
            egui::Key::SuperLeft,
            egui::Key::SuperRight,
        ] {
            assert!(is_modifier_key(key), "{key:?} must be skipped");
        }
        for key in [egui::Key::S, egui::Key::F5, egui::Key::Comma] {
            assert!(!is_modifier_key(key), "{key:?} must be recordable");
        }
    }
}
