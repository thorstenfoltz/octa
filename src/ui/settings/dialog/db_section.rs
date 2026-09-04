//! Databases section of the Settings dialog: the saved-connection list, an
//! add/edit form (engine, host, auth mode, per-connection "Allow writes"),
//! and secret management - mirroring `cloud_section.rs` field for field.
//!
//! All edits land in `self.draft` (committed on Apply); secrets are written
//! immediately through `db_secrets` (keyring, plaintext fallback in the
//! draft).

use eframe::egui;

use crate::db::ssh_tunnel::SshAuth;
use crate::db::{DEFAULT_QUERY_TIMEOUT_SECS, DbAuth, DbAuthKind, DbConnection, DbEngine};
use crate::i18n::t;
use crate::ui::settings::db_secrets;
use crate::ui::settings::secrets::KeyStorage;
use crate::ui::settings::{SecretPurge, SettingsDialog};

/// Short auth-mode label for the combo + list rows.
fn auth_label(auth: &DbAuth) -> String {
    t(&format!("db.{}", auth.kind().i18n_key()))
}

/// The auth kinds offered for an engine, in picker order (thin wrapper so the
/// form and its test share one source of truth).
fn auth_options(engine: DbEngine) -> &'static [DbAuthKind] {
    engine.supported_auth()
}

