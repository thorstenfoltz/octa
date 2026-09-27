//! File -> Open API endpoint...: pick a saved endpoint, optionally override
//! its path, open it as a tab.
//!
//! Deliberately small. Everything that makes an endpoint work - its host,
//! auth, records path and pagination - was decided once in
//! **Settings -> API endpoints**; this dialog only chooses which of those to
//! run and where. A person who has no endpoints saved is sent there rather
//! than asked to type a URL, because a URL typed here would have no
//! credentials and no paging anyway (`File > Open URL` already does that).

use octa::i18n::t;

use crate::app::state::OctaApp;
use octa::ui::settings::center_on_first_show;

/// What the dialog is holding while it is open.
#[derive(Default)]
pub(crate) struct ApiEndpointState {
    /// Id of the selected connection, empty until one is chosen.
    pub(crate) conn_id: String,
    /// Path override; empty means the connection's own.
    pub(crate) path: String,
}

impl OctaApp {
    pub(crate) fn open_api_dialog(&mut self) {
        let first = self
            .settings
            .api_connections
            .first()
            .map(|c| c.id.clone())
            .unwrap_or_default();
        self.api_endpoint_dialog = Some(ApiEndpointState {
            conn_id: first,
            path: String::new(),
        });
    }

    pub(crate) fn render_api_endpoint_dialog(&mut self, ctx: &egui::Context) {
        let Some(state) = self.api_endpoint_dialog.as_mut() else {
            return;
        };
        let mut open = true;
        let mut cancelled = false;
        let mut fetch: Option<(octa::api::ApiConnection, Option<String>)> = None;
        let mut go_to_settings = false;

        // Centred on first show rather than anchored: `Area::anchor` calls
        // `movable(false)`, so an anchored dialog cannot be dragged off the
        // data it is covering.
        let center = center_on_first_show(ctx, egui::vec2(420.0, 200.0));
        egui::Window::new(t("api.open_title"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(center)
            .show(ctx, |ui| {
                if self.settings.api_connections.is_empty() {
                    ui.label(t("api.open_none"));
                    ui.add_space(6.0);
                    if ui
                        .button(t("api.open_settings"))
                        .on_hover_text(t("api.open_settings_hint"))
                        .clicked()
                    {
                        go_to_settings = true;
                    }
                    return;
                }

                let selected = self
                    .settings
                    .api_connections
                    .iter()
                    .find(|c| c.id == state.conn_id)
                    .cloned();

                ui.horizontal(|ui| {
                    ui.label(t("api.open_endpoint"))
                        .on_hover_text(t("api.open_endpoint_hint"));
                    egui::ComboBox::from_id_salt("api_open_pick")
                        .selected_text(
                            selected
                                .as_ref()
                                .map(|c| c.name.clone())
                                .unwrap_or_else(|| t("api.open_pick")),
                        )
                        .show_ui(ui, |ui| {
                            for c in &self.settings.api_connections {
                                ui.selectable_value(&mut state.conn_id, c.id.clone(), &c.name);
                            }
                        })
                        .response
                        .on_hover_text(t("api.open_endpoint_hint"));
                });

                if let Some(c) = &selected {
                    ui.label(egui::RichText::new(c.url_for(None)).weak().small());
                }

                ui.horizontal(|ui| {
                    ui.label(t("api.open_path"))
                        .on_hover_text(t("api.open_path_hint"));
                    ui.add(
                        egui::TextEdit::singleline(&mut state.path)
                            .hint_text(
                                selected
                                    .as_ref()
                                    .map(|c| c.path.clone())
                                    .unwrap_or_default(),
                            )
                            .desired_width(260.0),
                    )
                    .on_hover_text(t("api.open_path_hint"));
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let ready = selected.is_some();
                    let open_btn = ui.add_enabled(ready, egui::Button::new(t("api.open_button")));
                    let open_btn = if ready {
                        open_btn.on_hover_text(t("api.open_button_hint"))
                    } else {
                        open_btn.on_disabled_hover_text(t("api.open_pick"))
                    };
                    if open_btn.clicked()
                        && let Some(c) = selected.clone()
                    {
                        let p = state.path.trim();
                        fetch = Some((c, (!p.is_empty()).then(|| p.to_string())));
                    }
                    if ui.button(t("api.open_cancel")).clicked() {
                        cancelled = true;
                    }
                });
            });

        if go_to_settings {
            self.api_endpoint_dialog = None;
            self.settings_dialog.open(&self.settings);
            self.settings_dialog.focus_api_section = true;
            return;
        }
        if let Some((conn, path)) = fetch {
            self.api_endpoint_dialog = None;
            self.start_api_fetch(conn, path, ctx);
            return;
        }
        if !open || cancelled {
            self.api_endpoint_dialog = None;
        }
    }
}
