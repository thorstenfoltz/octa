//! Chat side windows: the saved-prompts manager, mention autocomplete, and the session-history window. Split out of chat_panel/mod.rs.

use eframe::egui;

use octa::i18n::t;

use crate::app::chat::persist;
use crate::app::state::OctaApp;
use crate::ui::settings::{
    DialogSize, chat_profiles, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::helpers::tab_display_name;
use super::profile_api_key;

impl OctaApp {
    /// Insert a saved prompt's text into the input box (append on its own line
    /// when there's already text), then focus the input.
    pub(crate) fn insert_prompt_text(&mut self, text: String) {
        if self.chat.input.trim().is_empty() {
            self.chat.input = text;
        } else {
            if !self.chat.input.ends_with('\n') {
                self.chat.input.push('\n');
            }
            self.chat.input.push_str(&text);
        }
        self.chat.focus_input = true;
    }

    /// Saved-prompts manager window (standard min/max/close chrome): save the
    /// current input as a named prompt, insert a saved one, or delete one.
    /// Replaces the old dropdown menu (which jumped around on click).
    pub(crate) fn render_prompts_window(&mut self, ctx: &egui::Context) {
        if !self.chat.prompts_window_open {
            return;
        }
        let mut size = self.chat.prompts_window_size;
        let mut close_requested = false;
        let mut save_prompt = false;
        let mut insert_prompt: Option<String> = None;
        let mut delete_prompt: Option<String> = None;
        let can_save = !self.chat.input.trim().is_empty();

        let dialog_id = egui::Id::new("octa_chat_prompts_window_v1");
        let window = egui::Window::new(t("chat.prompts"))
            .id(dialog_id)
            .title_bar(false)
            .collapsible(false);
        let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
            w.resizable(true)
                .default_width(420.0)
                .default_height(440.0)
                .min_width(300.0)
                .min_height(180.0)
        });
        let minimized = size == DialogSize::Minimized;

        let inner = window.show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t("chat.prompts")).strong().size(15.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if draw_window_controls(ui, &mut size) {
                        close_requested = true;
                    }
                });
            });
            if minimized {
                return;
            }
            ui.separator();

            if ui
                .add_enabled(can_save, egui::Button::new(t("chat.prompt_save")))
                .on_hover_text(t("chat.prompts_hint"))
                .clicked()
            {
                save_prompt = true;
            }
            ui.separator();

            if self.chat_prompts.is_empty() {
                ui.label(
                    egui::RichText::new(t("chat.prompt_empty"))
                        .weak()
                        .size(12.0),
                );
                return;
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for prompt in &self.chat_prompts {
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .button("x")
                                        .on_hover_text(t("chat.prompt_delete"))
                                        .clicked()
                                    {
                                        delete_prompt = Some(prompt.name.clone());
                                    }
                                    if ui.button(t("chat.prompt_insert")).clicked() {
                                        insert_prompt = Some(prompt.text.clone());
                                    }
                                    ui.with_layout(
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            let hover = if prompt.description.is_empty() {
                                                prompt.text.clone()
                                            } else {
                                                format!("{}\n\n{}", prompt.description, prompt.text)
                                            };
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&prompt.name).strong(),
                                                )
                                                .truncate(),
                                            )
                                            .on_hover_text(hover);
                                        },
                                    );
                                },
                            );
                        });
                        if !prompt.description.is_empty() {
                            ui.label(egui::RichText::new(&prompt.description).weak().size(11.0));
                        }
                        ui.separator();
                    }
                });
        });

        if let Some(inner) = inner {
            remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
        }
        self.chat.prompts_window_size = size;

        if save_prompt {
            self.chat_prompt_save = Some(crate::app::state::ChatPromptDraft {
                name: String::new(),
                description: String::new(),
                text: self.chat.input.clone(),
            });
        }
        if let Some(text) = insert_prompt {
            self.insert_prompt_text(text);
        }
        if let Some(name) = delete_prompt {
            self.chat_prompts.retain(|p| p.name != name);
            crate::app::chat_prompts::save(&self.chat_prompts);
        }
        if close_requested {
            self.chat.prompts_window_open = false;
        }
    }

    /// Build the `@`-mention autocomplete list from the open tabs: one entry per
    /// non-chart tab (`#N name` -> inserts `@#N`) followed by every distinct
    /// column name (inserts the bare name). Returned as `(display, insert)`.
    pub(crate) fn build_mention_suggestions(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let mut n = 0usize;
        for (i, tab) in self.tabs.iter().enumerate() {
            if tab.is_chart_tab {
                continue;
            }
            n += 1;
            let name = tab_display_name(tab, i);
            out.push((format!("#{n} {name}"), format!("@#{n}")));
        }
        let mut seen = std::collections::HashSet::new();
        for tab in self.tabs.iter() {
            if tab.is_chart_tab {
                continue;
            }
            for col in &tab.table.columns {
                if seen.insert(col.name.clone()) {
                    out.push((col.name.clone(), col.name.clone()));
                }
            }
        }
        out
    }

    /// Whether the active profile can run: local providers (Ollama) are always
    /// ready; cloud providers need a resolvable API key (the profile's own, or
    /// the shared one for its provider).
    pub(crate) fn provider_ready(&self) -> bool {
        let profile = chat_profiles::active_profile(&self.settings);
        !profile.kind.needs_api_key() || profile_api_key(&profile, &self.settings).is_some()
    }

    /// The saved-session browser window (load / delete past chats).
    pub(crate) fn render_chat_history_window(&mut self, ctx: &egui::Context) {
        if !self.chat.session_list_open {
            return;
        }
        let mut to_load: Option<String> = None;
        let mut to_delete: Option<String> = None;
        let mut clear_all = false;
        let mut close_requested = false;
        let mut size = self.chat.session_list_size;

        let dialog_id = egui::Id::new("octa_chat_history_v2");
        let window = egui::Window::new(t("chat.history_title"))
            .id(dialog_id)
            .title_bar(false)
            .collapsible(false);
        let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
            w.resizable(true)
                .default_width(380.0)
                .default_height(440.0)
                .min_width(280.0)
                .min_height(180.0)
        });
        let minimized = size == DialogSize::Minimized;

        let inner = window.show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(t("chat.history_title"))
                        .strong()
                        .size(15.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if draw_window_controls(ui, &mut size) {
                        close_requested = true;
                    }
                });
            });
            if minimized {
                return;
            }
            ui.separator();

            let metas = persist::list();
            if metas.is_empty() {
                ui.label(t("chat.history_empty"));
                return;
            }
            ui.horizontal(|ui| {
                ui.label(format!("{} {}", metas.len(), t("chat.history_count")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t("chat.delete_all")).clicked() {
                        clear_all = true;
                    }
                });
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for m in metas {
                        ui.horizontal(|ui| {
                            let label = if m.title.trim().is_empty() {
                                t("chat.untitled")
                            } else {
                                m.title.clone()
                            };
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("x").on_hover_text(t("chat.delete")).clicked() {
                                        to_delete = Some(m.id.clone());
                                    }
                                    ui.with_layout(
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .add(
                                                    egui::Button::new(label)
                                                        .min_size(egui::vec2(
                                                            ui.available_width(),
                                                            0.0,
                                                        ))
                                                        .wrap(),
                                                )
                                                .clicked()
                                            {
                                                to_load = Some(m.id.clone());
                                            }
                                        },
                                    );
                                },
                            );
                        });
                    }
                });
        });

        if let Some(inner) = inner {
            remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
        }
        self.chat.session_list_size = size;

        if let Some(id) = to_load {
            self.load_chat_session(&id);
            self.chat.session_list_open = false;
        }
        if let Some(id) = to_delete {
            let _ = persist::delete(&id);
        }
        if clear_all {
            let removed = persist::delete_all();
            eprintln!("octa: cleared {removed} saved chat session(s)");
        }
        if close_requested {
            self.chat.session_list_open = false;
        }
    }
}
