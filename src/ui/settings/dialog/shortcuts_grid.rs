//! The Shortcuts section of the Settings dialog: one editable row per
//! [`ShortcutAction`], grouped by [`ShortcutGroup`]. Split out of the main
//! dialog module purely for navigability - no behaviour change.

use egui;

use crate::ui::settings::SettingsDialog;
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
                        "{} is already bound to \"{}\". Clear that binding first or pick a different key.",
                        combo.label(),
                        other.label(),
                    ));
                } else {
                    self.draft.shortcuts.set(action, combo);
                    self.shortcut_conflict = None;
                }
                self.recording = None;
            }
        }

        if let Some(msg) = &self.shortcut_conflict {
            ui.colored_label(egui::Color32::from_rgb(0xd9, 0x53, 0x4f), msg);
            ui.add_space(4.0);
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
                        }
                        if ui.button(crate::i18n::t("settings.clear")).clicked() {
                            self.draft.shortcuts.set(action, KeyCombo::UNBOUND);
                        }
                        if ui.button(crate::i18n::t("settings.reset")).clicked() {
                            self.draft.shortcuts.reset(action);
                        }
                    });
                }
            });
        }
    }
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