/// Trim a buffer, mapping empty to None.
fn trim_opt(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Whether an auth kind stores a secret in the keyring (so the form shows a
/// secret field + the Clear-secret controls).
fn auth_uses_secret(kind: DbAuthKind) -> bool {
    matches!(
        kind,
        DbAuthKind::Password
            | DbAuthKind::Token
            | DbAuthKind::KeyPairJwt
            | DbAuthKind::OAuthClientCredentials
            // GcpIam stores the optional Google OAuth client secret for the
            // browser sign-in fallback in the same secret slot.
            | DbAuthKind::GcpIam
    )
}

impl SettingsDialog {
    /// Body of the "Databases" Settings section.
    pub(super) fn db_section_body(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(t("settings.confirm_db_write_back"))
                .on_hover_text(t("settings_hint.confirm_db_write_back"));
            ui.checkbox(&mut self.draft.confirm_db_write_back, "")
                .on_hover_text(t("settings_hint.confirm_db_write_back"));
        });
        ui.separator();
        self.db_connection_list(ui);
        ui.separator();
        // The add/edit form lives behind its own sub-header so the section
        // opens as a readable connection list; clicking Edit on a row forces
        // the form open.
        let editing = !self.db_form_id.is_empty();
        egui::CollapsingHeader::new(egui::RichText::new(t("settings.sub_conn_form")).strong())
            .id_salt("settings_db_sub_form")
            .open(editing.then_some(true))
            .show(ui, |ui| {
                self.db_connection_form(ui);
            });
        ui.separator();
        self.db_history_settings(ui);
    }

    /// Query-history settings: whether to keep the queries you run, and how
    /// many. Lives with the connections because the history is scoped to them.
    fn db_history_settings(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new(t("db.history_title")).strong());
        let was_on = self.draft.sql_history_enabled;
        ui.checkbox(&mut self.draft.sql_history_enabled, t("db.history_enabled"))
            .on_hover_text(t("db.history_enabled_hint"));
        if was_on && !self.draft.sql_history_enabled {
            // Switching it off means "do not keep my queries", not merely
            // "stop adding to the pile". Queries can carry literals out of the
            // data, so leaving the old file behind would be a nasty surprise.
            crate::sql::history::forget_everything();
        }
        ui.add_enabled_ui(self.draft.sql_history_enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label(t("db.history_limit"))
                    .on_hover_text(t("db.history_limit_hint"));
                let mut buf = self.draft.sql_history_limit.to_string();
                if ui
                    .add(egui::TextEdit::singleline(&mut buf).desired_width(60.0))
                    .on_hover_text(t("db.history_limit_hint"))
                    .changed()
                    && let Ok(v) = buf.trim().parse::<usize>()
                {
                    self.draft.sql_history_limit = v;
                }
            });
        });
    }

    /// The saved-connection list with per-row Edit / Remove.
    fn db_connection_list(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new(t("db.connections")).strong());
        if self.draft.db_connections.is_empty() {
            ui.weak(t("db.no_connections"));
        }
        let mut remove_idx: Option<usize> = None;
        let mut edit_idx: Option<usize> = None;
        egui::Grid::new("db_conn_list")
            .num_columns(3)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for (i, conn) in self.draft.db_connections.iter().enumerate() {
                    let writes = if conn.allow_writes {
                        format!("  [{}]", t("db.writes_on"))
                    } else {
                        String::new()
                    };
                    let signed_in = if crate::db::auth::has_browser_token(&conn.id) {
                        format!("  [{}]", t("db.signed_in_browser"))
                    } else {
                        String::new()
                    };
                    ui.label(format!(
                        "{}  ({}: {}:{}/{}){}{}",
                        conn.name,
                        conn.engine.label(),
                        conn.host,
                        conn.port,
                        conn.database,
                        writes,
                        signed_in
                    ));
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
            && i < self.draft.db_connections.len()
        {
            let conn = self.draft.db_connections.remove(i);
            db_secrets::delete_db_secret(&conn.id, &mut self.draft);
            db_secrets::delete_ssh_secret(&conn.id, &mut self.draft);
            self.purge_secret(SecretPurge::Db(conn.id.clone()));
            if self.db_form_id == conn.id {
                self.clear_db_form();
            }
        }
        if let Some(i) = edit_idx {
            self.load_db_form(i);
        }
    }

    /// The add / edit connection form.
    fn db_connection_form(&mut self, ui: &mut egui::Ui) {
        // Drain a finished connection test into the status line.
        if let Some(slot) = &self.db_test_result
            && let Some(res) = slot.lock().ok().and_then(|mut g| g.take())
        {
            self.db_test_msg = Some(match res {
                Ok(()) => (true, t("db.test_ok")),
                Err(e) => (false, format!("{} {e}", t("db.test_failed"))),
            });
            self.db_test_result = None;
        }
        // Drain a finished browser sign-in into its status line.
        if let Some(slot) = &self.db_signin_result
            && let Some(res) = slot.lock().ok().and_then(|mut g| g.take())
        {
            self.db_signin_msg = Some(match res {
                Ok(()) => (true, t("db.signed_in_browser")),
                Err(e) => (false, e),
            });
            self.db_signin_result = None;
        }
        let editing = !self.db_form_id.is_empty();
        ui.label(
            egui::RichText::new(if editing {
                t("db.edit_connection")
            } else {
                t("db.add_connection")
            })
            .strong(),
        );

        egui::Grid::new("db_conn_form")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label(t("cloud.name")).on_hover_text(t("db.name_hint"));
                ui.text_edit_singleline(&mut self.db_form_name)
                    .on_hover_text(t("db.name_hint"));
                ui.end_row();

                ui.label(t("db.engine")).on_hover_text(t("db.engine_hint"));
                let prev_engine = self.db_form_engine;
                egui::ComboBox::from_id_salt("db_form_engine")
                    .selected_text(self.db_form_engine.label())
                    .show_ui(ui, |ui| {
                        for &e in DbEngine::ALL {
                            ui.selectable_value(&mut self.db_form_engine, e, e.label());
                        }
                    })
                    .response
                    .on_hover_text(t("db.engine_hint"));
                if self.db_form_engine != prev_engine {
                    // Auto-fill the conventional port unless the user typed
                    // a custom one.
                    let old_default = prev_engine.default_port().to_string();
                    if self.db_form_port.trim().is_empty() || self.db_form_port == old_default {
                        self.db_form_port = self.db_form_engine.default_port().to_string();
                    }
                    // Reset the auth mode when the new engine doesn't offer the
                    // current one (each engine advertises its own set).
                    let options = auth_options(self.db_form_engine);
                    if !options.contains(&self.db_form_auth.kind()) {
                        self.db_form_auth =
                            self.auth_from_kind(*options.first().unwrap_or(&DbAuthKind::Password));
                    }
                }
                ui.end_row();

                ui.label(t("db.host")).on_hover_text(t("db.host_hint"));
                ui.text_edit_singleline(&mut self.db_form_host)
                    .on_hover_text(t("db.host_hint"));
                ui.end_row();

                ui.label(t("db.port")).on_hover_text(t("db.port_hint"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.db_form_port)
                        .desired_width(80.0)
                        .hint_text(self.db_form_engine.default_port().to_string()),
                )
                .on_hover_text(t("db.port_hint"));
                ui.end_row();

                ui.label(t("db.database"))
                    .on_hover_text(t("db.database_hint"));
                ui.text_edit_singleline(&mut self.db_form_database)
                    .on_hover_text(t("db.database_hint"));
                ui.end_row();

                ui.label(t("db.username"))
                    .on_hover_text(t("db.username_hint"));
                ui.text_edit_singleline(&mut self.db_form_username)
                    .on_hover_text(t("db.username_hint"));
                ui.end_row();

                ui.label(t("db.auth")).on_hover_text(t("db.auth_hint"));
                let mut selected_kind = self.db_form_auth.kind();
                egui::ComboBox::from_id_salt("db_form_auth")
                    .selected_text(auth_label(&self.db_form_auth))
                    .show_ui(ui, |ui| {
                        for &kind in auth_options(self.db_form_engine) {
                            ui.selectable_value(
                                &mut selected_kind,
                                kind,
                                t(&format!("db.{}", kind.i18n_key())),
                            );
                        }
                    })
                    .response
                    .on_hover_text(t("db.auth_hint"));
                if selected_kind != self.db_form_auth.kind() {
                    self.db_form_auth = self.auth_from_kind(selected_kind);
                }
                ui.end_row();

                // Every auth field is labelled and hinted the same way. The
                // hint goes on BOTH the label and the field: a tooltip on the
                // label alone does not answer a hover over the widget beside
                // it, which is what a user actually points at.
                let text_field_h =
                    |ui: &mut egui::Ui, buf: &mut String, label: String, hint: String| {
                        ui.label(label).on_hover_text(hint.clone());
                        ui.add(egui::TextEdit::singleline(buf).desired_width(260.0))
                            .on_hover_text(hint);
                        ui.end_row();
                    };
                let secret_field_h =
                    |ui: &mut egui::Ui, buf: &mut String, label: String, hint: String| {
                        ui.label(label).on_hover_text(hint.clone());
                        ui.add(
                            egui::TextEdit::singleline(buf)
                                .password(true)
                                .desired_width(220.0),
                        )
                        .on_hover_text(hint);
                        ui.end_row();
                    };
                let note = |ui: &mut egui::Ui, msg: String| {
                    ui.label("");
                    ui.label(msg);
                    ui.end_row();
                };
                // Same labelled-and-hinted field, but stacked rather than in a
                // grid row: the Advanced body is its own little column, not
                // part of the form grid, so it must not call `end_row`.
                let field_v = |ui: &mut egui::Ui, buf: &mut String, label: String, hint: String| {
                    ui.label(label).on_hover_text(hint.clone());
                    ui.add(egui::TextEdit::singleline(buf).desired_width(260.0))
                        .on_hover_text(hint);
                };
                let secret_v =
                    |ui: &mut egui::Ui, buf: &mut String, label: String, hint: String| {
                        ui.label(label).on_hover_text(hint.clone());
                        ui.add(
                            egui::TextEdit::singleline(buf)
                                .password(true)
                                .desired_width(220.0),
                        )
                        .on_hover_text(hint);
                    };
                let advanced =
                    |ui: &mut egui::Ui, id: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
                        egui::CollapsingHeader::new(t("db.advanced"))
                            .id_salt(id)
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.label(t("db.advanced_hint"));
                                body(ui);
                            })
                            .header_response
                            .on_hover_text(t("db.advanced_hint"));
                    };
                match self.db_form_auth.kind() {
                    DbAuthKind::Password => {
                        secret_field_h(
                            ui,
                            &mut self.db_form_secret,
                            t("db.password"),
                            t("db.password_hint"),
                        );
                    }
                    DbAuthKind::AwsIam => {
                        ui.label(t("cloud.region"))
                            .on_hover_text(t("db.aws_region_hint"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.db_form_region)
                                .hint_text(t("db.aws_region_hint")),
                        )
                        .on_hover_text(t("db.aws_region_hint"));
                        ui.end_row();
                        note(ui, t("db.aws_iam_note"));
                        note(ui, t("db.aws_sso_note"));
                        text_field_h(
                            ui,
                            &mut self.db_form_sso_start_url,
                            t("db.field_sso_start_url"),
                            t("db.field_sso_start_url_hint"),
                        );
                        text_field_h(
                            ui,
                            &mut self.db_form_sso_region,
                            t("db.field_sso_region"),
                            t("db.field_sso_region_hint"),
                        );
                        text_field_h(
                            ui,
                            &mut self.db_form_sso_account,
                            t("db.field_sso_account"),
                            t("db.field_sso_account_hint"),
                        );
                        text_field_h(
                            ui,
                            &mut self.db_form_sso_role,
                            t("db.field_sso_role"),
                            t("db.field_sso_role_hint"),
                        );
                    }
                    DbAuthKind::AzureAd => {
                        note(ui, t("db.azure_ad_note"));
                        note(ui, t("db.browser_signin_hint"));
                        // The client id and tenant are collapsed away. Shown as
                        // plain fields they read as required, and the one thing
                        // a user cannot supply on their own is a client id -
                        // which is exactly what made people give up here. The
                        // CLI sign-in below needs neither.
                        ui.label("");
                        advanced(ui, "db_adv_azure", &mut |ui: &mut egui::Ui| {
                            field_v(
                                ui,
                                &mut self.db_form_oauth_client_id,
                                t("db.field_oauth_client_id"),
                                t("db.field_oauth_client_id_hint"),
                            );
                            field_v(
                                ui,
                                &mut self.db_form_oauth_tenant,
                                t("db.field_oauth_tenant"),
                                t("db.field_oauth_tenant_hint"),
                            );
                        });
                        ui.end_row();
                    }
                    DbAuthKind::GcpIam => {
                        note(ui, t("db.gcp_iam_note"));
                        note(ui, t("db.browser_signin_hint"));
                        ui.label("");
                        advanced(ui, "db_adv_gcp", &mut |ui: &mut egui::Ui| {
                            field_v(
                                ui,
                                &mut self.db_form_oauth_client_id,
                                t("db.field_oauth_client_id"),
                                t("db.field_oauth_client_id_hint"),
                            );
                            secret_v(
                                ui,
                                &mut self.db_form_secret,
                                t("db.field_oauth_client_secret"),
                                t("db.field_oauth_client_secret_hint"),
                            );
                        });
                        ui.end_row();
                    }
                    DbAuthKind::Token => {
                        secret_field_h(
                            ui,
                            &mut self.db_form_secret,
                            t("db.field_pat"),
                            t("db.password_hint"),
                        );
                    }
                    DbAuthKind::KeyPairJwt => {
                        text_field_h(
                            ui,
                            &mut self.db_form_private_key,
                            t("db.field_private_key"),
                            t("db.field_private_key_hint"),
                        );
                        secret_field_h(
                            ui,
                            &mut self.db_form_secret,
                            t("db.field_passphrase"),
                            t("db.password_hint"),
                        );
                    }
                    DbAuthKind::OAuthClientCredentials => {
                        text_field_h(
                            ui,
                            &mut self.db_form_client_id,
                            t("db.field_client_id"),
                            t("db.field_client_id_hint"),
                        );
                        text_field_h(
                            ui,
                            &mut self.db_form_token_url,
                            t("db.field_token_url"),
                            t("db.field_token_url_hint"),
                        );
                        secret_field_h(
                            ui,
                            &mut self.db_form_secret,
                            t("db.field_client_secret"),
                            t("db.password_hint"),
                        );
                    }
                    DbAuthKind::OAuthBrowser => note(ui, t("db.auth_oauth_browser_note")),
                    DbAuthKind::GcpAdc => note(ui, t("db.auth_gcp_adc_note")),
                    DbAuthKind::GcpServiceAccount => {
                        text_field_h(
                            ui,
                            &mut self.db_form_sa_key,
                            t("db.field_sa_key"),
                            t("db.field_sa_key_hint"),
                        );
                    }
                }

                ui.label("");
                ui.checkbox(&mut self.db_form_allow_writes, t("db.allow_writes"))
                    .on_hover_text(t("db.allow_writes_hint"));
                ui.end_row();

                // Jump host. Collapsed and off by default: most connections are
                // direct, and the section is only interesting to the people who
                // today have to open a tunnel in a terminal first.
                ui.label("");
                egui::CollapsingHeader::new(t("db.ssh_section"))
                    .id_salt("db_ssh_section")
                    .default_open(self.db_form_ssh_on)
                    .show(ui, |ui| {
                        ui.checkbox(&mut self.db_form_ssh_on, t("db.ssh_enable"))
                            .on_hover_text(t("db.ssh_enable_hint"));
                        if !self.db_form_ssh_on {
                            return;
                        }
                        ui.add_space(4.0);
                        field_v(
                            ui,
                            &mut self.db_form_ssh_host,
                            t("db.ssh_host"),
                            t("db.ssh_host_hint"),
                        );
                        field_v(
                            ui,
                            &mut self.db_form_ssh_port,
                            t("db.ssh_port"),
                            t("db.ssh_port_hint"),
                        );
                        field_v(
                            ui,
                            &mut self.db_form_ssh_user,
                            t("db.ssh_user"),
                            t("db.ssh_user_hint"),
                        );

                        ui.label(t("db.ssh_auth"))
                            .on_hover_text(t("db.ssh_auth_hint"));
                        let current = self.db_form_ssh_auth.clone();
                        egui::ComboBox::from_id_salt("db_ssh_auth")
                            .selected_text(t(&format!("db.{}", current.i18n_key())))
                            .show_ui(ui, |ui| {
                                for option in [
                                    SshAuth::Agent,
                                    SshAuth::PrivateKey {
                                        path: String::new(),
                                    },
                                    SshAuth::Password,
                                ] {
                                    let label = t(&format!("db.{}", option.i18n_key()));
                                    let selected = std::mem::discriminant(&current)
                                        == std::mem::discriminant(&option);
                                    if ui.selectable_label(selected, label).clicked() {
                                        self.db_form_ssh_auth = option;
                                    }
                                }
                            })
                            .response
                            .on_hover_text(t("db.ssh_auth_hint"));

                        match &self.db_form_ssh_auth {
                            SshAuth::Agent => {
                                ui.label(t("db.ssh_agent_note"));
                            }
                            SshAuth::PrivateKey { .. } => {
                                field_v(
                                    ui,
                                    &mut self.db_form_ssh_key_path,
                                    t("db.ssh_key_path"),
                                    t("db.ssh_key_path_hint"),
                                );
                                secret_v(
                                    ui,
                                    &mut self.db_form_ssh_secret,
                                    t("db.ssh_passphrase"),
                                    t("db.ssh_passphrase_hint"),
                                );
                            }
                            SshAuth::Password => {
                                secret_v(
                                    ui,
                                    &mut self.db_form_ssh_secret,
                                    t("db.ssh_password"),
                                    t("db.ssh_password_hint"),
                                );
                            }
                        }
                        // The key path lives in the enum, so keep the variant in
                        // step with the text field the user is typing into.
                        if let SshAuth::PrivateKey { path } = &mut self.db_form_ssh_auth {
                            *path = self.db_form_ssh_key_path.trim().to_string();
                        }

                        ui.add_space(4.0);
                        ui.checkbox(&mut self.db_form_ssh_accept_new, t("db.ssh_accept_new"))
                            .on_hover_text(t("db.ssh_accept_new_hint"));
                    })
                    .header_response
                    .on_hover_text(t("db.ssh_section_hint"));
                ui.end_row();
            });

        ui.horizontal(|ui| {
            if ui.button(t("cloud.save_connection")).clicked() {
                self.save_db_form();
            }
            let testing = self.db_test_result.is_some();
            if ui
                .add_enabled(!testing, egui::Button::new(t("db.test_connection")))
                .clicked()
            {
                self.start_db_test(ui.ctx().clone());
            }
            if testing {
                ui.spinner();
                ui.label(t("db.test_running"));
            }
            if editing && ui.button(t("cloud.cancel_edit")).clicked() {
                self.clear_db_form();
            }
        });
        if self.db_form_engine.polls_for_results() {
            // These engines submit a statement and then ask the server whether
            // it has finished yet, so how long to keep asking is Octa's call
            // and belongs to the connection, not to one global number: a
            // warehouse that cold-starts needs minutes where a Trino cluster
            // answers in seconds.
            egui::Grid::new("db_conn_form_timeout")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label(t("db.query_timeout"))
                        .on_hover_text(t("db.query_timeout_hint"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.db_form_query_timeout)
                            .desired_width(80.0)
                            .hint_text(DEFAULT_QUERY_TIMEOUT_SECS.to_string()),
                    )
                    .on_hover_text(t("db.query_timeout_hint"));
                    ui.end_row();
                });
        }
        if self.db_form_engine == DbEngine::Athena {
            // Engine fields, not auth fields: Athena needs them whichever
            // credentials sign the request.
            egui::Grid::new("db_conn_form_athena")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    let field =
                        |ui: &mut egui::Ui, buf: &mut String, label: String, hint: String| {
                            ui.label(label).on_hover_text(hint.clone());
                            ui.add(egui::TextEdit::singleline(buf).desired_width(260.0))
                                .on_hover_text(hint);
                            ui.end_row();
                        };
                    field(
                        ui,
                        &mut self.db_form_athena_workgroup,
                        t("db.athena_workgroup"),
                        t("db.athena_workgroup_hint"),
                    );
                    field(
                        ui,
                        &mut self.db_form_athena_output,
                        t("db.athena_output"),
                        t("db.athena_output_hint"),
                    );
                });
        }
        if let Some((ok, msg)) = &self.db_test_msg {
            // Own row + wrapped: a driver's connection error is long, and the
            // useful half is at the end.
            crate::ui::settings::draw_result_message(ui, *ok, msg);
        }

        // Browser sign-in row: shown for Azure AD / GCP IAM once a client id is
        // set. Signs in and caches a token that every DB connect path then uses.
        let kind = self.db_form_auth.kind();
        // Always offered for Azure AD / GCP IAM. It used to appear only once a
        // client id was typed, which hid the very button that makes a client id
        // unnecessary: without one, sign-in runs the vendor CLI's own login.
        let aad_gcp = matches!(kind, DbAuthKind::AzureAd | DbAuthKind::GcpIam);
        // Databricks user-to-machine browser OAuth (default `databricks-cli`
        // client, so no client id required).
        let databricks_browser =
            kind == DbAuthKind::OAuthBrowser && self.db_form_engine == DbEngine::Databricks;
        // AWS IAM Identity Center: eligible once a portal start URL is set.
        let aws_sso = kind == DbAuthKind::AwsIam && !self.db_form_sso_start_url.trim().is_empty();
        let signin_eligible = aad_gcp || databricks_browser || aws_sso;
        if signin_eligible {
            ui.horizontal(|ui| {
                let signing_in = self.db_signin_result.is_some();
                if ui
                    .add_enabled(!signing_in, egui::Button::new(t("db.signin_button")))
                    .on_hover_text(t("db.signin_button_hint"))
                    .clicked()
                {
                    self.start_db_browser_signin(ui.ctx().clone());
                }
                if signing_in {
                    ui.spinner();
                    ui.label(t("db.signin_needed"));
                } else {
                    // What state is this connection actually in? Without this
                    // the only way to find out was to try connecting.
                    let conn_id = self.db_form_id.as_str();
                    match crate::db::auth::browser_token_minutes_left(conn_id) {
                        Some(mins) => {
                            ui.label(t("db.signin_status_in").replace("{n}", &mins.to_string()));
                            if ui
                                .button(t("db.signout_button"))
                                .on_hover_text(t("db.signout_button_hint"))
                                .clicked()
                            {
                                crate::db::auth::forget_browser_token(conn_id);
                                self.db_signin_msg = None;
                            }
                        }
                        None => {
                            ui.label(t("db.signin_status_out"));
                        }
                    }
                }
            });
            if let Some((ok, msg)) = &self.db_signin_msg {
                let color = if *ok {
                    egui::Color32::from_rgb(0x30, 0x80, 0x30)
                } else {
                    egui::Color32::from_rgb(0xd9, 0x53, 0x4f)
                };
                crate::ui::message::selectable_message(ui, color, msg);
            }
        }

        // Clear-secret with the armed-confirm guard (secret-bearing auth only).
        if editing && auth_uses_secret(self.db_form_auth.kind()) {
            ui.horizontal(|ui| {
                if ui.button(t("cloud.secret_clear")).clicked() {
                    self.db_secret_clear_confirm = Some(self.db_form_id.clone());
                    self.db_secret_status_msg = None;
                }
                let where_msg = match db_secrets::db_secret_storage(&self.db_form_id, &self.draft) {
                    KeyStorage::Env(var) => format!("{} {var}", t("chat.key_source_env")),
                    KeyStorage::Keyring => t("chat.key_source_keyring"),
                    KeyStorage::Plaintext(path) => {
                        format!("{} {}", t("chat.key_source_plaintext"), path.display())
                    }
                    KeyStorage::None => t("chat.key_source_none"),
                };
                ui.label(format!("{} {where_msg}", t("cloud.secret_storage")));
            });
            if self.db_secret_clear_confirm.as_deref() == Some(self.db_form_id.as_str()) {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(t("cloud.secret_clear_confirm"))
                            .color(egui::Color32::from_rgb(0xd9, 0x53, 0x4f)),
                    );
                    if ui.button(t("cloud.secret_clear_yes")).clicked() {
                        db_secrets::delete_db_secret(&self.db_form_id, &mut self.draft);
                        // The bastion credential is part of this connection's
                        // stored secrets: clearing one and leaving the other
                        // behind would be a surprise.
                        db_secrets::delete_ssh_secret(&self.db_form_id, &mut self.draft);
                        self.purge_secret(SecretPurge::Db(self.db_form_id.clone()));
                        self.db_secret_status_msg = Some(t("cloud.secret_cleared"));
                        self.db_secret_clear_confirm = None;
                    }
                    if ui.button(t("cloud.secret_clear_cancel")).clicked() {
                        self.db_secret_clear_confirm = None;
                    }
                });
            }
        }
        if let Some(msg) = &self.db_secret_status_msg {
            crate::ui::message::selectable_message(
                ui,
                egui::Color32::from_rgb(0x30, 0x80, 0x30),
                msg,
            );
        }
    }

    /// Build the `DbAuth` for a kind from the current form buffers.
    fn auth_from_kind(&self, kind: DbAuthKind) -> DbAuth {
        let opt = |s: &str| {
            let t = s.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        match kind {
            DbAuthKind::Password => DbAuth::Password,
            DbAuthKind::AwsIam => DbAuth::AwsIam {
                region: opt(&self.db_form_region),
                sso_start_url: opt(&self.db_form_sso_start_url),
                sso_region: opt(&self.db_form_sso_region),
                sso_account_id: opt(&self.db_form_sso_account),
                sso_role: opt(&self.db_form_sso_role),
            },
            DbAuthKind::AzureAd => DbAuth::AzureAd,
            DbAuthKind::GcpIam => DbAuth::GcpIam,
            DbAuthKind::Token => DbAuth::Token,
            DbAuthKind::KeyPairJwt => DbAuth::KeyPairJwt {
                private_key_path: self.db_form_private_key.trim().to_string(),
            },
            DbAuthKind::OAuthClientCredentials => DbAuth::OAuthClientCredentials {
                client_id: self.db_form_client_id.trim().to_string(),
                token_url: opt(&self.db_form_token_url),
            },
            DbAuthKind::OAuthBrowser => DbAuth::OAuthBrowser,
            DbAuthKind::GcpAdc => DbAuth::GcpAdc,
            DbAuthKind::GcpServiceAccount => DbAuth::GcpServiceAccount {
                key_path: self.db_form_sa_key.trim().to_string(),
            },
        }
    }

    /// Build a `DbConnection` from the current form buffers (no validation).
    fn form_connection(&self, id: String) -> DbConnection {
        let port: u16 = self
            .db_form_port
            .trim()
            .parse()
            .unwrap_or_else(|_| self.db_form_engine.default_port());
        let auth = self.auth_from_kind(self.db_form_auth.kind());
        DbConnection {
            id,
            name: self.db_form_name.trim().to_string(),
            engine: self.db_form_engine,
            host: self.db_form_host.trim().to_string(),
            port,
            database: self.db_form_database.trim().to_string(),
            username: self.db_form_username.trim().to_string(),
            auth,
            allow_writes: self.db_form_allow_writes,
            oauth_client_id: trim_opt(&self.db_form_oauth_client_id),
            oauth_tenant: trim_opt(&self.db_form_oauth_tenant),
            athena_workgroup: trim_opt(&self.db_form_athena_workgroup),
            athena_output_location: trim_opt(&self.db_form_athena_output),
            query_timeout_secs: self
                .db_form_query_timeout
                .trim()
                .parse()
                .unwrap_or(DEFAULT_QUERY_TIMEOUT_SECS)
                .max(1),
            ssh: self.ssh_from_form(),
            // A live tunnel's port; never comes from the form.
            tunnel_port: None,
        }
    }

    /// The jump-host settings from the form, or `None` when the tunnel is
    /// switched off. A tunnel with no host is `None` too: half-filled settings
    /// would fail at connect time rather than at save time.
    fn ssh_from_form(&self) -> Option<crate::db::ssh_tunnel::SshTunnel> {
        use crate::db::ssh_tunnel::SshTunnel;
        if !self.db_form_ssh_on || self.db_form_ssh_host.trim().is_empty() {
            return None;
        }
        Some(SshTunnel {
            host: self.db_form_ssh_host.trim().to_string(),
            port: self.db_form_ssh_port.trim().parse().unwrap_or(22),
            username: self.db_form_ssh_user.trim().to_string(),
            auth: self.db_form_ssh_auth.clone(),
            accept_new_host_key: self.db_form_ssh_accept_new,
        })
    }

    /// Spawn a worker that connects with the CURRENT form values (saved or
    /// not) and runs `SELECT 1`. Deliberately a fresh connection every time:
    /// exercising the real handshake is the point of the button. Token auth
    /// shells out to a CLI, so this must stay off the UI thread.
    fn start_db_test(&mut self, ctx: egui::Context) {
        let conn = self.form_connection(if self.db_form_id.is_empty() {
            "test".to_string()
        } else {
            self.db_form_id.clone()
        });
        let typed = self.db_form_secret.trim();
        let secret = if typed.is_empty() {
            db_secrets::get_db_secret(&self.db_form_id, &self.draft)
        } else {
            Some(typed.to_string())
        };
        // Same rule for the bastion credential: whatever is typed in the form
        // wins, so Test connection exercises the values on screen rather than
        // the ones last saved.
        let typed_ssh = self.db_form_ssh_secret.trim();
        let ssh_secret = if typed_ssh.is_empty() {
            db_secrets::get_ssh_secret(&self.db_form_id, &self.draft)
        } else {
            Some(typed_ssh.to_string())
        };
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        self.db_test_result = Some(slot.clone());
        self.db_test_msg = None;
        std::thread::spawn(move || {
            let res = crate::db::connect(&conn, secret.as_deref(), ssh_secret.as_deref())
                .and_then(|mut c| c.query("SELECT 1").map(|_| ()))
                .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = slot.lock() {
                *g = Some(res);
            }
            ctx.request_repaint();
        });
    }

    /// Run the browser OAuth sign-in for the current form's Azure AD / GCP IAM
    /// connection on a worker thread, caching the token on success so every DB
    /// connect path uses it.
    fn start_db_browser_signin(&mut self, ctx: egui::Context) {
        // A saved connection keeps its frozen id; a new one gets a temporary id
        // so the token cache still keys consistently once saved (the user should
        // save first, but signing in early still works for a test).
        let id = if self.db_form_id.is_empty() {
            DbConnection::fresh_id()
        } else {
            self.db_form_id.clone()
        };
        let conn = self.form_connection(id.clone());
        // The Google client secret (if any) lives in the secret buffer or the
        // stored keyring secret.
        let typed = self.db_form_secret.trim();
        let client_secret = if typed.is_empty() {
            db_secrets::get_db_secret(&id, &self.draft)
        } else {
            Some(typed.to_string())
        };
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        self.db_signin_result = Some(slot.clone());
        self.db_signin_msg = None;
        std::thread::spawn(move || {
            let res = if crate::db::auth::aws_sso_configured(&conn) {
                // AWS Identity Center uses the device-authorization flow, not
                // the generic PKCE loopback config.
                crate::db::auth::aws_sso_signin(
                    &conn,
                    crate::auth::oauth_browser::open_url_in_browser,
                )
                .map(|token| crate::db::auth::cache_browser_token(&conn.id, token))
                .map_err(|e| format!("{e:#}"))
            } else {
                match crate::db::auth::browser_oauth_config(&conn, client_secret.as_deref()) {
                    // A client id is configured: use our own browser flow, which
                    // needs no command-line tool at all. Someone who went to the
                    // trouble of registering an app gets to use it.
                    Some(cfg) => crate::auth::oauth_browser::acquire_token(
                        &cfg,
                        crate::auth::oauth_browser::open_url_in_browser,
                    )
                    .map(|token| crate::db::auth::cache_browser_token(&conn.id, token))
                    .map_err(|e| format!("{e:#}")),
                    // No client id, which is the normal case: hand the browser
                    // round-trip to the vendor's own CLI. It is a registered
                    // OAuth application already, so nobody has to supply an id.
                    None => {
                        crate::db::auth::cli_browser_signin(&conn).map_err(|e| format!("{e:#}"))
                    }
                }
            };
            if let Ok(mut g) = slot.lock() {
                *g = Some(res);
            }
            ctx.request_repaint();
        });
    }

    /// Load an existing connection into the form (not its secret).
    fn load_db_form(&mut self, index: usize) {
        let Some(conn) = self.draft.db_connections.get(index) else {
            return;
        };
        self.db_form_id = conn.id.clone();
        self.db_form_name = conn.name.clone();
        self.db_form_engine = conn.engine;
        self.db_form_host = conn.host.clone();
        self.db_form_port = conn.port.to_string();
        self.db_form_database = conn.database.clone();
        self.db_form_query_timeout = conn.query_timeout_secs.to_string();
        self.db_form_athena_workgroup = conn.athena_workgroup.clone().unwrap_or_default();
        self.db_form_athena_output = conn.athena_output_location.clone().unwrap_or_default();
        self.db_form_username = conn.username.clone();
        self.db_form_auth = conn.auth.clone();
        self.db_form_region = match &conn.auth {
            DbAuth::AwsIam { region, .. } => region.clone().unwrap_or_default(),
            _ => String::new(),
        };
        (
            self.db_form_sso_start_url,
            self.db_form_sso_region,
            self.db_form_sso_account,
            self.db_form_sso_role,
        ) = match &conn.auth {
            DbAuth::AwsIam {
                sso_start_url,
                sso_region,
                sso_account_id,
                sso_role,
                ..
            } => (
                sso_start_url.clone().unwrap_or_default(),
                sso_region.clone().unwrap_or_default(),
                sso_account_id.clone().unwrap_or_default(),
                sso_role.clone().unwrap_or_default(),
            ),
            _ => Default::default(),
        };
        self.db_form_private_key = match &conn.auth {
            DbAuth::KeyPairJwt { private_key_path } => private_key_path.clone(),
            _ => String::new(),
        };
        (self.db_form_client_id, self.db_form_token_url) = match &conn.auth {
            DbAuth::OAuthClientCredentials {
                client_id,
                token_url,
            } => (client_id.clone(), token_url.clone().unwrap_or_default()),
            _ => (String::new(), String::new()),
        };
        self.db_form_sa_key = match &conn.auth {
            DbAuth::GcpServiceAccount { key_path } => key_path.clone(),
            _ => String::new(),
        };
        self.db_form_oauth_client_id = conn.oauth_client_id.clone().unwrap_or_default();
        self.db_form_oauth_tenant = conn.oauth_tenant.clone().unwrap_or_default();
        self.db_form_allow_writes = conn.allow_writes;
        match conn.ssh.as_ref() {
            Some(t) => {
                self.db_form_ssh_on = true;
                self.db_form_ssh_host = t.host.clone();
                self.db_form_ssh_port = t.port.to_string();
                self.db_form_ssh_user = t.username.clone();
                self.db_form_ssh_auth = t.auth.clone();
                self.db_form_ssh_key_path = match &t.auth {
                    crate::db::ssh_tunnel::SshAuth::PrivateKey { path } => path.clone(),
                    _ => String::new(),
                };
                self.db_form_ssh_accept_new = t.accept_new_host_key;
            }
            None => {
                self.db_form_ssh_on = false;
                self.db_form_ssh_host.clear();
                self.db_form_ssh_port = "22".to_string();
                self.db_form_ssh_user.clear();
                self.db_form_ssh_auth = crate::db::ssh_tunnel::SshAuth::Agent;
                self.db_form_ssh_key_path.clear();
                self.db_form_ssh_accept_new = false;
            }
        }
        // Never round-trips out of the keyring, same as the database secret.
        self.db_form_ssh_secret.clear();
        self.db_form_secret.clear();
        self.db_secret_status_msg = None;
        self.db_secret_clear_confirm = None;
        self.db_test_result = None;
        self.db_test_msg = None;
    }

    /// Reset the form to "add a new connection".
    pub(super) fn clear_db_form(&mut self) {
        self.db_form_id.clear();
        self.db_form_name.clear();
        self.db_form_engine = DbEngine::Postgres;
        self.db_form_host.clear();
        self.db_form_port = DbEngine::Postgres.default_port().to_string();
        self.db_form_database.clear();
        self.db_form_query_timeout = DEFAULT_QUERY_TIMEOUT_SECS.to_string();
        self.db_form_athena_workgroup.clear();
        self.db_form_athena_output.clear();
        self.db_form_username.clear();
        self.db_form_auth = DbAuth::Password;
        self.db_form_region.clear();
        self.db_form_sso_start_url.clear();
        self.db_form_sso_region.clear();
        self.db_form_sso_account.clear();
        self.db_form_sso_role.clear();
        self.db_form_private_key.clear();
        self.db_form_client_id.clear();
        self.db_form_token_url.clear();
        self.db_form_sa_key.clear();
        self.db_form_oauth_client_id.clear();
        self.db_form_oauth_tenant.clear();
        self.db_form_allow_writes = false;
        self.db_form_ssh_on = false;
        self.db_form_ssh_host.clear();
        self.db_form_ssh_port = "22".to_string();
        self.db_form_ssh_user.clear();
        self.db_form_ssh_auth = crate::db::ssh_tunnel::SshAuth::Agent;
        self.db_form_ssh_key_path.clear();
        self.db_form_ssh_secret.clear();
        self.db_form_ssh_accept_new = false;
        self.db_form_secret.clear();
        self.db_secret_status_msg = None;
        self.db_secret_clear_confirm = None;
        self.db_test_result = None;
        self.db_test_msg = None;
    }

    /// Validate + upsert the form into `draft.db_connections`, storing any
    /// entered password. Stable id: kept on edit, minted fresh on add.
    fn save_db_form(&mut self) {
        let name = self.db_form_name.trim().to_string();
        let host = self.db_form_host.trim().to_string();
        let database = self.db_form_database.trim().to_string();
        if name.is_empty() || host.is_empty() || database.is_empty() {
            self.db_secret_status_msg = Some(t("db.need_fields"));
            return;
        }
        let id = if self.db_form_id.is_empty() {
            DbConnection::fresh_id()
        } else {
            self.db_form_id.clone()
        };
        let conn = self.form_connection(id.clone());
        match self.draft.db_connections.iter().position(|c| c.id == id) {
            Some(i) => self.draft.db_connections[i] = conn,
            None => self.draft.db_connections.push(conn),
        }
        // The bastion credential is a second, separate keyring entry, so a
        // connection can carry both a database password and an SSH one.
        let ssh_secret = self.db_form_ssh_secret.trim().to_string();
        if !ssh_secret.is_empty() {
            let _ = db_secrets::set_ssh_secret(&id, &ssh_secret, &mut self.draft);
        } else if !self.db_form_ssh_on {
            // The tunnel was switched off: do not leave its credential behind.
            db_secrets::delete_ssh_secret(&id, &mut self.draft);
        }
        let secret = self.db_form_secret.trim().to_string();
        if !secret.is_empty() {
            match db_secrets::set_db_secret(&id, &secret, &mut self.draft) {
                Ok(true) => {
                    self.db_secret_status_msg = Some(t("cloud.secret_saved_keyring"));
                }
                Ok(false) => {
                    self.db_secret_status_msg = Some(t("cloud.secret_saved_plaintext"));
                }
                Err(e) => self.db_secret_status_msg = Some(e),
            }
        } else {
            self.db_secret_status_msg = Some(t("cloud.connection_saved"));
        }
        self.clear_db_form();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_options_follow_engine() {
        for &e in DbEngine::ALL {
            assert_eq!(auth_options(e), e.supported_auth(), "{e:?}");
        }
    }

    #[test]
    fn secret_bearing_kinds() {
        assert!(auth_uses_secret(DbAuthKind::Token));
        assert!(auth_uses_secret(DbAuthKind::OAuthClientCredentials));
        assert!(!auth_uses_secret(DbAuthKind::GcpAdc));
        assert!(!auth_uses_secret(DbAuthKind::OAuthBrowser));
    }
}
