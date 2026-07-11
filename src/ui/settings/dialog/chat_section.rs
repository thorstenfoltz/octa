//! Chat / Assistant profile manager for the Settings dialog: the list of named
//! model profiles plus the add/edit form. Split out of the main dialog module
//! for navigability.
//!
//! A profile bundles provider + model + temperature + thinking under a name the
//! user chose, so the assistant panel switches between whole configurations with
//! one dropdown. There can be many per provider (an Anthropic "Opus, deep" next
//! to an Anthropic "Sonnet, quick"), which is why this is a list with a form and
//! not a set of fixed fields.

use egui;

use crate::ui::settings::chat_profiles::ChatModelProfile;
use crate::ui::settings::{ChatProviderKind, SettingsDialog, chat_models, secrets};

impl SettingsDialog {
    /// The profile list (edit / remove per row) above the add/edit form.
    pub(super) fn chat_profiles_section(&mut self, ui: &mut egui::Ui) {
        use crate::i18n::t;

        ui.label(egui::RichText::new(t("chat.profiles")).strong());
        ui.label(
            egui::RichText::new(t("settings_hint.chat_profiles"))
                .weak()
                .size(11.0),
        );
        ui.add_space(4.0);

        if self.draft.chat_profiles.is_empty() {
            ui.label(egui::RichText::new(t("chat.no_profiles")).weak().size(11.0));
        }

        let mut remove: Option<usize> = None;
        let mut edit: Option<usize> = None;
        let active = self.draft.chat_active_profile.clone();

        egui::Grid::new("settings_chat_profile_list")
            .num_columns(3)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for (i, p) in self.draft.chat_profiles.iter().enumerate() {
                    let mut label = format!("{}  ({} / {})", p.name, p.kind.label(), p.model);
                    if p.id == active {
                        // Deleting the profile the panel is using is allowed
                        // (ensure_profiles re-points it), but the user should
                        // see which one that is.
                        label.push_str("  *");
                    }
                    ui.label(label).on_hover_text(if p.description.is_empty() {
                        p.id.clone()
                    } else {
                        p.description.clone()
                    });
                    if ui.small_button(t("cloud.edit")).clicked() {
                        edit = Some(i);
                    }
                    if ui.small_button(t("cloud.remove")).clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });

        if let Some(i) = remove
            && i < self.draft.chat_profiles.len()
        {
            let p = self.draft.chat_profiles.remove(i);
            // A profile's own key is meaningless once the profile is gone.
            secrets::delete_profile_key(&p.id, &mut self.draft);
            if self.chat_profile_form_id == p.id {
                self.clear_chat_profile_form();
            }
            // Keep the active id pointing at something that exists.
            if self.draft.chat_active_profile == p.id {
                self.draft.chat_active_profile = self
                    .draft
                    .chat_profiles
                    .first()
                    .map(|p| p.id.clone())
                    .unwrap_or_default();
            }
        }
        if let Some(i) = edit {
            self.load_chat_profile_form(i);
        }

