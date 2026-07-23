//! Cloud storage section of the Settings dialog: the write-enable toggle, the
//! saved-connection list, an add/edit form, and per-connection secret
//! management (keyring, with an armed Clear button mirroring the chat key UI).
//!
//! All edits land in `self.draft` (committed on Apply); secrets are written
//! immediately through `cloud_secrets` (keyring, with a plaintext fallback in
//! the draft). The async Sign in button is added in the next sub-phase (4B)
//! alongside the background cloud worker.

use eframe::egui;

use crate::cloud::{CloudConnection, CloudKind, CloudSecret};
use crate::i18n::t;
use crate::ui::settings::SettingsDialog;
use crate::ui::settings::cloud_secrets;
use crate::ui::settings::secrets::KeyStorage;

impl SettingsDialog {
    /// Body of the "Cloud storage" Settings section.
    pub(super) fn cloud_section_body(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(
            &mut self.draft.cloud_writes_enabled,
            t("cloud.writes_enabled"),
        )
        .on_hover_text(t("cloud.writes_enabled_hint"));
        ui.separator();

        self.cloud_connection_list(ui);
        ui.separator();
        // The add/edit form (plus its secret controls) lives behind its own
        // sub-header so the section opens as a readable connection list;
        // clicking Edit on a row, or the sidebar's "+ Add", forces it open.
        let editing = !self.cloud_form_id.is_empty();
        let force_open = editing || std::mem::take(&mut self.focus_cloud_form);
        egui::CollapsingHeader::new(egui::RichText::new(t("settings.sub_conn_form")).strong())
            .id_salt("settings_cloud_sub_form")
            .open(force_open.then_some(true))
            .show(ui, |ui| {
                self.cloud_connection_form(ui);

                // Secrets apply to S3 + Azure (keys/SAS) and to GCS only when a
                // browser OAuth client id is set (the secret is then the Google
                // client secret). An anonymous connection needs no secret.
                let has_oauth = !self.cloud_form_oauth_client_id.trim().is_empty();
                let show_secret = !self.cloud_form_anonymous
                    && (matches!(self.cloud_form_kind, CloudKind::S3 | CloudKind::AzureBlob)
                        || (self.cloud_form_kind == CloudKind::Gcs && has_oauth));
                if show_secret {
                    ui.separator();
                    ui.label(egui::RichText::new(t("cloud.secret")).strong())
                        .on_hover_text(t("cloud.secret_hint"));
                    self.cloud_secret_controls(ui);
                }

                // Browser sign-in for Azure Blob / GCS once a client id is set.
                if matches!(self.cloud_form_kind, CloudKind::AzureBlob | CloudKind::Gcs)
                    && has_oauth
                {
                    ui.separator();
                    self.cloud_browser_signin_controls(ui);
                }
            });
    }

