//! Chat panel header controls: profile-model pick, tab-context chips, and the Ollama server row (probe/start/stop). Split out of chat_panel/mod.rs.

use eframe::egui;
use std::sync::atomic::Ordering;
use std::time::Instant;

use octa::i18n::t;

use crate::app::chat::ollama;
use crate::app::state::OctaApp;
use crate::ui::settings::ChatProviderKind;

use super::helpers::tab_display_name;

impl OctaApp {
    pub(crate) fn set_active_profile_model(&mut self, model: String) {
        let id = self.settings.chat_active_profile.clone();
        if let Some(p) = self
            .settings
            .chat_profiles
            .iter_mut()
            .find(|p| p.id == id && p.model != model)
        {
            p.model = model;
            self.settings.save();
        }
    }

    /// A chip row of the open tabs the assistant can see, active one
    /// highlighted, so the user knows what is in context and can insert an
    /// `@#N` reference to target a specific tab (handy when names repeat). The
    /// handles match `build_tool_context`'s numbering (non-chart tabs in order).
    pub(crate) fn render_tab_context_chips(&mut self, ui: &mut egui::Ui) {
        // Collect (handle, label, is_active) for non-chart tabs.
        let mut chips: Vec<(String, String, bool)> = Vec::new();
        let mut n = 0usize;
        for (i, tab) in self.tabs.iter().enumerate() {
            if tab.is_chart_tab {
                continue;
            }
            n += 1;
            let handle = format!("#{n}");
            let name = tab_display_name(tab, i);
            chips.push((handle, name, i == self.active_tab));
        }
        if chips.is_empty() {
            return;
        }
        let mut insert: Option<String> = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(t("chat.context_label"))
                    .weak()
                    .size(11.0),
            );
            for (handle, name, active) in &chips {
                let label = format!("{handle} {name}");
                if ui
                    .selectable_label(*active, egui::RichText::new(label).size(11.0))
                    .on_hover_text(t("chat.context_chip_hint"))
                    .clicked()
                {
                    insert = Some(handle.clone());
                }
            }
        });
        if let Some(handle) = insert {
            if !self.chat.input.is_empty() && !self.chat.input.ends_with(' ') {
                self.chat.input.push(' ');
            }
            self.chat.input.push('@');
            self.chat.input.push_str(&handle);
            self.chat.input.push(' ');
            self.chat.focus_input = true;
        }
    }

    /// The Ollama model picker: a dropdown of locally-installed models plus
    /// Refresh / Start controls and a running-status chip.
    pub(crate) fn render_ollama_model_row(&mut self, ui: &mut egui::Ui, current: &str) {
        let (models, running, checked, error) = {
            let s = self.chat.ollama.shared.lock().unwrap();
            (s.models.clone(), s.running, s.checked, s.error.clone())
        };
        let busy = self.chat.ollama.busy.load(Ordering::Relaxed);
        let mut model = current.to_string();

        if models.is_empty() {
            // No models discovered yet: free-text plus the discovery hint below.
            let resp = ui.add(
                egui::TextEdit::singleline(&mut model)
                    .desired_width(140.0)
                    .hint_text(octa::ui::settings::chat_models::default_model(
                        ChatProviderKind::Ollama,
                    )),
            );
            if resp.changed() {
                self.set_active_profile_model(model.clone());
            }
        } else {
            // The installed-model list is discovered from the running server,
            // which the Settings form cannot do, so this picker stays in the
            // panel and writes the choice back into the active profile.
            egui::ComboBox::from_id_salt("octa_chat_ollama_model")
                .selected_text(if model.is_empty() {
                    "(pick)".to_string()
                } else {
                    model.clone()
                })
                .show_ui(ui, |ui| {
                    for m in &models {
                        if ui.selectable_label(&model == m, m).clicked() {
                            model = m.clone();
                        }
                    }
                });
            if model != current {
                self.set_active_profile_model(model.clone());
            }
        }

        if busy {
            ui.add(egui::Spinner::new().size(14.0));
        } else {
            if ui.button(t("chat.ollama_refresh")).clicked() {
                self.refresh_ollama(ui.ctx());
            }
            if checked && !running && ui.button(t("chat.ollama_start")).clicked() {
                self.start_ollama(ui.ctx());
            }
            // Offer Stop whenever a local server is up (Octa-owned or not) - the
            // button is an explicit request to stop it.
            let can_stop = self.chat.ollama.octa_owns_server()
                || (running && ollama::is_local_url(&self.settings.chat_ollama_url));
            if can_stop && ui.button(t("chat.ollama_stop")).clicked() {
                self.stop_ollama(ui.ctx());
            }
        }

        // Status / hint line.
        if let Some(err) = error {
            ui.colored_label(egui::Color32::from_rgb(0xc0, 0x40, 0x20), err);
        } else if checked {
            if running {
                if models.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(0xc0, 0x60, 0x10),
                        t("chat.ollama_no_models"),
                    );
                } else {
                    ui.colored_label(
                        egui::Color32::from_rgb(0x30, 0x90, 0x50),
                        t("chat.ollama_status_running"),
                    );
                }
            } else {
                ui.colored_label(
                    egui::Color32::from_rgb(0xc0, 0x60, 0x10),
                    t("chat.ollama_status_stopped"),
                );
            }
        }
    }

    /// Probe the Ollama server + model list on a worker thread.
    pub(crate) fn refresh_ollama(&self, ctx: &egui::Context) {
        if self.chat.ollama.busy.swap(true, Ordering::Relaxed) {
            return; // already probing
        }
        let base = self.settings.chat_ollama_url.clone();
        let shared = self.chat.ollama.shared.clone();
        let busy = self.chat.ollama.busy.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let running = ollama::is_running(&base);
            let (models, error) = if running {
                match ollama::list_models(&base) {
                    Ok(m) => (m, None),
                    Err(e) => (Vec::new(), Some(e)),
                }
            } else {
                (Vec::new(), None)
            };
            {
                let mut s = shared.lock().unwrap();
                s.running = running;
                s.models = models;
                s.checked = true;
                s.error = error;
            }
            busy.store(false, Ordering::Relaxed);
            ctx.request_repaint();
        });
    }

    /// Start `ollama serve` in the background, then re-probe.
    pub(crate) fn start_ollama(&self, ctx: &egui::Context) {
        if self.chat.ollama.busy.swap(true, Ordering::Relaxed) {
            return;
        }
        let base = self.settings.chat_ollama_url.clone();
        let shared = self.chat.ollama.shared.clone();
        let busy = self.chat.ollama.busy.clone();
        let server_child = self.chat.ollama.server_child.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let (mut child, start_err) = match ollama::start_server() {
                Ok(child) => (Some(child), None),
                Err(e) => (None, Some(e)),
            };
            // Give the server a moment to bind, then probe a few times.
            let mut running = false;
            for _ in 0..6 {
                std::thread::sleep(std::time::Duration::from_millis(500));
                if ollama::is_running(&base) {
                    running = true;
                    break;
                }
            }
            // If our spawned process already exited (e.g. the port was in use
            // because a server was already running), we do NOT own the live
            // server - drop the dead handle so Stop / on-exit don't think they
            // can kill it.
            if let Some(c) = child.as_mut()
                && matches!(c.try_wait(), Ok(Some(_)))
            {
                child = None;
            }
            *server_child.lock().unwrap() = child;

            let (models, error) = if running {
                match ollama::list_models(&base) {
                    Ok(m) => (m, None),
                    Err(e) => (Vec::new(), Some(e)),
                }
            } else {
                (Vec::new(), start_err)
            };
            {
                let mut s = shared.lock().unwrap();
                s.running = running;
                s.models = models;
                s.checked = true;
                s.error = error;
            }
            busy.store(false, Ordering::Relaxed);
            ctx.request_repaint();
        });
    }

    /// Stop the local Ollama server. Kills the process Octa started (if any)
    /// and, since this is an explicit user action, force-stops a local server
    /// Octa does not own a handle for; then re-probes.
    pub(crate) fn stop_ollama(&mut self, ctx: &egui::Context) {
        // Kill our own child first (fast, synchronous).
        self.chat.ollama.stop_server();
        self.chat.ollama.last_probe = Some(Instant::now());
        if self.chat.ollama.busy.swap(true, Ordering::Relaxed) {
            return;
        }
        let base = self.settings.chat_ollama_url.clone();
        let shared = self.chat.ollama.shared.clone();
        let busy = self.chat.ollama.busy.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(250));
            // Still up + local -> force-stop the local `ollama serve`.
            if ollama::is_running(&base) && ollama::is_local_url(&base) {
                ollama::stop_local_server();
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            let running = ollama::is_running(&base);
            let (models, error) = if running {
                match ollama::list_models(&base) {
                    Ok(m) => (m, None),
                    Err(e) => (Vec::new(), Some(e)),
                }
            } else {
                (Vec::new(), None)
            };
            {
                let mut s = shared.lock().unwrap();
                s.running = running;
                s.models = models;
                s.checked = true;
                s.error = error;
            }
            busy.store(false, Ordering::Relaxed);
            ctx.request_repaint();
        });
    }
}
