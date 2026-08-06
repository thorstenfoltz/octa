//! "Report AI content" dialog. Microsoft Store policy requires a product that
//! presents generative-AI output to give users an in-product way to report
//! inappropriate output, and the assistant panel renders model text.
//!
//! Octa neither hosts nor trains a model - the user brings an API key or runs
//! Ollama locally - so a complaint about what the model *wrote* can only be
//! acted on by whoever serves that model. The dialog therefore routes content
//! reports outward to the active profile's provider, and keeps a second,
//! always-present button for the part Octa does own: its own bugs.

use eframe::egui;
use egui::RichText;

use octa::auth::oauth_browser::open_url_in_browser;
use octa::i18n::t;

use crate::ui::settings::{ChatProviderKind, chat_models, chat_profiles};

use super::super::state::OctaApp;

/// Prefilled new-issue URL. Percent-encoded by hand rather than pulling in a
/// URL-encoding helper for one constant.
const OCTA_ISSUE_URL: &str =
    "https://github.com/thorstenfoltz/octa/issues/new?title=AI%20content%20report";

pub(crate) fn render_ai_report_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_ai_report_dialog {
        return;
    }

    let profile = chat_profiles::active_profile(&app.settings);
    let kind = profile.kind;
    // Only the OpenAI-compatible branch below reads this, so it mirrors that
    // provider's own fallback in `providers::config_for_profile`: the global
    // `chat_base_url` when the profile carries no URL of its own. Naming any
    // other URL here would point the user at the wrong operator.
    let endpoint = if profile.base_url.trim().is_empty() {
        app.settings.chat_base_url.trim().to_string()
    } else {
        profile.base_url.trim().to_string()
    };

    let mut close = false;

    egui::Window::new(t("ai_report.title"))
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(420.0);
            ui.set_max_width(460.0);

            ui.label(t("ai_report.body"));
            ui.add_space(8.0);
            // A blank model field means the provider substitutes its default,
            // so name that rather than showing empty parentheses.
            let model = if profile.model.trim().is_empty() {
                chat_models::default_model(kind).to_string()
            } else {
                profile.model.clone()
            };
            ui.label(
                RichText::new(format!(
                    "{} {} ({model})",
                    t("ai_report.active"),
                    profile.name
                ))
                .weak(),
            );
            ui.add_space(10.0);

            // --- Content reports: outward, to whoever serves the model. ---
            match kind.report_url() {
                Some(url) => {
                    ui.label(t("ai_report.provider_body"));
                    ui.add_space(6.0);
                    if ui
                        .button(format!("{} {}", t("ai_report.report_to"), kind.label()))
                        .on_hover_text(url)
                        .clicked()
                    {
                        open_url_in_browser(url);
                    }
                }
                None => {
                    let body = if kind == ChatProviderKind::Ollama {
                        t("ai_report.local_body")
                    } else {
                        // Both URLs can be blank on a half-configured profile;
                        // trim so the sentence does not end in a dangling space.
                        format!("{} {endpoint}", t("ai_report.endpoint_body"))
                            .trim_end()
                            .to_string()
                    };
                    ui.label(body);
                }
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // --- Octa's own faults: the part that is actually fixable here. ---
            ui.label(t("ai_report.octa_body"));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .button(t("ai_report.report_octa"))
                    .on_hover_text(t("ai_report.report_octa_hint"))
                    .clicked()
                {
                    open_url_in_browser(OCTA_ISSUE_URL);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t("common.close")).clicked() {
                        close = true;
                    }
                });
            });
        });

    if close {
        app.show_ai_report_dialog = false;
    }
}
