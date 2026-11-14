//! Claude through the user's own Claude Code CLI (`claude -p`).
//!
//! Runs on the Claude plan the user signed that program in with; Octa holds
//! no token. See `cli.rs` for the rules shared with the Codex and Copilot
//! adapters. Anthropic confirmed to the user on 2026-11-11 that Octa may do
//! this.
//!
//! Checked against Claude Code 2.1.322 on 2026-11-11 with a signed-in
//! subscription: with the options below the program reports `"tools":[]` and
//! `"mcp_servers":[]`, keeps its sign-in, and streams the answer as one JSON
//! object per line.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde_json::Value;

use super::super::types::{ChatEvent, Message, ToolDef};
use super::{ChatProvider, ProviderConfig, cli};

pub struct ClaudeCli;

/// Replaces Claude Code's own prompt, which is about editing a code project.
/// What Octa wants said is in the text on stdin: a whole file does not fit on
/// a Windows command line.
const SYSTEM_LINE: &str = "You are the assistant inside the Octa data viewer. \
Follow the instructions in the message.";

/// The arguments for one run.
///
/// - `--tools ""`: none of the program's own tools, so it can neither run a
///   command nor read a file. Octa's tools go through `cli.rs`.
/// - `--setting-sources ""`, `--strict-mcp-config`,
///   `--disable-slash-commands`: the user's own Claude Code setup (hooks,
///   plugins, MCP servers, skills) stays out of an Octa turn. Sign-in is
///   unaffected. Not `--bare`: that one ignores the subscription sign-in.
/// - `--no-session-persistence`: no session file is left for every question.
pub(crate) fn args(cfg: &ProviderConfig, _dir: &Path) -> Vec<String> {
    let mut a: Vec<String> = [
        "-p",
        "--tools",
        "",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--disable-slash-commands",
        "--no-session-persistence",
        "--system-prompt",
        SYSTEM_LINE,
        "--output-format",
        "stream-json",
        "--include-partial-messages",
        "--verbose",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let model = cfg.model.trim();
    if !model.is_empty() {
        a.push("--model".into());
        a.push(model.to_string());
    }
    if let Some(effort) = cfg.reasoning.as_deref().map(str::trim)
        && !effort.is_empty()
    {
        a.push("--effort".into());
        a.push(effort.to_string());
    }
    a
}

/// One line of the program's output as an event, or `None` for the many
/// lines that carry nothing the transcript needs.
pub(crate) fn handle_line(line: &str) -> Option<ChatEvent> {
    let v: Value = serde_json::from_str(line).ok()?;
    // The plan's limits come with every answer; the panel shows them.
    crate::app::chat::plan_usage::note_claude_line(&v);
    match v["type"].as_str()? {
        "stream_event" => {
            let delta = &v["event"]["delta"];
            (v["event"]["type"] == "content_block_delta" && delta["type"] == "text_delta")
                .then(|| ChatEvent::TextDelta(delta["text"].as_str().unwrap_or("").to_string()))
        }
        // The last line: the whole run's token counts, or why it failed.
        "result" => {
            if v["is_error"].as_bool() == Some(true) {
                let why = v["result"].as_str().unwrap_or("the run failed");
                return Some(ChatEvent::Error(format!("'claude' reported: {why}")));
            }
            let n = |key: &str| v["usage"][key].as_u64().unwrap_or(0) as u32;
            Some(ChatEvent::Usage {
                // Cached parts count: the meter means "tokens sent".
                input_tokens: n("input_tokens")
                    + n("cache_creation_input_tokens")
                    + n("cache_read_input_tokens"),
                output_tokens: n("output_tokens"),
            })
        }
        _ => None,
    }
}

impl ChatProvider for ClaudeCli {
    fn name(&self) -> &'static str {
        "claude_cli"
    }

    fn stream_turn(
        &self,
        cfg: &ProviderConfig,
        system: &str,
        messages: &[Message],
        tools: &[ToolDef],
        cancel: &AtomicBool,
        sink: &mut dyn FnMut(ChatEvent),
    ) -> Result<(), String> {
        cli::turn(
            &cli::program(cfg, "claude"),
            |dir| args(cfg, dir),
            &cli::flatten(system, messages, tools),
            !tools.is_empty(),
            cancel,
            sink,
            Some(handle_line),
        )
    }
}

#[cfg(test)]
#[path = "claude_cli_tests.rs"]
mod tests;