    /// The saved-connection list with per-row Edit / Remove.
    fn cloud_connection_list(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new(t("cloud.connections")).strong());
        if self.draft.cloud_connections.is_empty() {
            ui.weak(t("cloud.no_connections"));
        }
        let mut remove_idx: Option<usize> = None;
        let mut edit_idx: Option<usize> = None;
        // A grid keeps the Edit / Remove buttons in aligned columns regardless
        // of how long each connection's name is.
        egui::Grid::new("cloud_conn_list")
            .num_columns(3)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for (i, conn) in self.draft.cloud_connections.iter().enumerate() {
                    let mut label = if conn.account_level {
                        format!("{}  ({}: account)", conn.name, kind_label(conn.kind))
                    } else {
                        format!(
                            "{}  ({}: {})",
                            conn.name,
                            kind_label(conn.kind),
                            conn.bucket
                        )
                    };
                    if !conn.allow_writes {
                        label.push_str(&format!("  [{}]", t("db.copy_writes_off")));
                    }
                    if crate::cloud::has_cloud_browser_token(&conn.id) {
                        label.push_str(&format!("  [{}]", t("db.signed_in_browser")));
                    }
                    ui.label(label);
                    if ui.small_button(t("cloud.edit")).clicked() {
                        edit_idx = Some(i);
                    }
                    if ui.small_button(t("cloud.remove")).clicked() {
                        remove_idx = Some(i);
                    }
                    ui.end_row();
                }
            });
        if let Some(i) = remove_idx
            && i < self.draft.cloud_connections.len()
        {
            let conn = self.draft.cloud_connections.remove(i);
            cloud_secrets::delete_cloud_secret(&conn.id, &mut self.draft);
            if self.cloud_form_id == conn.id {
                self.clear_cloud_form();
            }
        }
        if let Some(i) = edit_idx {
            self.load_cloud_form(i);
        }
    }

    /// The add / edit connection form.
    fn cloud_connection_form(&mut self, ui: &mut egui::Ui) {
        let editing = !self.cloud_form_id.is_empty();
        ui.label(
            egui::RichText::new(if editing {
                t("cloud.edit_connection")
            } else {
                t("cloud.add_connection")
            })
            .strong(),
        );

        egui::Grid::new("cloud_conn_form")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label(t("cloud.name"))
                    .on_hover_text(t("cloud.name_hint"));
                ui.text_edit_singleline(&mut self.cloud_form_name)
                    .on_hover_text(t("cloud.name_hint"));
                ui.end_row();

                ui.label(t("cloud.provider"))
                    .on_hover_text(t("cloud.provider_hint"));
                egui::ComboBox::from_id_salt("cloud_form_kind")
                    .selected_text(kind_label(self.cloud_form_kind))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.cloud_form_kind,
                            CloudKind::S3,
                            kind_label(CloudKind::S3),
                        );
                        ui.selectable_value(
                            &mut self.cloud_form_kind,
                            CloudKind::AzureBlob,
                            kind_label(CloudKind::AzureBlob),
                        );
                        ui.selectable_value(
                            &mut self.cloud_form_kind,
                            CloudKind::Gcs,
                            kind_label(CloudKind::Gcs),
                        );
                    });
                ui.end_row();

                ui.label("");
                ui.checkbox(&mut self.cloud_form_account_level, t("cloud.account_level"))
                    .on_hover_text(t("cloud.account_level_hint"));
                ui.end_row();

                if self.cloud_form_account_level {
                    ui.label("");
                    ui.label(t("cloud.account_level_note"));
                    ui.end_row();
                } else {
                    let (bucket_label, bucket_hint) =
                        if self.cloud_form_kind == CloudKind::AzureBlob {
                            (t("cloud.container"), t("cloud.container_hint"))
                        } else {
                            (t("cloud.bucket"), t("cloud.bucket_hint"))
                        };
                    ui.label(bucket_label).on_hover_text(bucket_hint.clone());
                    ui.text_edit_singleline(&mut self.cloud_form_bucket)
                        .on_hover_text(bucket_hint);
                    ui.end_row();

                    ui.label(t("cloud.prefix"))
                        .on_hover_text(t("cloud.prefix_hint"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.cloud_form_prefix)
                            .hint_text(t("cloud.prefix_hint")),
                    );
                    ui.end_row();
                }

                match self.cloud_form_kind {
                    CloudKind::S3 => {
                        ui.label(t("cloud.endpoint"))
                            .on_hover_text(t("cloud.endpoint_hint"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.cloud_form_endpoint)
                                .hint_text(t("cloud.endpoint_hint")),
                        );
                        ui.end_row();
                        ui.label(t("cloud.region"))
                            .on_hover_text(t("cloud.region_hint"));
                        ui.text_edit_singleline(&mut self.cloud_form_region);
                        ui.end_row();
                        ui.label(t("cloud.profile"))
                            .on_hover_text(t("cloud.profile_hint"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.cloud_form_profile)
                                .hint_text(t("cloud.profile_hint")),
                        );
                        ui.end_row();
                        ui.label("");
                        ui.checkbox(&mut self.cloud_form_path_style, t("cloud.path_style"))
                            .on_hover_text(t("cloud.path_style_hint"));
                        ui.end_row();
                        ui.label("");
                        ui.checkbox(&mut self.cloud_form_allow_http, t("cloud.allow_http"))
                            .on_hover_text(t("cloud.allow_http_hint"));
                        ui.end_row();
                    }
                    CloudKind::AzureBlob => {
                        ui.label(t("cloud.account"))
                            .on_hover_text(t("cloud.account_hint"));
                        ui.text_edit_singleline(&mut self.cloud_form_account)
                            .on_hover_text(t("cloud.account_hint"));
                        ui.end_row();
                        ui.label(t("db.field_oauth_client_id"))
                            .on_hover_text(t("db.field_oauth_client_id_hint"));
                        ui.text_edit_singleline(&mut self.cloud_form_oauth_client_id)
                            .on_hover_text(t("db.field_oauth_client_id_hint"));
                        ui.end_row();
                        ui.label(t("db.field_oauth_tenant"))
                            .on_hover_text(t("db.field_oauth_tenant_hint"));
                        ui.text_edit_singleline(&mut self.cloud_form_oauth_tenant)
                            .on_hover_text(t("db.field_oauth_tenant_hint"));
                        ui.end_row();
                        ui.label("");
                        ui.label(t("db.browser_signin_hint"));
                        ui.end_row();
                    }
                    CloudKind::Gcs => {
                        // Buckets in GCP belong to a project; for account-level
                        // listing let the user name the project (and optionally
                        // which gcloud identity) so several projects/accounts
                        // each get their own connection.
                        if self.cloud_form_account_level {
                            ui.label(t("cloud.gcs_project"))
                                .on_hover_text(t("cloud.gcs_project_hint"));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.cloud_form_project)
                                    .hint_text(t("cloud.gcs_project_hint")),
                            );
                            ui.end_row();
                            ui.label(t("cloud.gcs_account"))
                                .on_hover_text(t("cloud.gcs_account_hint"));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.cloud_form_account)
                                    .hint_text(t("cloud.gcs_account_hint")),
                            );
                            ui.end_row();
                        }
                        ui.label(t("db.field_oauth_client_id"))
                            .on_hover_text(t("db.field_oauth_client_id_hint"));
                        ui.text_edit_singleline(&mut self.cloud_form_oauth_client_id)
                            .on_hover_text(t("db.field_oauth_client_id_hint"));
                        ui.end_row();
                        ui.label("");
                        ui.label(t("db.browser_signin_hint"));
                        ui.end_row();
                    }
                }

                // Public / anonymous access applies to every provider.
                ui.label("");
                ui.checkbox(&mut self.cloud_form_anonymous, t("cloud.anonymous"))
                    .on_hover_text(t("cloud.anonymous_hint"));
                ui.end_row();

                // Per-connection write permission (AND-ed with the global
                // "Allow writing to cloud storage" switch above).
                ui.label("");
                ui.checkbox(
                    &mut self.cloud_form_allow_writes,
                    t("cloud.conn_allow_writes"),
                )
                .on_hover_text(t("cloud.conn_allow_writes_hint"));
                ui.end_row();
            });

        ui.horizontal(|ui| {
            if ui.button(t("cloud.save_connection")).clicked() {
                self.save_cloud_form();
            }
            if editing && ui.button(t("cloud.cancel_edit")).clicked() {
                self.clear_cloud_form();
            }
        });
        if let Some(msg) = &self.cloud_secret_status_msg {
            ui.colored_label(egui::Color32::from_rgb(0x30, 0x80, 0x30), msg);
        }
    }

    /// The "Sign in with browser" button + status for Azure Blob / GCS.
    fn cloud_browser_signin_controls(&mut self, ui: &mut egui::Ui) {
        // Drain a finished sign-in into the status line.
        if let Some(slot) = &self.cloud_signin_result
            && let Some(res) = slot.lock().ok().and_then(|mut g| g.take())
        {
            self.cloud_signin_msg = Some(match res {
                Ok(()) => (true, t("db.signed_in_browser")),
                Err(e) => (false, e),
            });
            self.cloud_signin_result = None;
        }
        ui.label(t("db.browser_signin_hint"));
        ui.horizontal(|ui| {
            let signing_in = self.cloud_signin_result.is_some();
            if ui
                .add_enabled(!signing_in, egui::Button::new(t("db.signin_button")))
                .on_hover_text(t("db.signin_button_hint"))
                .clicked()
            {
                self.start_cloud_browser_signin(ui.ctx().clone());
            }
            if signing_in {
                ui.spinner();
                ui.label(t("db.signin_needed"));
            }
        });
        if let Some((ok, msg)) = &self.cloud_signin_msg {
            let color = if *ok {
                egui::Color32::from_rgb(0x30, 0x80, 0x30)
            } else {
                egui::Color32::from_rgb(0xd9, 0x53, 0x4f)
            };
            ui.colored_label(color, msg);
        }
    }

    /// Run the cloud browser OAuth sign-in on a worker thread, caching the token
    /// so every cloud connect path uses it.
    fn start_cloud_browser_signin(&mut self, ctx: egui::Context) {
        let id = if self.cloud_form_id.is_empty() {
            format!("cloud-{}", self.cloud_form_name.trim())
        } else {
            self.cloud_form_id.clone()
        };
        let conn = self.form_cloud_connection(id.clone());
        // The Google client secret (GCS) lives in the secret buffer or the
        // stored keyring secret; Azure needs none.
        let typed = self.cloud_form_secret.trim();
        let client_secret = if !typed.is_empty() {
            Some(typed.to_string())
        } else {
            cloud_secrets::get_cloud_secret(&id, &self.draft)
                .and_then(|s| s.oauth_client_secret().map(str::to_string))
        };
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        self.cloud_signin_result = Some(slot.clone());
        self.cloud_signin_msg = None;
        std::thread::spawn(move || {
            let res =
                match crate::cloud::cloud_browser_oauth_config(&conn, client_secret.as_deref()) {
                    Some(cfg) => crate::auth::oauth_browser::acquire_token(
                        &cfg,
                        crate::auth::oauth_browser::open_url_in_browser,
                    )
                    .map(|token| crate::cloud::cache_cloud_browser_token(&conn.id, token))
                    .map_err(|e| format!("{e:#}")),
                    None => Err(t("db.browser_signin_hint")),
                };
            if let Ok(mut g) = slot.lock() {
                *g = Some(res);
            }
            ctx.request_repaint();
        });
    }

    /// Secret inputs + Save / armed Clear + storage-location label.
    fn cloud_secret_controls(&mut self, ui: &mut egui::Ui) {
        if self.cloud_form_kind == CloudKind::S3 {
            ui.horizontal(|ui| {
                ui.label(t("cloud.access_key_id"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.cloud_form_access_key_id)
                        .desired_width(220.0),
                );
            });
        } else if self.cloud_form_kind == CloudKind::AzureBlob {
            ui.checkbox(&mut self.cloud_form_azure_is_sas, t("cloud.azure_sas"))
                .on_hover_text(t("cloud.azure_sas_hint"));
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.cloud_form_secret)
                    .password(true)
                    .hint_text(t("cloud.secret"))
                    .desired_width(220.0),
            );
            // Save secret only makes sense for an already-saved connection; a
            // new connection stores its secret when the connection is saved.
            let editing = !self.cloud_form_id.is_empty();
            if editing
                && ui.button(t("cloud.secret_save")).clicked()
                && let Some(secret) = self.build_cloud_secret()
            {
                self.store_cloud_secret(&self.cloud_form_id.clone(), &secret);
                self.cloud_form_secret.clear();
            }
            if editing && ui.button(t("cloud.secret_clear")).clicked() {
                self.cloud_secret_clear_confirm = Some(self.cloud_form_id.clone());
                self.cloud_secret_status_msg = None;
            }
        });

        if self.cloud_secret_clear_confirm.as_deref() == Some(self.cloud_form_id.as_str())
            && !self.cloud_form_id.is_empty()
        {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(t("cloud.secret_clear_confirm"))
                        .color(egui::Color32::from_rgb(0xd9, 0x53, 0x4f)),
                );
                if ui.button(t("cloud.secret_clear_yes")).clicked() {
                    cloud_secrets::delete_cloud_secret(&self.cloud_form_id, &mut self.draft);
                    self.cloud_secret_status_msg = Some(t("cloud.secret_cleared"));
                    self.cloud_secret_clear_confirm = None;
                }
                if ui.button(t("cloud.secret_clear_cancel")).clicked() {
                    self.cloud_secret_clear_confirm = None;
                }
            });
        }

        if !self.cloud_form_id.is_empty() {
            let where_msg =
                match cloud_secrets::cloud_secret_storage(&self.cloud_form_id, &self.draft) {
                    KeyStorage::Env(var) => format!("{} {var}", t("chat.key_source_env")),
                    KeyStorage::Keyring => t("chat.key_source_keyring"),
                    KeyStorage::Plaintext(path) => {
                        format!("{} {}", t("chat.key_source_plaintext"), path.display())
                    }
                    KeyStorage::None => t("chat.key_source_none"),
                };
            ui.label(format!("{} {where_msg}", t("cloud.secret_storage")));
        }
    }

    /// Build a `CloudSecret` from the secret buffers, or `None` if empty.
    fn build_cloud_secret(&self) -> Option<CloudSecret> {
        let secret = self.cloud_form_secret.trim();
        if secret.is_empty() {
            return None;
        }
        match self.cloud_form_kind {
            CloudKind::S3 => {
                let id = self.cloud_form_access_key_id.trim();
                if id.is_empty() {
                    return None;
                }
                Some(CloudSecret::S3 {
                    access_key_id: id.to_string(),
                    secret_access_key: secret.to_string(),
                    token: None,
                })
            }
            CloudKind::AzureBlob => Some(if self.cloud_form_azure_is_sas {
                CloudSecret::AzureSas(secret.to_string())
            } else {
                CloudSecret::AzureKey(secret.to_string())
            }),
            // The GCS secret slot holds the Google OAuth client secret for the
            // browser sign-in fallback.
            CloudKind::Gcs => Some(CloudSecret::GcsOAuthClientSecret(secret.to_string())),
        }
    }

    /// Persist a secret, reporting where it landed.
    fn store_cloud_secret(&mut self, id: &str, secret: &CloudSecret) {
        match cloud_secrets::set_cloud_secret(id, secret, &mut self.draft) {
            Ok(true) => self.cloud_secret_status_msg = Some(t("cloud.secret_saved_keyring")),
            Ok(false) => {
                self.cloud_secret_status_msg = Some(t("cloud.secret_saved_plaintext"));
            }
            Err(e) => self.cloud_secret_status_msg = Some(e),
        }
    }

    /// Load an existing connection into the form (not its secret).
    fn load_cloud_form(&mut self, index: usize) {
        let Some(conn) = self.draft.cloud_connections.get(index) else {
            return;
        };
        self.cloud_editing = Some(index);
        self.cloud_form_id = conn.id.clone();
        self.cloud_form_name = conn.name.clone();
        self.cloud_form_kind = conn.kind;
        self.cloud_form_bucket = conn.bucket.clone();
        self.cloud_form_region = conn.region.clone().unwrap_or_default();
        self.cloud_form_endpoint = conn.endpoint.clone().unwrap_or_default();
        self.cloud_form_account = conn.account.clone().unwrap_or_default();
        self.cloud_form_profile = conn.profile.clone().unwrap_or_default();
        self.cloud_form_path_style = conn.force_path_style;
        self.cloud_form_allow_http = conn.allow_http;
        self.cloud_form_anonymous = conn.anonymous;
        self.cloud_form_allow_writes = conn.allow_writes;
        self.cloud_form_account_level = conn.account_level;
        self.cloud_form_prefix = conn.prefix.clone().unwrap_or_default();
        self.cloud_form_project = conn.project.clone().unwrap_or_default();
        self.cloud_form_oauth_client_id = conn.oauth_client_id.clone().unwrap_or_default();
        self.cloud_form_oauth_tenant = conn.oauth_tenant.clone().unwrap_or_default();
        self.cloud_form_access_key_id.clear();
        self.cloud_form_secret.clear();
        self.cloud_form_azure_is_sas = false;
        self.cloud_secret_status_msg = None;
        self.cloud_secret_clear_confirm = None;
    }

    /// Reset the form to "add a new connection".
    pub(super) fn clear_cloud_form(&mut self) {
        self.cloud_editing = None;
        self.cloud_form_id.clear();
        self.cloud_form_name.clear();
        self.cloud_form_kind = CloudKind::default();
        self.cloud_form_bucket.clear();
        self.cloud_form_region.clear();
        self.cloud_form_endpoint.clear();
        self.cloud_form_account.clear();
        self.cloud_form_profile.clear();
        self.cloud_form_path_style = false;
        self.cloud_form_allow_http = false;
        self.cloud_form_anonymous = false;
        self.cloud_form_allow_writes = false;
        self.cloud_form_account_level = false;
        self.cloud_form_prefix.clear();
        self.cloud_form_project.clear();
        self.cloud_form_access_key_id.clear();
        self.cloud_form_secret.clear();
        self.cloud_form_azure_is_sas = false;
        self.cloud_form_oauth_client_id.clear();
        self.cloud_form_oauth_tenant.clear();
    }

    /// Validate + upsert the form into `draft.cloud_connections`, storing any
    /// entered secret. Stable id: kept on edit, slugged from the name on add.
    /// Build a `CloudConnection` from the current form buffers.
    fn form_cloud_connection(&self, id: String) -> CloudConnection {
        let opt = |s: &str| {
            let t = s.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        CloudConnection {
            name: self.cloud_form_name.trim().to_string(),
            kind: self.cloud_form_kind,
            bucket: self.cloud_form_bucket.trim().to_string(),
            region: opt(&self.cloud_form_region),
            endpoint: opt(&self.cloud_form_endpoint),
            force_path_style: self.cloud_form_path_style,
            allow_http: self.cloud_form_allow_http,
            secret_ref: Some(id.clone()),
            account: opt(&self.cloud_form_account),
            profile: opt(&self.cloud_form_profile),
            anonymous: self.cloud_form_anonymous,
            allow_writes: self.cloud_form_allow_writes,
            prefix: {
                let p = self.cloud_form_prefix.trim().trim_end_matches('/');
                (!p.is_empty()).then(|| format!("{p}/"))
            },
            account_level: self.cloud_form_account_level,
            project: opt(&self.cloud_form_project),
            oauth_client_id: opt(&self.cloud_form_oauth_client_id),
            oauth_tenant: opt(&self.cloud_form_oauth_tenant),
            id,
        }
    }

    fn save_cloud_form(&mut self) {
        let name = self.cloud_form_name.trim().to_string();
        let bucket = self.cloud_form_bucket.trim().to_string();
        if name.is_empty() || (!self.cloud_form_account_level && bucket.is_empty()) {
            self.cloud_secret_status_msg = Some(t("cloud.need_name_bucket"));
            return;
        }
        let id = if self.cloud_form_id.is_empty() {
            self.fresh_connection_id(&name)
        } else {
            self.cloud_form_id.clone()
        };
        let conn = self.form_cloud_connection(id.clone());
        match self.draft.cloud_connections.iter().position(|c| c.id == id) {
            Some(i) => self.draft.cloud_connections[i] = conn,
            None => self.draft.cloud_connections.push(conn),
        }
        if let Some(secret) = self.build_cloud_secret() {
            self.store_cloud_secret(&id, &secret);
        } else {
            self.cloud_secret_status_msg = Some(t("cloud.connection_saved"));
        }
        self.clear_cloud_form();
    }

    /// A stable, keyring-safe id slugged from the name, de-duped against the
    /// existing connection ids.
    fn fresh_connection_id(&self, name: &str) -> String {
        let base: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let base = base.trim_matches('-');
        let base = if base.is_empty() { "connection" } else { base };
        let exists = |id: &str| self.draft.cloud_connections.iter().any(|c| c.id == id);
        if !exists(base) {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| !exists(id))
            .unwrap_or_else(|| base.to_string())
    }
}

/// ASCII provider label for combos and the connection list.
fn kind_label(kind: CloudKind) -> &'static str {
    match kind {
        CloudKind::S3 => "S3 / S3-compatible",
        CloudKind::AzureBlob => "Azure Blob",
        CloudKind::Gcs => "Google Cloud Storage",
    }
}
