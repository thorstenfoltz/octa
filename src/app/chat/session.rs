//! Live state for one chat session, shared between the UI thread and the
//! agent worker via `Arc<Mutex<ChatSessionState>>`. The worker mutates
//! `streaming` / `messages` / `tool_log` as a turn progresses; the UI drains
//! it each frame.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::types::Message;

/// What the active turn is doing right now, for the UI spinner / live text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnPhase {
    /// Waiting on / receiving the model's streamed response.
    Streaming,
    /// Running the tools the model asked for.
    ExecutingTools,
}

/// The partial assistant turn currently being streamed.
#[derive(Clone, Debug)]
pub struct StreamingTurn {
    /// Assistant prose accumulated so far.
    pub text: String,
    pub phase: TurnPhase,
    /// Number of tools queued to run this iteration (for the spinner label).
    pub pending_tool_count: usize,
}

impl Default for StreamingTurn {
    fn default() -> Self {
        StreamingTurn {
            text: String::new(),
            phase: TurnPhase::Streaming,
            pending_tool_count: 0,
        }
    }
}

/// The full live state of a chat session.
pub struct ChatSessionState {
    pub id: String,
    pub title: String,
    pub provider_id: String,
    pub model: String,
    pub messages: Vec<Message>,
    /// `Some` while a turn is in flight.
    pub streaming: Option<StreamingTurn>,
    /// Set by the UI before spawning a turn, cleared by the worker on finish.
    pub running: Arc<AtomicBool>,
    /// Flipped by the UI's Cancel button; polled by the worker.
    pub cancel: Arc<AtomicBool>,
    /// Fatal error from the last turn, shown as a banner.
    pub error: Option<String>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Title was set explicitly by the user (don't auto-derive from message 1).
    pub title_pinned: bool,
}

impl ChatSessionState {
    /// A fresh, empty session for the given provider + model.
    pub fn new(provider_id: impl Into<String>, model: impl Into<String>) -> Self {
        ChatSessionState {
            id: new_session_id(),
            title: "New chat".to_string(),
            provider_id: provider_id.into(),
            model: model.into(),
            messages: Vec::new(),
            streaming: None,
            running: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(AtomicBool::new(false)),
            error: None,
            input_tokens: 0,
            output_tokens: 0,
            title_pinned: false,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Derive the title from the first user message, until the user renames it.
    pub fn refresh_auto_title(&mut self) {
        if self.title_pinned {
            return;
        }
        if let Some(first_user) = self.messages.iter().find(|m| {
            matches!(m.role, super::types::Role::User) && !m.text_preview().trim().is_empty()
        }) {
            let preview = first_user.text_preview();
            let trimmed: String = preview.chars().take(48).collect();
            self.title = if preview.chars().count() > 48 {
                format!("{}...", trimmed.trim_end())
            } else {
                trimmed
            };
        }
    }
}

/// A reasonably-unique session id from the wall clock plus a random suffix
/// (no `uuid` dependency - `rand` is already in the tree).
pub fn new_session_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let suffix: u32 = rand::random();
    format!("{now:x}-{suffix:08x}")
}
