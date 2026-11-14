//! Jev section: the switch, key, model and parallelism of Classify with Jev
//! (TypeSafe AI's decision model, `crate::api::jev`).
//!
//! The warning is not decoration: switching this on is the user agreeing that
//! cell data goes to servers in the United States. It is drawn above the
//! switch, always, and everything below the switch stays greyed until it is on.

use std::sync::{Arc, Mutex};

use egui;

use super::super::*;
use crate::api::jev::{self, JevConfig, KEY_ENV, KEY_ID, MAX_PARALLEL};
use crate::i18n::t;
use crate::ui::control_row::{control_grid, control_line, control_text_edit};
use crate::ui::menu_entry::disabled_tip;

type ModelSlot = Arc<Mutex<Option<Result<Vec<String>, String>>>>;

/// The section's own form state; reset whenever Settings opens.
#[derive(Default)]
pub(super) struct JevForm {
    key_buf: String,
    /// "Parallel requests" as typed; parsed as you type.
    pub(super) parallel_buf: String,
    /// A single click on Clear arms this; the second one deletes the key.
    clear_confirm: bool,
    /// Outcome of the last key or model action: text, and whether it is bad news.
    status: Option<(String, bool)>,
    /// Models the last Refresh returned.
    models: Vec<String>,
    /// A Refresh under way, filled by its worker.
    fetching: Option<ModelSlot>,
}

const OK_GREEN: egui::Color32 = egui::Color32::from_rgb(0x30, 0x80, 0x30);
const WARN_RED: egui::Color32 = egui::Color32::from_rgb(0xd9, 0x53, 0x4f);

impl SettingsDialog {
    pub(super) fn jev_section_body(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new(t("jev.intro")).weak().size(11.0));
        ui.add_space(6.0);
        ui.label(egui::RichText::new(t("jev.warning")).color(ui.visuals().warn_fg_color));
        ui.add_space(6.0);
        ui.checkbox(&mut self.draft.jev.enabled, t("jev.enable"))
            .on_hover_text(t("jev.enable_hint"));
        ui.add_space(6.0);

        let on = self.draft.jev.enabled;
        let off_why = t("greyed.jev_off");
        self.poll_jev_models(ui.ctx());

        control_grid(ui, "settings_jev", |ui| {
            // --- API key ---
            ui.label(t("chat.api_key")).on_hover_text(t("jev.key_hint"));
            control_line(ui, |ui| {
                let edit = egui::TextEdit::singleline(&mut self.jev.key_buf)
                    .password(true)
                    .hint_text(t("chat.api_key_hint"));
                ui.add_enabled_ui(on, |ui| control_text_edit(ui, 220.0, edit))
                    .inner
                    .on_hover_text(t("jev.key_hint"))
                    .on_disabled_hover_text(disabled_tip(&t("jev.key_hint"), &off_why));
                if ui
                    .add_enabled(on, egui::Button::new(t("chat.save_key")))
                    .on_hover_text(t("jev.save_key_hint"))
                    .on_disabled_hover_text(disabled_tip(&t("jev.save_key_hint"), &off_why))
                    .clicked()
                    && !self.jev.key_buf.trim().is_empty()
                {
                    let key = self.jev.key_buf.trim().to_string();
                    self.jev.status =
                        Some(match secrets::set_key_for(KEY_ID, &key, &mut self.draft) {
                            Ok(true) => (t("chat.key_stored_keyring"), false),
                            Ok(false) => (
                                format!(
                                    "{} {}",
                                    t("chat.key_stored_plaintext"),
                                    secrets::plaintext_path().display()
                                ),
                                true,
                            ),
                            Err(e) => (e, true),
                        });
                    self.jev.key_buf.clear();
                }
                // Removing a key is never greyed: switching the feature off
                // must not strand a stored secret.
                if ui
                    .button(t("chat.clear_key"))
                    .on_hover_text(t("jev.clear_key_hint"))
                    .clicked()
                {
                    self.jev.clear_confirm = true;
                }
            });
            ui.end_row();

            // --- Model ---
            ui.label(t("jev.model")).on_hover_text(t("jev.model_hint"));
            control_line(ui, |ui| {
                let mut names = self.jev.models.clone();
                for known in [jev::DEFAULT_MODEL, self.draft.jev.model.as_str()] {
                    if !known.is_empty() && !names.iter().any(|n| n == known) {
                        names.insert(0, known.to_string());
                    }
                }
                ui.add_enabled_ui(on, |ui| {
                    egui::ComboBox::from_id_salt("settings_jev_model")
                        .selected_text(self.draft.jev.model.as_str())
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.draft.jev.model,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                })
                .inner
                .on_hover_text(t("jev.model_hint"))
                .on_disabled_hover_text(disabled_tip(&t("jev.model_hint"), &off_why));

                let busy = self.jev.fetching.is_some();
                let why = if !on {
                    Some(off_why.clone())
                } else if busy {
                    Some(t("jev.refreshing"))
                } else {
                    None
                };
                if crate::ui::menu_entry::menu_entry(
                    ui,
                    &t("jev.refresh_models"),
                    &t("jev.refresh_models_hint"),
                    why.as_deref(),
                ) {
                    self.start_jev_model_fetch();
                }
                if busy {
                    ui.spinner();
                }
            });
            ui.end_row();

            // --- Parallel requests ---
            ui.label(t("jev.parallel"))
                .on_hover_text(t("jev.parallel_hint"));
            let edit = egui::TextEdit::singleline(&mut self.jev.parallel_buf)
                .hint_text(jev::DEFAULT_PARALLEL.to_string());
            let resp = ui
                .add_enabled_ui(on, |ui| control_text_edit(ui, 60.0, edit))
                .inner
                .on_hover_text(t("jev.parallel_hint"))
                .on_disabled_hover_text(disabled_tip(&t("jev.parallel_hint"), &off_why));
            if resp.changed()
                && let Ok(n) = self.jev.parallel_buf.trim().parse::<usize>()
            {
                self.draft.jev.parallel = n.clamp(1, MAX_PARALLEL);
            }
            ui.end_row();
        });