        ui.add_space(6.0);
        ui.separator();
        self.chat_profile_form(ui);
    }

    /// The add/edit form. Editing is the same form pre-filled, so there is one
    /// place where a profile's fields are defined.
    fn chat_profile_form(&mut self, ui: &mut egui::Ui) {
        use crate::i18n::t;

        let editing = !self.chat_profile_form_id.is_empty();
        ui.label(
            egui::RichText::new(if editing {
                t("chat.edit_profile")
            } else {
                t("chat.add_profile")
            })
            .strong(),
        );

        egui::Grid::new("settings_chat_profile_form")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(t("chat.profile_name"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.chat_profile_form_name)
                        .desired_width(320.0)
                        .hint_text(t("chat.profile_name_ph")),
                );
                ui.end_row();

                ui.label(t("chat.profile_desc"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.chat_profile_form_desc)
                        .desired_width(320.0)
                        .hint_text(t("chat.profile_desc_ph")),
                );
                ui.end_row();

                ui.label(t("chat.provider"))
                    .on_hover_text(crate::i18n::t("settings_hint.chat_provider"));
                egui::ComboBox::from_id_salt("settings_chat_profile_kind")
                    .selected_text(self.chat_profile_form_kind.label())
                    .show_ui(ui, |ui| {
                        for kind in ChatProviderKind::ALL {
                            ui.selectable_value(
                                &mut self.chat_profile_form_kind,
                                *kind,
                                kind.label(),
                            );
                        }
                    });
                ui.end_row();

                ui.label(t("chat.model"))
                    .on_hover_text(crate::i18n::t("settings_hint.chat_model"));
                self.chat_profile_model_picker(ui);
                ui.end_row();

                ui.label(t("chat.temperature"))
                    .on_hover_text(crate::i18n::t("settings_hint.chat_temperature"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.chat_profile_form_temp)
                        .desired_width(100.0)
                        .hint_text("0.0"),
                );
                ui.end_row();

                ui.label(t("chat.reasoning"))
                    .on_hover_text(t("chat.reasoning_hint"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.chat_profile_form_reasoning)
                        .desired_width(200.0)
                        .hint_text(t("chat.reasoning_ph")),
                )
                .on_hover_text(t("chat.reasoning_hint"));
                ui.end_row();

                // Only the endpoint-based providers have a base URL to set.
                if matches!(
                    self.chat_profile_form_kind,
                    ChatProviderKind::OpenAiCompatible | ChatProviderKind::Ollama
                ) {
                    ui.label(t("chat.base_url"))
                        .on_hover_text(t("settings_hint.chat_profile_base_url"));
                    let hint = if self.chat_profile_form_kind == ChatProviderKind::Ollama {
                        self.draft.chat_ollama_url.clone()
                    } else {
                        "https://openrouter.ai/api/v1".to_string()
                    };
                    ui.add(
                        egui::TextEdit::singleline(&mut self.chat_profile_form_base_url)
                            .desired_width(280.0)
                            .hint_text(hint),
                    );
                    ui.end_row();
                }

                ui.label("");
                ui.checkbox(
                    &mut self.chat_profile_form_use_own_key,
                    t("chat.use_own_key"),
                )
                .on_hover_text(t("chat.use_own_key_hint"));
                ui.end_row();

                // The own-key field: shown only when the profile opted in. The
                // shared per-provider keys are managed further down the section.
                if self.chat_profile_form_use_own_key {
                    ui.label(t("chat.own_key"));
                    ui.vertical(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.chat_profile_form_key)
                                .password(true)
                                .desired_width(280.0)
                                .hint_text(t("chat.own_key_ph")),
                        );
                        if editing {
                            let stored = secrets::profile_key_storage(
                                &self.chat_profile_form_id,
                                &self.draft,
                            );
                            if stored != secrets::KeyStorage::None {
                                ui.label(
                                    egui::RichText::new(t("chat.own_key_set")).weak().size(11.0),
                                );
                            }
                        }
                    });
                    ui.end_row();
                }
            });

        ui.horizontal(|ui| {
            if ui.button(t("chat.save_profile")).clicked() {
                self.save_chat_profile_form();
            }
            if editing && ui.button(t("cloud.cancel_edit")).clicked() {
                self.clear_chat_profile_form();
            }
        });

        if let Some(msg) = self.chat_profile_status.clone() {
            ui.label(egui::RichText::new(msg).size(11.0));
        }
    }

    /// Model field for the profile form: a preset dropdown (when the provider
    /// has presets) above a free-text field, so the newest model can always be
    /// typed even if it is not in the list.
    fn chat_profile_model_picker(&mut self, ui: &mut egui::Ui) {
        let kind = self.chat_profile_form_kind;
        let presets = chat_models::preset_models(kind);
        const W: f32 = 320.0;

        ui.vertical(|ui| {
            if !presets.is_empty() {
                egui::ComboBox::from_id_salt(("settings_chat_profile_model", kind.id()))
                    .selected_text(if self.chat_profile_form_model.is_empty() {
                        "(model)".to_string()
                    } else {
                        self.chat_profile_form_model.clone()
                    })
                    .width(W)
                    .show_ui(ui, |ui| {
                        for m in &presets {
                            if ui
                                .selectable_label(&self.chat_profile_form_model == m, m.as_str())
                                .clicked()
                            {
                                self.chat_profile_form_model = m.clone();
                            }
                        }
                    });
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.chat_profile_form_model)
                    .desired_width(W)
                    .hint_text(chat_models::default_model(kind)),
            );
        });
    }

    /// Pre-fill the form from an existing profile (the Edit button).
    fn load_chat_profile_form(&mut self, index: usize) {
        let Some(p) = self.draft.chat_profiles.get(index) else {
            return;
        };
        self.chat_profile_form_id = p.id.clone();
        self.chat_profile_form_name = p.name.clone();
        self.chat_profile_form_desc = p.description.clone();
        self.chat_profile_form_kind = p.kind;
        self.chat_profile_form_model = p.model.clone();
        self.chat_profile_form_temp = format!("{:.2}", p.temperature);
        self.chat_profile_form_reasoning = p.reasoning.clone();
        self.chat_profile_form_base_url = p.base_url.clone();
        self.chat_profile_form_use_own_key = p.use_own_key;
        // Never read a stored secret back into a text field.
        self.chat_profile_form_key.clear();
        self.chat_profile_status = None;
    }

    /// Reset the form back to "adding a new profile".
    pub(crate) fn clear_chat_profile_form(&mut self) {
        self.chat_profile_form_id.clear();
        self.chat_profile_form_name.clear();
        self.chat_profile_form_desc.clear();
        self.chat_profile_form_kind = ChatProviderKind::default();
        self.chat_profile_form_model.clear();
        self.chat_profile_form_temp.clear();
        self.chat_profile_form_reasoning.clear();
        self.chat_profile_form_base_url.clear();
        self.chat_profile_form_use_own_key = false;
        self.chat_profile_form_key.clear();
        self.chat_profile_status = None;
    }

    /// Validate the form and add or update the profile in the draft settings.
    fn save_chat_profile_form(&mut self) {
        use crate::i18n::t;

        let name = self.chat_profile_form_name.trim().to_string();
        if name.is_empty() {
            self.chat_profile_status = Some(t("chat.profile_need_name"));
            return;
        }

        let model = self.chat_profile_form_model.trim().to_string();
        let model = if model.is_empty() {
            chat_models::default_model(self.chat_profile_form_kind)
        } else {
            model
        };

        // Comma-tolerant, same as the other numeric settings buffers. A blank
        // or unparseable value means 0.0 (deterministic), not an error.
        let temperature = self
            .chat_profile_form_temp
            .trim()
            .replace(',', ".")
            .parse::<f32>()
            .unwrap_or(0.0)
            .clamp(0.0, 2.0);

        // The id is minted once and then frozen: it addresses the profile's key
        // in the keyring, so renaming must not orphan it.
        let id = if self.chat_profile_form_id.is_empty() {
            ChatModelProfile::fresh_id(&name, &self.draft.chat_profiles)
        } else {
            self.chat_profile_form_id.clone()
        };

        let profile = ChatModelProfile {
            id: id.clone(),
            name,
            description: self.chat_profile_form_desc.trim().to_string(),
            kind: self.chat_profile_form_kind,
            model,
            temperature,
            reasoning: self.chat_profile_form_reasoning.trim().to_string(),
            base_url: self.chat_profile_form_base_url.trim().to_string(),
            use_own_key: self.chat_profile_form_use_own_key,
        };

        match self.draft.chat_profiles.iter().position(|p| p.id == id) {
            Some(i) => self.draft.chat_profiles[i] = profile,
            None => self.draft.chat_profiles.push(profile),
        }

        let mut status = t("chat.profile_saved");
        if self.chat_profile_form_use_own_key {
            let key = self.chat_profile_form_key.trim().to_string();
            if !key.is_empty() {
                match secrets::set_profile_key(&id, &key, &mut self.draft) {
                    Ok(true) => {}
                    Ok(false) => status = t("chat.key_saved_plain"),
                    Err(e) => status = e,
                }
            }
        } else {
            // Turning the override off drops the key rather than leaving an
            // unused secret behind.
            secrets::delete_profile_key(&id, &mut self.draft);
        }

        // The panel always needs a selected profile; the first one saved wins.
        if self.draft.chat_active_profile.is_empty() {
            self.draft.chat_active_profile = id;
        }

        // Back to "add a new profile", but keep the confirmation visible.
        self.clear_chat_profile_form();
        self.chat_profile_status = Some(status);
    }
}
