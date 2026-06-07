//! The docked chat assistant panel and its self-contained settings dialog.
//! One panel is shared across tabs (like the SQL panel). The UI thread drains
//! the `Arc<Mutex<ChatSessionState>>` each frame; the agent worker fills it.

use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use serde_json::Value;

use octa::data::DataTable;

use crate::mcp::tools::{TableSnapshot, ToolContext};
use crate::ui::settings::{AppSettings, ChatPanelPosition, ChatProviderKind};
use octa::i18n::t;

use super::chat::providers::{ProviderConfig, make_provider};
use super::chat::session::{ChatSessionState, TurnPhase};
use super::chat::{agent, build_system_prompt, ollama, persist, secrets, tools};
use super::state::OctaApp;

/// Conservative response caps for tools driven by the chat agent - much
/// tighter than the MCP defaults so one `read_table` can't swamp the model.
const CHAT_ROW_CAP: usize = 200;
const CHAT_CELL_CAP: usize = 4096;

/// Shared (worker-updated) Ollama discovery state.
#[derive(Default)]
pub(crate) struct OllamaShared {
    /// Locally-installed models (from `/api/tags`).
    pub models: Vec<String>,
    /// Whether the server answered the last probe.
    pub running: bool,
    /// Whether a probe has completed at least once.
    pub checked: bool,
    /// Last probe / start error, if any.
    pub error: Option<String>,
}

/// How often the panel re-probes the Ollama server while it is the active
/// provider, so a server stopped outside Octa flips to "not running" on its own.
const OLLAMA_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// UI-side Ollama state: the shared probe result plus a busy flag.
pub(crate) struct OllamaUi {
    pub shared: Arc<Mutex<OllamaShared>>,
    pub busy: Arc<AtomicBool>,
    /// Set once we have auto-probed for the current provider selection.
    pub auto_probed: bool,
    /// The `ollama serve` process Octa itself started, if any. Held so Octa can
    /// stop the server it launched (on exit or via the Stop button) without
    /// touching a server the user started independently.
    pub server_child: Arc<Mutex<Option<Child>>>,
    /// When the panel last kicked off a liveness probe (UI thread only).
    pub last_probe: Option<Instant>,
}

impl Default for OllamaUi {
    fn default() -> Self {
        OllamaUi {
            shared: Arc::new(Mutex::new(OllamaShared::default())),
            busy: Arc::new(AtomicBool::new(false)),
            auto_probed: false,
            server_child: Arc::new(Mutex::new(None)),
            last_probe: None,
        }
    }
}

impl OllamaUi {
    /// Whether Octa owns a (still-live) `ollama serve` process it started.
    pub fn octa_owns_server(&self) -> bool {
        self.server_child.lock().unwrap().is_some()
    }

    /// Stop the server Octa started, if any. No-op when Octa did not start one.
    /// Kills the whole process group so the model runner dies too.
    pub fn stop_server(&self) {
        if let Some(mut child) = self.server_child.lock().unwrap().take() {
            ollama::stop_child_group(&mut child);
        }
    }
}

/// UI-side state for the chat panel. The live conversation lives behind the
/// `Arc<Mutex<..>>` so the worker thread can mutate it.
pub(crate) struct ChatPanelState {
    pub visible: bool,
    pub input: String,
    pub focus_input: bool,
    /// `@`-mention autocomplete popup is open (a partial `@token` is at the
    /// caret). Reset when the token clears or the user dismisses it.
    pub ac_visible: bool,
    /// Highlighted suggestion index in the `@`-mention popup.
    pub ac_selected: usize,
    pub session: Arc<Mutex<ChatSessionState>>,
    pub session_list_open: bool,
    /// Message count at the last autosave, to debounce per-turn saves.
    pub last_saved_len: usize,
    /// Screen rect of the docked panel last frame, so the table's clipboard
    /// shortcut can yield to in-chat text selection when the pointer is over
    /// the panel.
    pub panel_rect: Option<egui::Rect>,
    /// Local-Ollama discovery state (model list, running flag).
    pub ollama: OllamaUi,
}

