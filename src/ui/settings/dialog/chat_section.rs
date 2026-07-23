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
use crate::ui::settings::{
    ChatPanelPosition, ChatProviderKind, SettingsDialog, chat_models, secrets,
};

impl SettingsDialog {
    /// The profile list (edit / remove per row) above the add/edit form.
    pub(super) fn chat_profiles_section(&mut self, ui: &mut egui::Ui) {
        use crate::i18n::t;

        // The section title comes from the enclosing sub-header; only the
        // explanatory hint is repeated here.
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
                    if p.allow_writes {
                        label.push_str(&format!("  [{}]", t("db.writes_on")));
                    }
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

                ui.label("");
                ui.checkbox(
                    &mut self.chat_profile_form_allow_writes,
                    t("chat.allow_writes"),
                )
                .on_hover_text(t("chat.allow_writes_hint"));
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
        self.chat_profile_form_allow_writes = p.allow_writes;
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
        self.chat_profile_form_allow_writes = false;
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
            allow_writes: self.chat_profile_form_allow_writes,
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

    /// Global (not per-profile) assistant options: models.toml, Ollama server
    /// URL, caps, panel position, export dir, write protection, audit log.
    pub(super) fn chat_global_options_grid(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings_chat")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                // The preset model lists come from a hand-editable
                // models.toml beside settings.toml; let the user reload it
                // after editing without restarting.
                ui.label(crate::i18n::t("chat.models_file"));
                ui.vertical(|ui| {
                    if ui.button(crate::i18n::t("chat.reload_models")).clicked() {
                        crate::ui::settings::chat_models::reload();
                    }
                    ui.label(
                        egui::RichText::new(
                            crate::ui::settings::chat_models::path()
                                .display()
                                .to_string(),
                        )
                        .weak()
                        .size(11.0),
                    );
                });
                ui.end_row();

                // The Ollama server address stays global: it is the local
                // server the panel probes and can start/stop, not a property
                // of any one profile. A profile may still override it.
                ui.label(crate::i18n::t("chat.ollama_url"))
                    .on_hover_text(crate::i18n::t("settings_hint.chat_ollama_url"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.draft.chat_ollama_url)
                        .desired_width(280.0)
                        .hint_text("http://localhost:11434"),
                );
                ui.end_row();

                ui.label(crate::i18n::t("chat.max_iterations"))
                    .on_hover_text(crate::i18n::t("settings_hint.chat_max_iterations"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.chat_max_iterations_buf)
                        .desired_width(100.0)
                        .hint_text("12"),
                );
                ui.end_row();

                ui.label(crate::i18n::t("chat.max_tokens"))
                    .on_hover_text(crate::i18n::t("settings_hint.chat_max_tokens"));
                ui.horizontal(|ui| {
                    let edit = egui::TextEdit::singleline(&mut self.chat_max_tokens_buf)
                        .desired_width(100.0)
                        .hint_text("16,384");
                    ui.add_enabled(!self.chat_unlimited_tokens, edit);
                    ui.checkbox(
                        &mut self.chat_unlimited_tokens,
                        crate::i18n::t("settings.unlimited"),
                    );
                });
                ui.end_row();

                ui.label(crate::i18n::t("chat.result_row_limit"))
                    .on_hover_text(crate::i18n::t("settings_hint.chat_result_row_limit"));
                ui.horizontal(|ui| {
                    let edit = egui::TextEdit::singleline(&mut self.chat_result_row_limit_buf)
                        .desired_width(100.0)
                        .hint_text("200");
                    ui.add_enabled(!self.chat_unlimited_rows, edit);
                    ui.checkbox(
                        &mut self.chat_unlimited_rows,
                        crate::i18n::t("settings.unlimited"),
                    );
                });
                ui.end_row();

                ui.label(crate::i18n::t("chat.position"))
                    .on_hover_text(crate::i18n::t("settings_hint.chat_position"));
                egui::ComboBox::from_id_salt("settings_chat_position")
                    .selected_text(self.draft.chat_panel_position.label_t())
                    .show_ui(ui, |ui| {
                        for option in ChatPanelPosition::ALL {
                            ui.selectable_value(
                                &mut self.draft.chat_panel_position,
                                *option,
                                option.label_t(),
                            );
                        }
                    });
                ui.end_row();

                ui.label(crate::i18n::t("chat.export_dir"))
                    .on_hover_text(crate::i18n::t("settings_hint.chat_export_dir"));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.draft.chat_export_dir)
                            .desired_width(280.0),
                    );
                    if ui.button(crate::i18n::t("chat.browse")).clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        self.draft.chat_export_dir = dir.to_string_lossy().into_owned();
                    }
                });
                ui.end_row();

                // Write protection + backups gate what the assistant (and
                // schema-changing database saves) may do to existing files.
                ui.label(crate::i18n::t("settings.write_protection"))
                    .on_hover_text(crate::i18n::t("settings_hint.write_protection"));
                ui.checkbox(&mut self.draft.write_protection, "");
                ui.end_row();

                ui.label(crate::i18n::t("settings.backup_before_modify"))
                    .on_hover_text(crate::i18n::t("settings_hint.backup_before_modify"));
                ui.checkbox(&mut self.draft.backup_before_modify, "");
                ui.end_row();

                ui.label(crate::i18n::t("chat.audit_log"))
                    .on_hover_text(crate::i18n::t("settings_hint.chat_audit_log"));
                ui.checkbox(&mut self.draft.chat_audit_log_enabled, "");
                ui.end_row();

                ui.label(crate::i18n::t("chat.audit_warn"))
                    .on_hover_text(crate::i18n::t("settings_hint.chat_audit_warn"));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.draft.chat_audit_log_warn_enabled, "");
                    ui.add_enabled_ui(self.draft.chat_audit_log_warn_enabled, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.chat_audit_warn_mb_buf)
                                .desired_width(56.0),
                        );
                        ui.label(crate::i18n::t("chat.audit_warn_mb"));
                    });
                });
                ui.end_row();
            });
    }

    /// API-key management: the active provider's key controls plus the
    /// per-provider overview grid.
    pub(super) fn chat_api_keys_body(&mut self, ui: &mut egui::Ui) {
        // API-key management for the active provider; keyless providers
        // (Ollama) just show a note. Keyring writes are immediate; the
        // plaintext fallback commits with the rest of the draft on Apply.
        let provider = self.draft.chat_provider;
        if provider.needs_api_key() {
            ui.separator();
            ui.label(crate::i18n::t("chat.api_key"))
                .on_hover_text(crate::i18n::t("settings_hint.chat_api_key"));
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.chat_key_input_buf)
                        .password(true)
                        .hint_text(crate::i18n::t("chat.api_key_hint"))
                        .desired_width(220.0),
                );
                if ui.button(crate::i18n::t("chat.save_key")).clicked()
                    && !self.chat_key_input_buf.trim().is_empty()
                {
                    let key = self.chat_key_input_buf.trim().to_string();
                    match secrets::set_api_key(provider, &key, &mut self.draft) {
                        Ok(true) => {
                            self.chat_key_status_msg =
                                Some(crate::i18n::t("chat.key_stored_keyring"));
                        }
                        Ok(false) => {
                            self.chat_key_status_msg = Some(format!(
                                "{} {}",
                                crate::i18n::t("chat.key_stored_plaintext"),
                                secrets::plaintext_path().display()
                            ));
                        }
                        Err(e) => self.chat_key_status_msg = Some(e),
                    }
                    self.chat_key_input_buf.clear();
                }
                if ui.button(crate::i18n::t("chat.clear_key")).clicked() {
                    // Don't wipe the key on a single click - arm a
                    // confirmation row instead (rendered just below).
                    self.chat_key_clear_confirm = Some(provider);
                    self.chat_key_status_msg = None;
                }
            });
            // Confirmation row: only an explicit second click deletes the
            // saved key. Cancel disarms it.
            if self.chat_key_clear_confirm == Some(provider) {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(crate::i18n::t("chat.clear_key_confirm"))
                            .color(egui::Color32::from_rgb(0xd9, 0x53, 0x4f)),
                    );
                    if ui.button(crate::i18n::t("chat.clear_key_yes")).clicked() {
                        secrets::delete_api_key(provider, &mut self.draft);
                        self.chat_key_status_msg = Some(crate::i18n::t("chat.key_cleared"));
                        self.chat_key_clear_confirm = None;
                    }
                    if ui.button(crate::i18n::t("chat.clear_key_cancel")).clicked() {
                        self.chat_key_clear_confirm = None;
                    }
                });
            }
            let where_msg = match secrets::storage_location(provider, &self.draft) {
                secrets::KeyStorage::Env(var) => {
                    format!("{} {var}", crate::i18n::t("chat.key_source_env"))
                }
                secrets::KeyStorage::Keyring => crate::i18n::t("chat.key_source_keyring"),
                secrets::KeyStorage::Plaintext(path) => format!(
                    "{} {}",
                    crate::i18n::t("chat.key_source_plaintext"),
                    path.display()
                ),
                secrets::KeyStorage::None => crate::i18n::t("chat.key_source_none"),
            };
            ui.label(format!("{} {where_msg}", crate::i18n::t("chat.key_source")));
            if let Some(msg) = &self.chat_key_status_msg {
                ui.colored_label(egui::Color32::from_rgb(0x30, 0x80, 0x30), msg);
            }
            // Keys stored in the keyring take effect at once, but the
            // plaintext fallback only commits with the rest of the dialog -
            // tell the user to Apply.
            ui.label(
                egui::RichText::new(crate::i18n::t("chat.key_apply_hint"))
                    .weak()
                    .size(11.0),
            );
        } else {
            ui.separator();
            ui.label(crate::i18n::t("chat.ollama_no_key"));
        }

        // Per-provider key overview, so the user can see at a glance which
        // providers are configured (not just the selected one).
        ui.separator();
        ui.label(egui::RichText::new(crate::i18n::t("chat.key_status_title")).strong());
        egui::Grid::new("settings_chat_keystatus")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                for kind in ChatProviderKind::ALL {
                    ui.label(kind.label());
                    if !kind.needs_api_key() {
                        ui.weak(crate::i18n::t("chat.ollama_no_key_short"));
                        ui.end_row();
                        continue;
                    }
                    let (text, set) = match secrets::storage_location(*kind, &self.draft) {
                        secrets::KeyStorage::Env(var) => (
                            format!("{} ({var})", crate::i18n::t("chat.key_source_env")),
                            true,
                        ),
                        secrets::KeyStorage::Keyring => {
                            (crate::i18n::t("chat.key_source_keyring"), true)
                        }
                        secrets::KeyStorage::Plaintext(_) => {
                            (crate::i18n::t("chat.key_source_plaintext_short"), true)
                        }
                        secrets::KeyStorage::None => {
                            (crate::i18n::t("chat.key_source_none"), false)
                        }
                    };
                    if set {
                        ui.colored_label(egui::Color32::from_rgb(0x30, 0x80, 0x30), text);
                    } else {
                        ui.weak(text);
                    }
                    ui.end_row();
                }
            });
    }
}
