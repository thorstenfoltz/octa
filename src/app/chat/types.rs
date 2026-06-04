//! Provider-neutral message + event model shared by the agent loop, the
//! provider adapters, and session persistence. Keeping one neutral shape here
//! means the four providers only translate at their wire boundary and the UI /
//! persistence never sees a provider-specific type.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Who authored a message in the transcript.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System prompt (sent once, not stored as a transcript turn).
    System,
    /// The human user.
    User,
    /// The model.
    Assistant,
    /// A tool result fed back to the model. Carried inside a `User` message's
    /// blocks on the wire for most providers, but kept distinct here for
    /// clarity in the transcript.
    Tool,
}

/// One piece of a message. A single assistant turn can mix prose
/// (`Text`) with one or more `ToolUse` requests; the following user turn
/// carries the matching `ToolResult` blocks.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Plain text (assistant prose or a user prompt).
    Text { text: String },
    /// The model asked to call a tool with these arguments.
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// The result of running a tool, fed back to the model.
    ToolResult {
        id: String,
        content: String,
        is_error: bool,
    },
}

impl ContentBlock {
    pub fn text(s: impl Into<String>) -> Self {
        ContentBlock::Text { text: s.into() }
    }
}

/// A single transcript turn.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub blocks: Vec<ContentBlock>,
}

impl Message {
    pub fn user_text(s: impl Into<String>) -> Self {
        Message {
            role: Role::User,
            blocks: vec![ContentBlock::text(s)],
        }
    }

    pub fn assistant(blocks: Vec<ContentBlock>) -> Self {
        Message {
            role: Role::Assistant,
            blocks,
        }
    }

    /// A user turn carrying tool results back to the model.
    pub fn tool_results(blocks: Vec<ContentBlock>) -> Self {
        Message {
            role: Role::User,
            blocks,
        }
    }

    /// Concatenated text of all `Text` blocks (for previews / titles).
    pub fn text_preview(&self) -> String {
        let mut out = String::new();
        for b in &self.blocks {
            if let ContentBlock::Text { text } = b {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(text);
            }
        }
        out
    }
}

/// Why the model stopped its turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// Natural end of the assistant turn.
    EndTurn,
    /// The model wants one or more tools run before continuing.
    ToolUse,
    /// Hit the response token cap.
    MaxTokens,
    /// Anything else the provider reported.
    Other(String),
}

/// A streaming event emitted by a provider during one turn. Tool calls are
/// assembled in full by the provider before being emitted (the agent does not
/// see partial argument fragments), which keeps the four adapters simple.
#[derive(Clone, Debug)]
pub enum ChatEvent {
    /// A chunk of assistant prose to append live.
    TextDelta(String),
    /// A fully-assembled tool call the model wants run.
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    /// Token usage, if the provider reports it.
    Usage {
        input_tokens: u32,
        output_tokens: u32,
    },
    /// The turn finished.
    Done { stop_reason: StopReason },
    /// A fatal error mid-stream.
    Error(String),
}

/// One tool advertised to the model: its name, description, and JSON Schema
/// for the arguments. Built from the MCP tools' `schemars`-derived `Params`.
#[derive(Clone, Debug)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}
