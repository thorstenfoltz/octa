//! The docked chat assistant panel and its self-contained settings dialog.
//! One panel is shared across tabs (like the SQL panel). The UI thread drains
//! the `Arc<Mutex<ChatSessionState>>` each frame; the agent worker fills it.

use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::ui::settings::{AppSettings, ChatPanelPosition, ChatProviderKind, chat_profiles};
use octa::i18n::t;

use super::chat::session::{ChatSessionState, TurnPhase};
use super::chat::{ollama, secrets};
use super::state::OctaApp;

mod context;
mod controls;
mod helpers;
mod session;
mod windows;

use helpers::{bubble, current_at_prefix, render_message};

/// Per-cell byte cap for chat tool results - tighter than the MCP default so
/// one wide cell can't swamp the model. The row cap is user-configurable
/// (`AppSettings.chat_result_row_limit`, default 200).
pub(crate) const CHAT_CELL_CAP: usize = 4096;

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
pub(crate) const OLLAMA_POLL_INTERVAL: Duration = Duration::from_secs(5);

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
    /// Window-size mode (Normal/Maximized/Minimized) for the History window.
    pub session_list_size: octa::ui::settings::DialogSize,
    /// Whether the saved-prompts manager window is open.
    pub prompts_window_open: bool,
    /// Window-size mode for the saved-prompts manager window.
    pub prompts_window_size: octa::ui::settings::DialogSize,
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
        let profile = chat_profiles::active_profile(settings);
        ChatPanelState {
            visible: false,
            input: String::new(),
            focus_input: false,
            ac_visible: false,
            ac_selected: 0,
            session: Arc::new(Mutex::new(ChatSessionState::new(
                profile.kind.id(),
                profile.model,
            ))),
            session_list_open: false,
            session_list_size: octa::ui::settings::DialogSize::default(),
            prompts_window_open: false,
            prompts_window_size: octa::ui::settings::DialogSize::default(),
            last_saved_len: 0,
            panel_rect: None,
            ollama: OllamaUi::default(),
        }
    }
}

/// The key a profile will actually use: its own when it opted into one, else
/// the key shared by every profile of that provider.
pub(crate) fn profile_api_key(
    profile: &octa::ui::settings::chat_profiles::ChatModelProfile,
    settings: &AppSettings,
) -> Option<String> {
    if profile.use_own_key {
        secrets::get_profile_key(&profile.id, settings)
    } else {
        secrets::get_api_key(profile.kind, settings)
    }
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
                    .default_size(580.0)
                    .min_size(380.0)
                    .show_inside(parent_ui, |ui| self.render_chat_body(ui))
                    .response
                    .rect
            }
            ChatPanelPosition::Left => {
                egui::Panel::left("octa_chat_panel")
                    .resizable(true)
                    .default_size(580.0)
                    .min_size(380.0)
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
                if ui
                    .button(t("chat.export"))
                    .on_hover_text(t("chat.export_hint"))
                    .clicked()
                {
                    self.export_chat_session();
                }
            });
        });
        self.render_chat_history_window(ui.ctx());
        self.render_prompts_window(ui.ctx());

        // Profile quick-switch row. One dropdown replaces the old provider +
        // model pair: everything about "who am I talking to" now lives in the
        // named profile, which is configured in Settings.
        let mut profile_changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(t("chat.profile"));
            let active_id = self.settings.chat_active_profile.clone();
            let active_name = self
                .settings
                .chat_profiles
                .iter()
                .find(|p| p.id == active_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| t("chat.no_profiles"));

            let mut chosen: Option<String> = None;
            egui::ComboBox::from_id_salt("octa_chat_profile")
                .selected_text(active_name)
                .show_ui(ui, |ui| {
                    for p in &self.settings.chat_profiles {
                        let label = if p.description.trim().is_empty() {
                            format!("{}  ({})", p.name, p.model)
                        } else {
                            format!("{}  ({}) - {}", p.name, p.model, p.description)
                        };
                        if ui.selectable_label(p.id == active_id, label).clicked() {
                            chosen = Some(p.id.clone());
                        }
                    }
                });
            if let Some(id) = chosen
                && id != self.settings.chat_active_profile
            {
                self.settings.chat_active_profile = id;
                self.settings.save();
                profile_changed = true;
            }

            if ui
                .small_button(t("chat.manage_profiles"))
                .on_hover_text(t("chat.manage_profiles_hint"))
                .clicked()
            {
                self.settings_dialog.open(&self.settings);
                self.settings_dialog.focus_chat_section = true;
            }
        });

        if profile_changed {
            // A different profile may point at a different Ollama server, so
            // the cached probe result no longer applies.
            self.chat.ollama.auto_probed = false;
            self.chat.ollama.last_probe = None;
        }

        let profile = chat_profiles::active_profile(&self.settings);

        // Ollama: keep the local-server controls (and the list of installed
        // models) in the panel, since they are about the running server rather
        // than the profile. Picking a model here writes it into the profile.
        if profile.kind == ChatProviderKind::Ollama {
            ui.horizontal_wrapped(|ui| {
                ui.label(t("chat.model"));
                self.render_ollama_model_row(ui, &profile.model);
            });

            // Probe once per selection and then on a timer, so a server stopped
            // outside Octa (or one that just came up) is reflected without the
            // user clicking Refresh.
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

        // Profile readiness hint.
        if profile.kind.needs_api_key() && profile_api_key(&profile, &self.settings).is_none() {
            ui.colored_label(
                egui::Color32::from_rgb(0xc0, 0x60, 0x10),
                t("chat.no_key_hint"),
            );
        }

        self.render_tab_context_chips(ui);
    }

    /// Write a model name back into the active profile (used by the Ollama
    /// model dropdown, which discovers models the profile form cannot).
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

        // --- Compute @-mention popup state BEFORE rendering the editor. ----
        // The multiline TextEdit consumes arrow / Enter / Tab keys for caret
        // movement and newlines during its own render, so the popup's key
        // handling must run first or it never sees them (mirrors the SQL editor
        // autocomplete in src/view_modes/sql.rs). Uses last frame's text +
        // cursor from memory, which is one frame behind - same as SQL.
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

        // Consume the popup keys while it is open, before the editor renders:
        // Up/Down move the selection, Enter or Tab accepts, Escape dismisses.
        // Gated on `popup_active`, so plain typing keeps arrows/Enter/Tab.
        let mut apply: Option<String> = None;
        if popup_active {
            let mut sel = self.chat.ac_selected;
            let len = filtered.len();
            let mut dismiss = false;
            ui.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    sel = (sel + 1) % len;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    sel = if sel == 0 { len - 1 } else { sel - 1 };
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
                {
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
        }

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

            // Prompts library: opens a manager window (save / insert / delete).
            if ui
                .button(t("chat.prompts"))
                .on_hover_text(t("chat.prompts_hint"))
                .clicked()
            {
                self.chat.prompts_window_open = !self.chat.prompts_window_open;
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
                // Enter sends; Shift+Enter inserts a newline. While the mention
                // popup is open Enter was consumed above to accept a suggestion,
                // so it never sends here.
                if !popup_active
                    && resp.has_focus()
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

        // --- @-mention autocomplete popup (visual + click) --------------
        let resp = input_resp.expect("chat input always renders");
        if popup_active {
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
}