impl ChatPanelState {
    pub fn new(settings: &AppSettings) -> Self {
        let provider = settings.chat_provider;
        let model = current_model(settings, provider);
        ChatPanelState {
            visible: false,
            input: String::new(),
            focus_input: false,
            ac_visible: false,
            ac_selected: 0,
            session: Arc::new(Mutex::new(ChatSessionState::new(provider.id(), model))),
            session_list_open: false,
            last_saved_len: 0,
            panel_rect: None,
            ollama: OllamaUi::default(),
        }
    }
}

/// The model configured for `provider`, falling back to its built-in default.
fn current_model(settings: &AppSettings, provider: ChatProviderKind) -> String {
    settings
        .chat_models
        .get(provider.id())
        .filter(|m| !m.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| octa::ui::settings::chat_models::default_model(provider))
}

impl OctaApp {
    /// Toggle the panel open/closed (toolbar button + shortcut).
    pub(crate) fn toggle_chat_panel(&mut self) {
        self.chat.visible = !self.chat.visible;
        if self.chat.visible {
            self.chat.focus_input = true;
        }
    }

    pub(crate) fn close_chat_panel(&mut self) {
        self.chat.visible = false;
    }

    /// Render the docked panel. Called from the frame loop before the central
    /// panel, mirroring the SQL panel.
    pub(crate) fn render_chat_panel(&mut self, parent_ui: &mut egui::Ui) {
        if !self.chat.visible {
            return;
        }
        self.autosave_chat_session();
        let position = self.settings.chat_panel_position;
        let rect = match position {
            ChatPanelPosition::Right => {
                egui::Panel::right("octa_chat_panel")
                    .resizable(true)
                    .default_size(420.0)
                    .min_size(300.0)
                    .show_inside(parent_ui, |ui| self.render_chat_body(ui))
                    .response
                    .rect
            }
            ChatPanelPosition::Left => {
                egui::Panel::left("octa_chat_panel")
                    .resizable(true)
                    .default_size(420.0)
                    .min_size(300.0)
                    .show_inside(parent_ui, |ui| self.render_chat_body(ui))
                    .response
                    .rect
            }
            ChatPanelPosition::Bottom => {
                egui::Panel::bottom("octa_chat_panel")
                    .resizable(true)
                    .default_size(320.0)
                    .min_size(160.0)
                    .show_inside(parent_ui, |ui| self.render_chat_body(ui))
                    .response
                    .rect
            }
            ChatPanelPosition::Top => {
                egui::Panel::top("octa_chat_panel")
                    .resizable(true)
                    .default_size(320.0)
                    .min_size(160.0)
                    .show_inside(parent_ui, |ui| self.render_chat_body(ui))
                    .response
                    .rect
            }
        };
        self.chat.panel_rect = Some(rect);
    }

