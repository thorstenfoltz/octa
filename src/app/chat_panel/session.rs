//! Chat session lifecycle: load/new/persist/autosave/export and the agent-turn kickoff (send_chat_message). Split out of chat_panel/mod.rs.

use eframe::egui;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use octa::i18n::t;

use crate::app::chat::providers::{self, make_provider};
use crate::app::chat::session::ChatSessionState;
use crate::app::chat::{agent, build_system_prompt, persist, tools};
use crate::app::state::OctaApp;
use crate::ui::settings::{ChatProviderKind, chat_profiles};
use octa::ui::status_bar::format_number;

use super::profile_api_key;

impl OctaApp {
    /// Replace the live session with a saved one, persisting the current first.
    pub(crate) fn load_chat_session(&mut self, id: &str) {
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
        let profile = chat_profiles::active_profile(&self.settings);
        self.chat.session = Arc::new(Mutex::new(ChatSessionState::new(
            profile.kind.id(),
            profile.model,
        )));
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
    pub(crate) fn autosave_chat_session(&mut self) {
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
    pub(crate) fn conversation_text(&self) -> String {
        use crate::app::chat::types::{ContentBlock, Role};
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

    /// Save the active session as Markdown (.md) or JSON (.json). The picked
    /// file extension selects the format.
    pub(crate) fn export_chat_session(&mut self) {
        use crate::app::chat::{export, persist};
        let saved = {
            let guard = self.chat.session.lock().unwrap();
            if guard.messages.is_empty() {
                self.status_message = Some((t("chat.export_empty"), std::time::Instant::now()));
                return;
            }
            persist::snapshot(&guard)
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Markdown", &["md"])
            .add_filter("JSON", &["json"])
            .set_file_name("chat-export.md")
            .save_file()
        else {
            return;
        };
        let is_json = path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("json"))
            .unwrap_or(false);
        let content = if is_json {
            serde_json::to_string_pretty(&saved).unwrap_or_default()
        } else {
            export::to_markdown(&saved, export::EXPORT_RESULT_CAP_BYTES)
        };
        match std::fs::write(&path, content) {
            Ok(()) => {
                self.status_message = Some((
                    format!("{}: {}", t("chat.export_done"), path.display()),
                    std::time::Instant::now(),
                ));
            }
            Err(e) => {
                self.status_message =
                    Some((format!("Export failed: {e}"), std::time::Instant::now()));
            }
        }
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
    /// The header's usage meter: tokens this session, input and output.
    /// `None` before the first turn reports any usage, so an empty chat
    /// carries no chrome.
    pub(crate) fn usage_meter(&mut self) -> Option<String> {
        let (input_tokens, output_tokens) = {
            let s = self.chat.session.lock().unwrap();
            (s.input_tokens, s.output_tokens)
        };
        if input_tokens == 0 && output_tokens == 0 {
            return None;
        }
        Some(
            t("chat.usage")
                .replace("{in}", &format_number(input_tokens as usize))
                .replace("{out}", &format_number(output_tokens as usize)),
        )
    }

    /// Ask the assistant to explain the active tab, as an ordinary chat turn.
    ///
    /// A fixed prompt in front of the context the panel already assembles: the
    /// system prompt lists the open tabs and the tools read them, so this adds
    /// no new plumbing, and the answer is a normal message the user can export
    /// or follow up on. Deliberately NOT the one-shot, tool-less path that
    /// "Ask" and "Ask SQL" use - explaining a file means looking at it.
    ///
    /// The input box is restored afterwards, so a half-written question is not
    /// eaten by pressing the button beside it.
    pub(crate) fn explain_active_file(&mut self, ctx: &egui::Context) {
        let tab = &self.tabs[self.active_tab];
        if tab.table.col_count() == 0 {
            return;
        }
        let label = tab
            .table
            .source_path
            .as_ref()
            .and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| tab.title_display());
        let prompt = t("chat.explain_prompt").replace("{file}", &label);

        self.chat.visible = true;
        let draft = std::mem::replace(&mut self.chat.input, prompt);
        self.send_chat_message(ctx);
        // `send_chat_message` takes the input on success and leaves it alone
        // when it bails (no key), so either way the user's draft belongs back.
        self.chat.input = draft;
    }

    pub(crate) fn send_chat_message(&mut self, ctx: &egui::Context) {
        // Everything about the request comes from the active profile: provider,
        // model, temperature and thinking. Only the caps stay global.
        let profile = chat_profiles::active_profile(&self.settings);
        let provider_kind = profile.kind;
        // Cloud providers need a key; Ollama runs locally and does not.
        let api_key = if provider_kind.needs_api_key() {
            match profile_api_key(&profile, &self.settings) {
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

        // The profile's own write switch governs the whole chat surface: it
        // gates the tool list, the tool context, and the live-tab edit drain
        // (the global Write protection switch no longer applies here).
        let allow_writes = profile.allow_writes && !self.chat.plain_mode;
        // The context is still built in plain mode, because `TurnRequest` owns
        // one either way - but with no tools advertised nothing can reach it,
        // and `allow_writes` is forced off above so the edit drain stays shut.
        let tool_ctx = self.build_tool_context(allow_writes);
        // Only the core group plus the `enable_tools` menu go out now; the
        // model pulls in a group when it needs one (see `chat::tool_groups`).
        let disabled: std::collections::BTreeSet<String> =
            self.settings.chat_disabled_tools.iter().cloned().collect();
        // "Just answer": no tools at all, and a prompt that does not describe
        // capabilities this turn has not got. Also the cheapest request the
        // panel can make - the tool payload is most of a normal one.
        let tool_defs = if self.chat.plain_mode {
            Vec::new()
        } else {
            tools::initial_tool_defs(allow_writes, &disabled)
        };
        let has_tool_menu = tool_defs.iter().any(|d| d.name == tools::ENABLE_TOOLS);
        let system = if self.chat.plain_mode {
            crate::app::chat::build_plain_system_prompt()
        } else {
            build_system_prompt(&tool_ctx.open_tab_summaries(), allow_writes, has_tool_menu)
        };

        let fallback_base_url = match provider_kind {
            ChatProviderKind::Ollama => self.settings.chat_ollama_url.clone(),
            _ => self.settings.chat_base_url.clone(),
        };
        let cfg = providers::config_for_profile(
            &profile,
            &fallback_base_url,
            api_key,
            if self.settings.chat_max_tokens_unlimited {
                None
            } else {
                Some(self.settings.chat_max_tokens)
            },
        );
        let model = cfg.model.clone();
        let provider = make_provider(provider_kind);
        let max_iterations = self.settings.chat_max_tool_iterations;

        // Push the user message + install fresh per-turn flags under one lock.
        // New `Arc`s (not just `store(false)`) so a previous cancelled turn that
        // is still blocked on a network read can't resume into this session.
        let session = self.chat.session.clone();
        // Capture the session id for the (opt-in) tool-call audit log.
        let audit_session = if self.settings.chat_audit_log_enabled {
            Some(session.lock().unwrap().id.clone())
        } else {
            None
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let running = Arc::new(AtomicBool::new(true));
        {
            let mut guard = session.lock().unwrap();
            guard.error = None;
            guard.provider_id = provider_kind.id().to_string();
            guard.model = model;
            guard
                .messages
                .push(crate::app::chat::types::Message::user_text(prompt));
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
                allow_writes,
                disabled_tools: disabled,
                tool_ctx,
                max_iterations,
                cancel,
                running,
                audit_session,
            },
            ctx.clone(),
        );
        self.chat.focus_input = true;
    }
}