        if self.jev.clear_confirm {
            control_line(ui, |ui| {
                ui.label(egui::RichText::new(t("chat.clear_key_confirm")).color(WARN_RED));
                if ui.button(t("chat.clear_key_yes")).clicked() {
                    secrets::delete_key_for(KEY_ID, &mut self.draft);
                    self.purge_secret(SecretPurge::Chat(KEY_ID.to_string()));
                    self.jev.status = Some((t("chat.key_cleared"), false));
                    self.jev.clear_confirm = false;
                }
                if ui.button(t("chat.clear_key_cancel")).clicked() {
                    self.jev.clear_confirm = false;
                }
            });
        }

        let source = match secrets::key_storage_for(KEY_ID, Some(KEY_ENV), &self.draft) {
            secrets::KeyStorage::Env(var) => format!("{} {var}", t("chat.key_source_env")),
            secrets::KeyStorage::Keyring => t("chat.key_source_keyring"),
            secrets::KeyStorage::Plaintext(path) => {
                format!("{} {}", t("chat.key_source_plaintext"), path.display())
            }
            secrets::KeyStorage::None => t("chat.key_source_none"),
        };
        ui.label(format!("{} {source}", t("chat.key_source")));
        if let Some((msg, bad)) = &self.jev.status {
            // Selectable, so a server's refusal can be copied.
            ui.add(
                egui::Label::new(egui::RichText::new(msg).color(if *bad {
                    WARN_RED
                } else {
                    OK_GREEN
                }))
                .selectable(true)
                .wrap(),
            );
        }
        ui.label(
            egui::RichText::new(t("jev.headless_note"))
                .weak()
                .size(11.0),
        );
    }

    /// Seed the section's buffers when Settings opens.
    pub(super) fn seed_jev_form(&mut self) {
        self.jev = JevForm {
            parallel_buf: self.draft.jev.parallel.to_string(),
            ..JevForm::default()
        };
    }

    /// Ask TypeSafe which models the key may use, on a worker: the request can
    /// take as long as its timeout.
    fn start_jev_model_fetch(&mut self) {
        // Read the draft, so a key saved a moment ago is the one tested, and
        // treat the switch as on: the button is greyed otherwise.
        let mut probe = self.draft.clone();
        probe.jev.enabled = true;
        let config = match JevConfig::check(&probe) {
            Ok(c) => c,
            Err(missing) => {
                self.jev.status = Some((t(missing.i18n_key()), true));
                return;
            }
        };
        let slot: ModelSlot = Arc::new(Mutex::new(None));
        let out = slot.clone();
        std::thread::spawn(move || {
            let result = config.list_models().map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = out.lock() {
                *g = Some(result);
            }
        });
        self.jev.fetching = Some(slot);
        self.jev.status = None;
    }

    fn poll_jev_models(&mut self, ctx: &egui::Context) {
        let Some(slot) = &self.jev.fetching else {
            return;
        };
        let done = slot.lock().ok().and_then(|mut g| g.take());
        match done {
            None => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
            Some(Ok(models)) => {
                self.jev.status = Some((
                    t("jev.models_found").replace("{n}", &models.len().to_string()),
                    false,
                ));
                self.jev.models = models;
                self.jev.fetching = None;
            }
            Some(Err(e)) => {
                self.jev.status = Some((e, true));
                self.jev.fetching = None;
            }
        }
    }
}