    fn render_chat_body(&mut self, ui: &mut egui::Ui) {
        self.render_chat_header(ui);
        ui.separator();
        // Input docked at the bottom; messages fill the rest.
        egui::Panel::bottom("octa_chat_input")
            .resizable(false)
            .show_inside(ui, |ui| {
                self.render_chat_input(ui);
            });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.render_chat_messages(ui);
        });
    }

    fn render_chat_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(t("chat.title"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("x").on_hover_text(t("chat.close")).clicked() {
                    self.close_chat_panel();
                }
                if ui.button(t("chat.settings")).clicked() {
                    // Chat settings now live in the main Settings dialog; open
                    // it with the Chat section expanded.
                    self.settings_dialog.open(&self.settings);
                    self.settings_dialog.focus_chat_section = true;
                }
                if ui.button(t("chat.new_session")).clicked() {
                    self.new_chat_session();
                }
                if ui.button(t("chat.history")).clicked() {
                    self.chat.session_list_open = !self.chat.session_list_open;
                }
                if ui
                    .button(t("chat.copy_all"))
                    .on_hover_text(t("chat.copy_all_hint"))
                    .clicked()
                {
                    let text = self.conversation_text();
                    if !text.is_empty() {
                        ui.ctx().copy_text(text);
                    }
                }
            });
        });
        self.render_chat_history_window(ui.ctx());

        // Provider + model quick-switch row.
        let mut provider_changed = false;
        ui.horizontal_wrapped(|ui| {
            let mut provider = self.settings.chat_provider;
            egui::ComboBox::from_id_salt("octa_chat_provider")
                .selected_text(provider.label())
                .show_ui(ui, |ui| {
                    for p in ChatProviderKind::ALL {
                        ui.selectable_value(&mut provider, *p, p.label());
                    }
                });
            if provider != self.settings.chat_provider {
                self.settings.chat_provider = provider;
                self.settings.save();
                provider_changed = true;
            }

            let provider = self.settings.chat_provider;
            ui.label(t("chat.model"));
            if provider == ChatProviderKind::Ollama {
                self.render_ollama_model_row(ui);
            } else {
                self.render_model_picker(ui, provider);
            }
        });

        if provider_changed {
            self.chat.ollama.auto_probed = false;
            self.chat.ollama.last_probe = None;
        }

        // For Ollama, probe once per selection and then on a timer, so a server
        // stopped outside Octa (or one that just came up) is reflected without
        // the user clicking Refresh.
        if self.settings.chat_provider == ChatProviderKind::Ollama {
            let busy = self.chat.ollama.busy.load(Ordering::Relaxed);
            let first = !self.chat.ollama.auto_probed;
            let due = self
                .chat
                .ollama
                .last_probe
                .is_none_or(|t| t.elapsed() >= OLLAMA_POLL_INTERVAL);
            if !busy && (first || due) {
                self.chat.ollama.auto_probed = true;
                self.chat.ollama.last_probe = Some(Instant::now());
                self.refresh_ollama(ui.ctx());
            }
            ui.ctx().request_repaint_after(OLLAMA_POLL_INTERVAL);
        }

        // Provider readiness hint.
        if self.settings.chat_provider.needs_api_key()
            && secrets::get_api_key(self.settings.chat_provider, &self.settings).is_none()
        {
            ui.colored_label(
                egui::Color32::from_rgb(0xc0, 0x60, 0x10),
                t("chat.no_key_hint"),
            );
        }

        self.render_tab_context_chips(ui);
    }

    /// A chip row of the open tabs the assistant can see, active one
    /// highlighted, so the user knows what is in context and can insert an
    /// `@#N` reference to target a specific tab (handy when names repeat). The
    /// handles match `build_tool_context`'s numbering (non-chart tabs in order).
    fn render_tab_context_chips(&mut self, ui: &mut egui::Ui) {
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

    /// Model picker for cloud providers: a dropdown of common / recent models
    /// (quick picks) plus a free-text field so the newest or any custom model
    /// can always be typed. The text field is the source of truth, stored as
    /// the per-provider default in `chat_models`.
    fn render_model_picker(&mut self, ui: &mut egui::Ui, provider: ChatProviderKind) {
        let presets = octa::ui::settings::chat_models::preset_models(provider);
        let mut model = current_model(&self.settings, provider);
        let mut changed = false;

        if !presets.is_empty() {
            egui::ComboBox::from_id_salt(("octa_chat_model_preset", provider.id()))
                .selected_text(if model.is_empty() {
                    "(model)".to_string()
                } else {
                    model.clone()
                })
                .width(200.0)
                .show_ui(ui, |ui| {
                    for m in &presets {
                        if ui.selectable_label(&model == m, m.as_str()).clicked() {
                            model = m.clone();
                            changed = true;
                        }
                    }
                });
        }

        let resp = ui.add(
            egui::TextEdit::singleline(&mut model)
                .desired_width(200.0)
                .hint_text(octa::ui::settings::chat_models::default_model(provider)),
        );
        if resp.changed() {
            changed = true;
        }

        if changed {
            self.settings
                .chat_models
                .insert(provider.id().to_string(), model);
            self.settings.save();
        }
    }

    /// The Ollama model picker: a dropdown of locally-installed models plus
    /// Refresh / Start controls and a running-status chip.
    fn render_ollama_model_row(&mut self, ui: &mut egui::Ui) {
        let (models, running, checked, error) = {
            let s = self.chat.ollama.shared.lock().unwrap();
            (s.models.clone(), s.running, s.checked, s.error.clone())
        };
        let busy = self.chat.ollama.busy.load(Ordering::Relaxed);
        let mut model = current_model(&self.settings, ChatProviderKind::Ollama);

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
                self.settings
                    .chat_models
                    .insert("ollama".to_string(), model.clone());
                self.settings.save();
            }
        } else {
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
            if model != current_model(&self.settings, ChatProviderKind::Ollama) {
                self.settings
                    .chat_models
                    .insert("ollama".to_string(), model.clone());
                self.settings.save();
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
    fn refresh_ollama(&self, ctx: &egui::Context) {
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
    fn start_ollama(&self, ctx: &egui::Context) {
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
    fn stop_ollama(&mut self, ctx: &egui::Context) {
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

    fn render_chat_messages(&mut self, ui: &mut egui::Ui) {
        let session = self.chat.session.clone();
        let guard = session.lock().unwrap();

        if let Some(err) = &guard.error {
            ui.colored_label(
                egui::Color32::from_rgb(0xc0, 0x30, 0x30),
                format!("{} {err}", t("chat.error")),
            );
            ui.separator();
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for msg in &guard.messages {
                    render_message(ui, msg);
                }
                // Live partial assistant turn.
                if let Some(stream) = &guard.streaming {
                    if !stream.text.is_empty() {
                        bubble(ui, t("chat.assistant"), &stream.text, false);
                    }
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(14.0));
                        let label = match stream.phase {
                            TurnPhase::Streaming => t("chat.working"),
                            TurnPhase::ExecutingTools => {
                                format!(
                                    "{} ({})",
                                    t("chat.running_tools"),
                                    stream.pending_tool_count
                                )
                            }
                        };
                        ui.label(label);
                    });
                }
            });
    }

    fn render_chat_input(&mut self, ui: &mut egui::Ui) {
        let running = self.chat.session.lock().unwrap().is_running();
        let ready = self.provider_ready();

        let input_id = egui::Id::new("octa_chat_input");
        // `@`-mention suggestions: open-tab handles/names + column names.
        let mention_suggestions = self.build_mention_suggestions();

        ui.add_space(4.0);
        let mut send_now = false;
        let mut input_resp: Option<egui::Response> = None;
        ui.horizontal(|ui| {
            if running {
                if ui.button(t("chat.cancel")).clicked() {
                    self.cancel_chat();
                }
            } else if ui
                .add_enabled(ready, egui::Button::new(t("chat.send")))
                .clicked()
            {
                send_now = true;
            }

            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let resp = ui.add_enabled(
                    !running,
                    egui::TextEdit::multiline(&mut self.chat.input)
                        .id(input_id)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .hint_text(t("chat.input_hint")),
                );
                if self.chat.focus_input {
                    resp.request_focus();
                    self.chat.focus_input = false;
                }
                // Enter sends; Shift+Enter inserts a newline. (Like the SQL
                // editor, Enter is left to the editor even when the mention
                // popup is open - Tab / click accept a suggestion instead.)
                if resp.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)
                    && ready
                    && !running
                {
                    send_now = true;
                }
                input_resp = Some(resp);
            });
        });
        ui.add_space(4.0);

        // --- @-mention autocomplete -------------------------------------
        let resp = input_resp.expect("chat input always renders");
        let focused = ui.ctx().memory(|m| m.focused() == Some(input_id));
        let mut filtered: Vec<(String, String)> = Vec::new();
        let mut at_start = 0usize;
        let mut at_token_len = 0usize; // bytes of "@partial"
        if focused && !running {
            let cursor_byte = egui::TextEdit::load_state(ui.ctx(), input_id)
                .and_then(|s| s.cursor.char_range())
                .map(|r| {
                    self.chat
                        .input
                        .char_indices()
                        .nth(r.primary.index)
                        .map(|(i, _)| i)
                        .unwrap_or(self.chat.input.len())
                })
                .unwrap_or(self.chat.input.len());
            if let Some((start, partial)) = current_at_prefix(&self.chat.input, cursor_byte) {
                at_start = start;
                at_token_len = partial.len() + 1; // include the '@'
                let q = partial.to_lowercase();
                filtered = mention_suggestions
                    .into_iter()
                    .filter(|(disp, ins)| {
                        q.is_empty()
                            || disp.to_lowercase().contains(&q)
                            || ins.to_lowercase().contains(&q)
                    })
                    .take(8)
                    .collect();
                if !filtered.is_empty() {
                    self.chat.ac_visible = true;
                }
            } else {
                self.chat.ac_visible = false;
            }
        } else {
            self.chat.ac_visible = false;
        }

        let popup_active = self.chat.ac_visible && !filtered.is_empty();
        if self.chat.ac_selected >= filtered.len() {
            self.chat.ac_selected = 0;
        }

        let mut apply: Option<String> = None;
        if popup_active {
            let mut sel = self.chat.ac_selected;
            let len = filtered.len();
            let mut dismiss = false;
            ui.input_mut(|i| {
                if i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowDown) {
                    sel = (sel + 1) % len;
                }
                if i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowUp) {
                    sel = if sel == 0 { len - 1 } else { sel - 1 };
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                    apply = filtered.get(sel).map(|(_, ins)| ins.clone());
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                    dismiss = true;
                }
            });
            self.chat.ac_selected = sel;
            if dismiss {
                self.chat.ac_visible = false;
            }

            let popup_id = ui.make_persistent_id("octa_chat_mention_popup");
            egui::Popup::from_response(&resp)
                .id(popup_id)
                .open(self.chat.ac_visible)
                .align(egui::RectAlign::TOP_START)
                .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
                .show(|ui| {
                    ui.set_min_width(200.0);
                    let strong = if ui.visuals().dark_mode {
                        egui::Color32::WHITE
                    } else {
                        ui.visuals().strong_text_color()
                    };
                    for (idx, (disp, ins)) in filtered.iter().enumerate() {
                        let selected = idx == self.chat.ac_selected;
                        let label = if selected {
                            egui::RichText::new(disp).color(strong).strong()
                        } else {
                            egui::RichText::new(disp)
                        };
                        let r = ui.selectable_label(selected, label);
                        if r.clicked() {
                            apply = Some(ins.clone());
                        }
                        if r.hovered() {
                            self.chat.ac_selected = idx;
                        }
                    }
                });
        }

        // Apply the chosen mention: replace the "@partial" token with the
        // insert text (tab handle keeps its '@', a column name drops it) plus a
        // trailing space, then move the caret past it.
        if let Some(ins) = apply {
            let end = at_start + at_token_len;
            if end <= self.chat.input.len() {
                let mut piece = ins;
                piece.push(' ');
                self.chat.input.replace_range(at_start..end, &piece);
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), input_id) {
                    let new_char_idx = self.chat.input[..at_start + piece.len()].chars().count();
                    let cc = egui::text::CCursor::new(new_char_idx);
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(cc)));
                    state.store(ui.ctx(), input_id);
                }
                resp.request_focus();
            }
            self.chat.ac_visible = false;
        }

        if send_now && !self.chat.input.trim().is_empty() {
            let ctx = ui.ctx().clone();
            self.send_chat_message(&ctx);
        }
    }

    /// Build the `@`-mention autocomplete list from the open tabs: one entry per
    /// non-chart tab (`#N name` -> inserts `@#N`) followed by every distinct
    /// column name (inserts the bare name). Returned as `(display, insert)`.
    fn build_mention_suggestions(&self) -> Vec<(String, String)> {
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

    /// Whether the active provider can run: local providers (Ollama) are always
    /// ready; cloud providers need a resolvable API key.
    fn provider_ready(&self) -> bool {
        let provider = self.settings.chat_provider;
        !provider.needs_api_key() || secrets::get_api_key(provider, &self.settings).is_some()
    }

    /// The saved-session browser window (load / delete past chats).
    fn render_chat_history_window(&mut self, ctx: &egui::Context) {
        if !self.chat.session_list_open {
            return;
        }
        let mut open = true;
        let mut to_load: Option<String> = None;
        let mut to_delete: Option<String> = None;
        let mut clear_all = false;
        egui::Window::new(t("chat.history_title"))
            .id(egui::Id::new("octa_chat_history_v1"))
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
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
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for m in metas {
                            ui.horizontal(|ui| {
                                let label = if m.title.trim().is_empty() {
                                    t("chat.untitled")
                                } else {
                                    m.title.clone()
                                };
                                if ui.button(label).clicked() {
                                    to_load = Some(m.id.clone());
                                }
                                if ui.button("x").on_hover_text(t("chat.delete")).clicked() {
                                    to_delete = Some(m.id.clone());
                                }
                            });
                        }
                    });
            });
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
        if !open {
            self.chat.session_list_open = false;
        }
    }

    /// Replace the live session with a saved one, persisting the current first.
    fn load_chat_session(&mut self, id: &str) {
        self.persist_current_session();
        match persist::load(id) {
            Ok(saved) => {
                let mut s = ChatSessionState::new(saved.provider, saved.model);
                s.id = saved.id;
                s.title = saved.title;
                s.title_pinned = true;
                s.messages = saved.messages;
                self.chat.last_saved_len = s.messages.len();
                self.chat.session = Arc::new(Mutex::new(s));
            }
            Err(e) => eprintln!("octa: failed to load chat session: {e}"),
        }
    }

    /// Start a fresh session, persisting the previous one if it had content.
    pub(crate) fn new_chat_session(&mut self) {
        self.persist_current_session();
        let provider = self.settings.chat_provider;
        let model = current_model(&self.settings, provider);
        self.chat.session = Arc::new(Mutex::new(ChatSessionState::new(provider.id(), model)));
        self.chat.last_saved_len = 0;
        self.chat.focus_input = true;
    }

    /// Save the current session to disk if it has any messages.
    pub(crate) fn persist_current_session(&self) {
        let guard = self.chat.session.lock().unwrap();
        if guard.messages.is_empty() {
            return;
        }
        let snap = persist::snapshot(&guard);
        if let Err(e) = persist::save(&snap) {
            eprintln!("octa: failed to save chat session: {e}");
        }
    }

    /// Persist the session once per completed turn (debounced on message
    /// count, and only while no turn is in flight).
    fn autosave_chat_session(&mut self) {
        let (len, running, snap) = {
            let guard = self.chat.session.lock().unwrap();
            let running = guard.is_running();
            let len = guard.messages.len();
            let snap = if !running && len > 0 && len != self.chat.last_saved_len {
                Some(persist::snapshot(&guard))
            } else {
                None
            };
            (len, running, snap)
        };
        if let Some(snap) = snap {
            if let Err(e) = persist::save(&snap) {
                eprintln!("octa: failed to save chat session: {e}");
            }
            self.chat.last_saved_len = len;
        }
        let _ = running;
    }

    /// Plain-text transcript of the current session (speaker-labelled), for the
    /// header's Copy-conversation button.
    fn conversation_text(&self) -> String {
        use super::chat::types::{ContentBlock, Role};
        let guard = self.chat.session.lock().unwrap();
        let mut out = String::new();
        for msg in &guard.messages {
            for block in &msg.blocks {
                if let ContentBlock::Text { text } = block
                    && !text.trim().is_empty()
                {
                    let who = match msg.role {
                        Role::Assistant => t("chat.assistant"),
                        _ => t("chat.you"),
                    };
                    out.push_str(&who);
                    out.push_str(":\n");
                    out.push_str(text);
                    out.push_str("\n\n");
                }
            }
        }
        out
    }

    pub(crate) fn cancel_chat(&mut self) {
        let mut guard = self.chat.session.lock().unwrap();
        // Signal the worker to stop, and make the UI responsive immediately:
        // re-enable input and drop the live spinner. The worker exits at its
        // next cancel check (between SSE lines / tool calls) without committing
        // a partial turn - run_turn checks `cancel` before committing.
        guard
            .cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        guard
            .running
            .store(false, std::sync::atomic::Ordering::Relaxed);
        guard.streaming = None;
    }

    /// Build the tool context, system prompt, provider config, push the user
    /// message, and spawn the worker turn.
    fn send_chat_message(&mut self, ctx: &egui::Context) {
        let provider_kind = self.settings.chat_provider;
        // Cloud providers need a key; Ollama runs locally and does not.
        let api_key = if provider_kind.needs_api_key() {
            match secrets::get_api_key(provider_kind, &self.settings) {
                Some(k) => k,
                None => return,
            }
        } else {
            String::new()
        };
        let prompt = std::mem::take(&mut self.chat.input);
        let prompt = prompt.trim().to_string();
        if prompt.is_empty() {
            return;
        }

        let tool_ctx = self.build_tool_context();
        let system = build_system_prompt(&tool_ctx.open_tab_summaries());
        let tool_defs = tools::tool_defs();

        let model = current_model(&self.settings, provider_kind);
        let cfg = ProviderConfig {
            model: model.clone(),
            base_url: match provider_kind {
                ChatProviderKind::OpenAiCompatible => Some(self.settings.chat_base_url.clone()),
                ChatProviderKind::Ollama => Some(self.settings.chat_ollama_url.clone()),
                _ => None,
            },
            api_key,
            temperature: self.settings.chat_temperature,
            max_tokens: if self.settings.chat_max_tokens_unlimited {
                None
            } else {
                Some(self.settings.chat_max_tokens)
            },
        };
        let provider = make_provider(provider_kind);
        let max_iterations = self.settings.chat_max_tool_iterations;

        // Push the user message + install fresh per-turn flags under one lock.
        // New `Arc`s (not just `store(false)`) so a previous cancelled turn that
        // is still blocked on a network read can't resume into this session.
        let session = self.chat.session.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let running = Arc::new(AtomicBool::new(true));
        {
            let mut guard = session.lock().unwrap();
            guard.error = None;
            guard.provider_id = provider_kind.id().to_string();
            guard.model = model;
            guard
                .messages
                .push(super::chat::types::Message::user_text(prompt));
            guard.refresh_auto_title();
            guard.cancel = cancel.clone();
            guard.running = running.clone();
        }

        agent::spawn_turn(
            session,
            agent::TurnRequest {
                provider,
                cfg,
                system,
                tools: tool_defs,
                tool_ctx,
                max_iterations,
                cancel,
                running,
            },
            ctx.clone(),
        );
        self.chat.focus_input = true;
    }

    /// Snapshot every open (non-chart) tab into a sandboxed `ToolContext`:
    /// the agent may read only these files (and the other sheets/tables of an
    /// open workbook/database) and writes are confined to the export dir.
    fn build_tool_context(&self) -> ToolContext {
        let mut open_tabs: Vec<TableSnapshot> = Vec::new();
        let mut active_index: Option<usize> = None;
        let mut allowed_read_paths: Vec<std::path::PathBuf> = Vec::new();

        for (i, tab) in self.tabs.iter().enumerate() {
            if tab.is_chart_tab {
                continue;
            }
            let is_active = i == self.active_tab;
            let snapshot = snapshot_table(&tab.table);
            let display_name = tab_display_name(tab, i);
            let source_path = tab.table.source_path.clone();
            if let Some(sp) = &source_path {
                let p = std::path::Path::new(sp);
                let canon = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
                if !allowed_read_paths.contains(&canon) {
                    allowed_read_paths.push(canon);
                }
            }
            if is_active {
                active_index = Some(open_tabs.len());
            }
            open_tabs.push(TableSnapshot {
                handle: format!("#{}", open_tabs.len() + 1),
                display_name,
                source_path,
                table: snapshot,
            });
        }

        let export_dir = {
            let raw = self.settings.chat_export_dir.trim();
            if raw.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(raw))
            }
        };

        ToolContext {
            open_tabs,
            active_tab: active_index,
            default_row_limit: Some(CHAT_ROW_CAP),
            cell_byte_cap: CHAT_CELL_CAP,
            restrict_filesystem: true,
            allowed_read_paths,
            export_dir,
        }
    }
}

