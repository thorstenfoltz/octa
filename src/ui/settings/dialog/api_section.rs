//! Settings -> API endpoints: the saved REST/JSON sources.
//!
//! The list + form + Test skeleton of [`super::db_section`], with the fields
//! an endpoint needs instead of a server's. Much smaller, because an endpoint
//! has no schemas, no catalogues and no write path: this source is read-only.
//!
//! Test probes page one and reports what it found, which is also what fills
//! the records-array picker - so "does this work" and "which array holds the
//! rows" are one request, not two.

use crate::api::{ApiAuth, ApiAuthKind, ApiConnection, ApiPaging, ApiPagingKind};
use crate::i18n::t;
use crate::ui::settings::api_secrets;
use crate::ui::settings::app_settings::{ApiProbe, ApiTestSlot};
use crate::ui::settings::secrets::KeyStorage;

use super::{SecretPurge, SettingsDialog};

impl SettingsDialog {
    pub(super) fn api_section_body(&mut self, ui: &mut egui::Ui) {
        self.drain_api_test();
        ui.label(t("api.intro"));
        ui.add_space(4.0);
        self.api_connection_list(ui);
        ui.separator();
        self.api_connection_form(ui);
    }

    /// Saved endpoints, with Edit and Remove.
    fn api_connection_list(&mut self, ui: &mut egui::Ui) {
        if self.draft.api_connections.is_empty() {
            ui.label(t("api.none_yet"));
            return;
        }
        let mut edit: Option<usize> = None;
        let mut remove: Option<usize> = None;
        for (i, c) in self.draft.api_connections.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&c.name).strong());
                ui.label(egui::RichText::new(&c.base_url).weak());
                ui.label(
                    egui::RichText::new(t(ApiAuthKind::of(&c.auth).i18n_key()))
                        .weak()
                        .small(),
                );
                if ui
                    .button(t("api.edit"))
                    .on_hover_text(t("api.edit_hint"))
                    .clicked()
                {
                    edit = Some(i);
                }
                if ui
                    .button(t("api.remove"))
                    .on_hover_text(t("api.remove_hint"))
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = edit {
            self.load_api_form(i);
        }
        if let Some(i) = remove {
            let c = self.draft.api_connections.remove(i);
            // Take the credential with it: a keyring entry nothing references
            // is a secret nobody can find to delete later.
            api_secrets::delete_api_secret(&c.id, &mut self.draft);
            self.purge_secret(SecretPurge::Api(c.id.clone()));
            if self.api_form_id == c.id {
                self.clear_api_form();
            }
        }
    }

    fn load_api_form(&mut self, idx: usize) {
        let Some(c) = self.draft.api_connections.get(idx).cloned() else {
            return;
        };
        self.api_form_id = c.id.clone();
        self.api_form_name = c.name;
        self.api_form_base_url = c.base_url;
        self.api_form_path = c.path;
        self.api_form_auth = ApiAuthKind::of(&c.auth);
        self.api_form_auth_param = match &c.auth {
            ApiAuth::HeaderKey { name } | ApiAuth::QueryKey { name } => name.clone(),
            _ => String::new(),
        };
        self.api_form_username = match &c.auth {
            ApiAuth::Basic { username } => username.clone(),
            _ => String::new(),
        };
        self.api_form_headers = c
            .headers
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n");
        self.api_form_records = c.records_pointer;
        self.api_form_paging = ApiPagingKind::of(&c.paging);
        let (page_param, limit_param, cursor_pointer) = match &c.paging {
            ApiPaging::PageNumber { param, .. } => (param.clone(), String::new(), String::new()),
            ApiPaging::OffsetLimit {
                offset_param,
                limit_param,
            } => (offset_param.clone(), limit_param.clone(), String::new()),
            ApiPaging::Cursor { pointer, param } => (param.clone(), String::new(), pointer.clone()),
            _ => (String::new(), String::new(), String::new()),
        };
        self.api_form_page_param = page_param;
        self.api_form_limit_param = limit_param;
        self.api_form_cursor_pointer = cursor_pointer;
        self.api_form_page_size = c.page_size.map(|n| n.to_string()).unwrap_or_default();
        self.api_form_timeout = c.timeout_secs.to_string();
        // The secret is never read back out of the keyring into the form: an
        // empty box means "leave it alone", which is what Save does.
        self.api_form_secret.clear();
        self.api_test_msg = None;
    }

    fn clear_api_form(&mut self) {
        self.api_form_id.clear();
        self.api_form_name.clear();
        self.api_form_base_url.clear();
        self.api_form_path.clear();
        self.api_form_auth = ApiAuthKind::None;
        self.api_form_auth_param.clear();
        self.api_form_username.clear();
        self.api_form_secret.clear();
        self.api_form_headers.clear();
        self.api_form_records.clear();
        self.api_form_paging = ApiPagingKind::None;
        self.api_form_page_param.clear();
        self.api_form_limit_param.clear();
        self.api_form_cursor_pointer.clear();
        self.api_form_page_size.clear();
        self.api_form_timeout.clear();
        self.api_test_msg = None;
    }

    /// The form's current state as a connection.
    fn form_api_connection(&self, id: String) -> ApiConnection {
        let auth = match self.api_form_auth {
            ApiAuthKind::None => ApiAuth::None,
            ApiAuthKind::Bearer => ApiAuth::Bearer,
            ApiAuthKind::HeaderKey => ApiAuth::HeaderKey {
                name: self.api_form_auth_param.trim().to_string(),
            },
            ApiAuthKind::QueryKey => ApiAuth::QueryKey {
                name: self.api_form_auth_param.trim().to_string(),
            },
            ApiAuthKind::Basic => ApiAuth::Basic {
                username: self.api_form_username.trim().to_string(),
            },
        };
        let paging = match self.api_form_paging {
            ApiPagingKind::None => ApiPaging::None,
            ApiPagingKind::PageNumber => ApiPaging::PageNumber {
                param: non_empty(&self.api_form_page_param, "page"),
                start: 1,
            },
            ApiPagingKind::OffsetLimit => ApiPaging::OffsetLimit {
                offset_param: non_empty(&self.api_form_page_param, "offset"),
                limit_param: non_empty(&self.api_form_limit_param, "limit"),
            },
            ApiPagingKind::Cursor => ApiPaging::Cursor {
                pointer: self.api_form_cursor_pointer.trim().to_string(),
                param: non_empty(&self.api_form_page_param, "cursor"),
            },
            ApiPagingKind::LinkHeader => ApiPaging::LinkHeader,
        };
        ApiConnection {
            id,
            name: self.api_form_name.trim().to_string(),
            base_url: self.api_form_base_url.trim().to_string(),
            path: self.api_form_path.trim().to_string(),
            auth,
            headers: parse_headers(&self.api_form_headers),
            records_pointer: self.api_form_records.trim().to_string(),
            paging,
            page_size: self.api_form_page_size.trim().parse().ok(),
            timeout_secs: self.api_form_timeout.trim().parse().unwrap_or(30),
        }
    }

    fn api_connection_form(&mut self, ui: &mut egui::Ui) {
        let editing = !self.api_form_id.is_empty();
        ui.label(egui::RichText::new(if editing {
            t("api.form_edit")
        } else {
            t("api.form_new")
        }))
        .on_hover_text(t("api.form_hint"));

        // Both the label and the field carry the hint: a tooltip on the label
        // alone does not answer a hover over the widget beside it.
        let field = |ui: &mut egui::Ui, buf: &mut String, label: String, hint: String| {
            ui.label(label).on_hover_text(hint.clone());
            ui.add(egui::TextEdit::singleline(buf).desired_width(320.0))
                .on_hover_text(hint);
            ui.end_row();
        };

        egui::Grid::new("api_form_grid")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                field(
                    ui,
                    &mut self.api_form_name,
                    t("api.name"),
                    t("api.name_hint"),
                );
                field(
                    ui,
                    &mut self.api_form_base_url,
                    t("api.base_url"),
                    t("api.base_url_hint"),
                );
                field(
                    ui,
                    &mut self.api_form_path,
                    t("api.path"),
                    t("api.path_hint"),
                );

                // --- auth ---
                ui.label(t("api.auth")).on_hover_text(t("api.auth_hint"));
                egui::ComboBox::from_id_salt("api_form_auth")
                    .selected_text(t(self.api_form_auth.i18n_key()))
                    .show_ui(ui, |ui| {
                        for &k in ApiAuthKind::ALL {
                            ui.selectable_value(&mut self.api_form_auth, k, t(k.i18n_key()));
                        }
                    })
                    .response
                    .on_hover_text(t("api.auth_hint"));
                ui.end_row();

                match self.api_form_auth {
                    ApiAuthKind::HeaderKey => field(
                        ui,
                        &mut self.api_form_auth_param,
                        t("api.header_name"),
                        t("api.header_name_hint"),
                    ),
                    ApiAuthKind::QueryKey => field(
                        ui,
                        &mut self.api_form_auth_param,
                        t("api.query_name"),
                        t("api.query_name_hint"),
                    ),
                    ApiAuthKind::Basic => field(
                        ui,
                        &mut self.api_form_username,
                        t("api.username"),
                        t("api.username_hint"),
                    ),
                    _ => {}
                }

                if self.api_form_auth.needs_secret() {
                    ui.label(t("api.secret"))
                        .on_hover_text(t("api.secret_hint"));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.api_form_secret)
                                .password(true)
                                .desired_width(220.0),
                        )
                        .on_hover_text(t("api.secret_hint"));
                        if editing {
                            let stored =
                                api_secrets::api_secret_storage(&self.api_form_id, &self.draft);
                            match stored {
                                KeyStorage::Keyring => {
                                    ui.label(
                                        egui::RichText::new(t("api.secret_in_keyring"))
                                            .weak()
                                            .small(),
                                    );
                                }
                                KeyStorage::Plaintext(_) => {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(0xc0, 0x80, 0x20),
                                        t("api.secret_plaintext"),
                                    )
                                    .on_hover_text(t("api.secret_plaintext_hint"));
                                }
                                // `Env` cannot occur: nothing reads an API
                                // credential from the environment, and the
                                // arm exists so a new storage kind is a
                                // compile error rather than a silent blank.
                                KeyStorage::Env(_) | KeyStorage::None => {}
                            }
                        }
                    });
                    ui.end_row();
                }

                ui.label(t("api.headers"))
                    .on_hover_text(t("api.headers_hint"));
                ui.add(
                    egui::TextEdit::multiline(&mut self.api_form_headers)
                        .desired_rows(2)
                        .desired_width(320.0)
                        .hint_text("Accept: application/json"),
                )
                .on_hover_text(t("api.headers_hint"));
                ui.end_row();

                // --- records ---
                field(
                    ui,
                    &mut self.api_form_records,
                    t("api.records"),
                    t("api.records_hint"),
                );

                // --- paging ---
                ui.label(t("api.paging"))
                    .on_hover_text(t("api.paging_hint"));
                egui::ComboBox::from_id_salt("api_form_paging")
                    .selected_text(t(self.api_form_paging.i18n_key()))
                    .show_ui(ui, |ui| {
                        for &k in ApiPagingKind::ALL {
                            ui.selectable_value(&mut self.api_form_paging, k, t(k.i18n_key()));
                        }
                    })
                    .response
                    .on_hover_text(t("api.paging_hint"));
                ui.end_row();

                match self.api_form_paging {
                    ApiPagingKind::PageNumber => field(
                        ui,
                        &mut self.api_form_page_param,
                        t("api.page_param"),
                        t("api.page_param_hint"),
                    ),
                    ApiPagingKind::OffsetLimit => {
                        field(
                            ui,
                            &mut self.api_form_page_param,
                            t("api.offset_param"),
                            t("api.offset_param_hint"),
                        );
                        field(
                            ui,
                            &mut self.api_form_limit_param,
                            t("api.limit_param"),
                            t("api.limit_param_hint"),
                        );
                    }
                    ApiPagingKind::Cursor => {
                        field(
                            ui,
                            &mut self.api_form_cursor_pointer,
                            t("api.cursor_pointer"),
                            t("api.cursor_pointer_hint"),
                        );
                        field(
                            ui,
                            &mut self.api_form_page_param,
                            t("api.cursor_param"),
                            t("api.cursor_param_hint"),
                        );
                    }
                    _ => {}
                }
                if !matches!(self.api_form_paging, ApiPagingKind::None) {
                    field(
                        ui,
                        &mut self.api_form_page_size,
                        t("api.page_size"),
                        t("api.page_size_hint"),
                    );
                }

                field(
                    ui,
                    &mut self.api_form_timeout,
                    t("api.timeout"),
                    t("api.timeout_hint"),
                );
            });

        // Candidates the last Test found, so the records path is picked, not
        // typed. Only shown when there is more than the obvious answer.
        let candidates: Vec<String> = self.api_last_candidates.clone();
        if candidates.len() > 1 {
            ui.horizontal_wrapped(|ui| {
                ui.label(t("api.records_found"));
                for c in candidates {
                    let label = if c.is_empty() {
                        t("api.records_root")
                    } else {
                        c.clone()
                    };
                    if ui
                        .selectable_label(self.api_form_records == c, label)
                        .on_hover_text(t("api.records_pick_hint"))
                        .clicked()
                    {
                        self.api_form_records = c.clone();
                    }
                }
            });
        }

        ui.add_space(4.0);
        self.api_form_buttons(ui, editing);
    }

    fn api_form_buttons(&mut self, ui: &mut egui::Ui, editing: bool) {
        let ready =
            !self.api_form_name.trim().is_empty() && !self.api_form_base_url.trim().is_empty();
        let testing = self.api_test_result.is_some();

        ui.horizontal(|ui| {
            let save = ui.add_enabled(ready, egui::Button::new(t("api.save")));
            let save = if ready {
                save.on_hover_text(t("api.save_hint"))
            } else {
                save.on_disabled_hover_text(t("api.save_disabled_hint"))
            };
            if save.clicked() {
                self.save_api_form();
            }

            let test = ui.add_enabled(ready && !testing, egui::Button::new(t("api.test")));
            let test = if testing {
                test.on_disabled_hover_text(t("api.test_running_hint"))
            } else if ready {
                test.on_hover_text(t("api.test_hint"))
            } else {
                test.on_disabled_hover_text(t("api.save_disabled_hint"))
            };
            if test.clicked() {
                self.start_api_test(ui.ctx().clone());
            }
            if testing {
                ui.add(egui::Spinner::new().size(14.0));
            }

            if editing
                && ui
                    .button(t("api.new"))
                    .on_hover_text(t("api.new_hint"))
                    .clicked()
            {
                self.clear_api_form();
            }
        });

        if let Some((ok, msg)) = &self.api_test_msg {
            let color = if *ok {
                egui::Color32::from_rgb(0x30, 0x90, 0x50)
            } else {
                egui::Color32::from_rgb(0xc0, 0x30, 0x30)
            };
            ui.colored_label(color, msg);
        }
    }

    fn save_api_form(&mut self) {
        let id = if self.api_form_id.is_empty() {
            ApiConnection::fresh_id()
        } else {
            self.api_form_id.clone()
        };
        let conn = self.form_api_connection(id.clone());

        // Switching to a mode that needs no credential drops the stored one:
        // otherwise the keyring keeps an entry nothing references any more,
        // and there is no other way to get rid of it short of deleting the
        // connection.
        if !self.api_form_auth.needs_secret() {
            api_secrets::delete_api_secret(&id, &mut self.draft);
            self.api_form_secret.clear();
        }
        // An empty secret box means "leave what is stored alone", so editing a
        // connection's path never silently wipes its credential.
        if !self.api_form_secret.trim().is_empty() {
            let _ = api_secrets::set_api_secret(&id, self.api_form_secret.trim(), &mut self.draft);
            self.api_form_secret.clear();
        }

        match self.draft.api_connections.iter_mut().find(|c| c.id == id) {
            Some(slot) => *slot = conn,
            None => self.draft.api_connections.push(conn),
        }
        self.api_form_id = id;
        self.api_test_msg = Some((true, t("api.saved")));
    }

    /// Probe page one on a worker thread. Same throwaway-connection shape as
    /// the database Test button: nothing is saved, and the UI thread never
    /// waits on a socket.
    fn start_api_test(&mut self, ctx: egui::Context) {
        let conn = self.form_api_connection(self.api_form_id.clone());
        // A connection being tested before it is saved has no keyring entry
        // yet, so the typed secret is used when there is one.
        let secret = if self.api_form_secret.trim().is_empty() {
            api_secrets::get_api_secret(&self.api_form_id, &self.draft)
        } else {
            Some(self.api_form_secret.trim().to_string())
        };
        let slot: ApiTestSlot = std::sync::Arc::new(std::sync::Mutex::new(None));
        self.api_test_result = Some(slot.clone());
        self.api_test_msg = None;

        std::thread::spawn(move || {
            let outcome = crate::api::client::fetch_first_page(&conn, secret.as_deref(), None)
                .map(|body| {
                    let candidates = crate::api::records::candidate_record_paths(&body);
                    let rows = crate::api::records::records_at(&body, &conn.records_pointer)
                        .map(|v| crate::api::records::row_count(&v))
                        .unwrap_or(0);
                    let columns = crate::api::records::records_at(&body, &conn.records_pointer)
                        .and_then(|v| {
                            crate::formats::json_reader::json_to_table(
                                v,
                                std::path::Path::new("probe"),
                                "JSON API",
                            )
                            .ok()
                        })
                        .map(|t| t.columns.iter().map(|c| c.name.clone()).collect())
                        .unwrap_or_default();
                    ApiProbe {
                        rows,
                        candidates,
                        columns,
                    }
                })
                .map_err(|e| format!("{e:#}"));
            *slot.lock().unwrap() = Some(outcome);
            ctx.request_repaint();
        });
    }

    fn drain_api_test(&mut self) {
        let Some(slot) = self.api_test_result.clone() else {
            return;
        };
        let taken = slot.lock().unwrap().take();
        let Some(result) = taken else { return };
        self.api_test_result = None;
        match result {
            Ok(probe) => {
                self.api_last_candidates = probe.candidates;
                let cols = if probe.columns.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", probe.columns.join(", "))
                };
                self.api_test_msg = Some((
                    true,
                    t("api.test_ok")
                        .replace("{n}", &probe.rows.to_string())
                        .replace("{columns}", &cols),
                ));
            }
            Err(msg) => {
                self.api_last_candidates.clear();
                self.api_test_msg = Some((false, msg));
            }
        }
    }
}

/// `Name: value` lines into pairs. Blank lines and lines with no colon are
/// dropped rather than sent as a malformed header.
fn parse_headers(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

/// The user's value, or a sensible default when they left the box empty.
fn non_empty(s: &str, fallback: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        fallback.to_string()
    } else {
        t.to_string()
    }
}
