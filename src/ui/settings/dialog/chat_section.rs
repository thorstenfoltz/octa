//! Chat / Assistant model-picker helpers for the Settings dialog. Split out of
//! the main dialog module purely for navigability - no behaviour change.

use egui;

use crate::ui::settings::{ChatProviderKind, SettingsDialog};

impl SettingsDialog {
    /// The model configured for `provider` in the draft, falling back to the
    /// provider's built-in default.
    pub(super) fn chat_current_model(&self, provider: ChatProviderKind) -> String {
        self.draft
            .chat_models
            .get(provider.id())
            .filter(|m| !m.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| crate::ui::settings::chat_models::default_model(provider))
    }

    /// Default-model picker for the Chat section: a preset dropdown (when the
    /// provider has presets) stacked above a roomy free-text field, mirroring the
    /// panel's quick-switch but writing into `draft.chat_models`.
    pub(super) fn chat_model_picker(&mut self, ui: &mut egui::Ui, provider: ChatProviderKind) {
        let presets = crate::ui::settings::chat_models::preset_models(provider);
        let mut model = self.chat_current_model(provider);
        let mut changed = false;
        const W: f32 = 320.0;

        // Vertical so the free-text field gets its own line at a comfortable
        // width instead of being squeezed next to the dropdown.
        ui.vertical(|ui| {
            if !presets.is_empty() {
                egui::ComboBox::from_id_salt(("settings_chat_model_preset", provider.id()))
                    .selected_text(if model.is_empty() {
                        "(model)".to_string()
                    } else {
                        model.clone()
                    })
                    .width(W)
                    .show_ui(ui, |ui| {
                        for m in &presets {
                            if ui.selectable_label(&model == m, m.as_str()).clicked() {
                                model = m.clone();
                                changed = true;
                            }
                        }
                    });
            }

            let resp = ui.add(
                egui::TextEdit::singleline(&mut model)
                    .desired_width(W)
                    .hint_text(crate::ui::settings::chat_models::default_model(provider)),
            );
            if resp.changed() {
                changed = true;
            }
        });

        if changed {
            self.draft
                .chat_models
                .insert(provider.id().to_string(), model);
        }
    }
}