/// A clone of `table` with cell edits materialised, so tools see the user's
/// in-memory changes without the live table being mutated.
fn snapshot_table(table: &DataTable) -> DataTable {
    let mut clone = table.clone();
    clone.apply_edits();
    clone
}

/// A clean tab display name (no modified `*`), for addressing via `open_tab`.
fn tab_display_name(tab: &super::state::TabState, index: usize) -> String {
    tab.table
        .source_path
        .as_ref()
        .and_then(|p| {
            std::path::Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| format!("Untitled {}", index + 1))
}

/// If the caret sits inside an `@`-prefixed token (no whitespace since the
/// `@`), return the byte offset of the `@` and the partial text after it.
/// `cursor_byte` must be a char boundary. Returns `None` otherwise.
fn current_at_prefix(text: &str, cursor_byte: usize) -> Option<(usize, String)> {
    let upto = &text[..cursor_byte.min(text.len())];
    let token_start = upto.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
    let token = &upto[token_start..];
    let stripped = token.strip_prefix('@')?;
    // A second '@' means we are past a completed mention - don't re-trigger.
    if stripped.contains('@') {
        return None;
    }
    Some((token_start, stripped.to_string()))
}

/// Render one persisted message as bubbles + tool disclosure rows.
fn render_message(ui: &mut egui::Ui, msg: &super::chat::types::Message) {
    use super::chat::types::{ContentBlock, Role};
    for block in &msg.blocks {
        match block {
            ContentBlock::Text { text } => {
                if text.trim().is_empty() {
                    continue;
                }
                let (who, user) = match msg.role {
                    Role::Assistant => (t("chat.assistant"), false),
                    _ => (t("chat.you"), true),
                };
                bubble(ui, who, text, user);
            }
            ContentBlock::ToolUse { name, input, .. } => {
                egui::CollapsingHeader::new(format!("{} {name}", t("chat.tool_call")))
                    .id_salt(("octa_chat_tooluse", name.as_str(), input.to_string().len()))
                    .show(ui, |ui| {
                        copyable_text(ui, &pretty(input), true);
                    });
            }
            ContentBlock::ToolResult {
                content, is_error, ..
            } => {
                let header = if *is_error {
                    t("chat.tool_error")
                } else {
                    t("chat.tool_result")
                };
                egui::CollapsingHeader::new(header)
                    .id_salt(("octa_chat_toolresult", content.len()))
                    // Show errors expanded so the user sees what went wrong
                    // without having to open the disclosure.
                    .default_open(*is_error)
                    .show(ui, |ui| {
                        copyable_text(ui, &clip(content, 4000), true);
                    });
            }
        }
    }
}

/// A selectable text block with a right-click **Copy** menu (copies the whole
/// block). Used for chat bubbles + tool output so text is grabbable both by
/// selection+Ctrl+C and by right-click, regardless of the table view's own
/// clipboard shortcut.
fn copyable_text(ui: &mut egui::Ui, text: &str, monospace: bool) {
    let rich = if monospace {
        egui::RichText::new(text).monospace()
    } else {
        egui::RichText::new(text)
    };
    let resp = ui.add(egui::Label::new(rich).selectable(true).wrap());
    resp.context_menu(|ui| {
        if ui.button(t("chat.copy")).clicked() {
            ui.ctx().copy_text(text.to_string());
            ui.close();
        }
    });
}

/// A simple speaker-labelled text block.
fn bubble(ui: &mut egui::Ui, who: String, text: &str, user: bool) {
    ui.add_space(4.0);
    let color = if user {
        egui::Color32::from_rgb(0x30, 0x70, 0xc0)
    } else {
        egui::Color32::from_rgb(0x30, 0x90, 0x50)
    };
    ui.colored_label(color, who);
    copyable_text(ui, text, false);
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}\n...[clipped]")
}
