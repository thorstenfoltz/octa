//! ChatGPT through the user's own Codex CLI (`codex exec`).
//!
//! Runs on the ChatGPT plan the user signed Codex in with; Octa holds no
//! token. See `cli.rs` for the rules shared with the Copilot adapter.
//!
//! Checked against codex-cli 0.167.1 on 2026-11-11 with a free ChatGPT
//! account: the prompt is read from stdin when the prompt argument is `-`, the
//! answer alone is printed to stdout, and progress goes to stderr. The answer
//! arrives in one piece when the turn ends, not token by token.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::super::types::{ChatEvent, Message, ToolDef};
use super::{ChatProvider, ProviderConfig, cli};

pub struct CodexCli;

/// The arguments for one run in `dir`.
///
/// - `--sandbox read-only`: Codex may still run commands to look around, and
///   this keeps them from changing anything on the machine.
/// - `--ignore-user-config` / `--ignore-rules`: the user's own Codex setup
///   (extra MCP servers, plugins, looser rules) stays out of an Octa turn.
///   Sign-in is unaffected.
/// - `--ephemeral`: no session file is left behind for every question.
pub(crate) fn args(cfg: &ProviderConfig, dir: &Path) -> Vec<String> {
    let mut a: Vec<String> = [
        "exec",
        "--skip-git-repo-check",
        "--sandbox",
        "read-only",
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "--color",
        "never",
        "-C",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.push(dir.to_string_lossy().to_string());
    let model = cfg.model.trim();
    if !model.is_empty() {
        a.push("-m".into());
        a.push(model.to_string());
    }
    // The prompt comes on stdin.
    a.push("-".into());
    a
}

impl ChatProvider for CodexCli {
    fn name(&self) -> &'static str {
        "codex_cli"
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
        let program = cli::program(cfg, "codex");
        let result = cli::turn(
            &program,
            |dir| args(cfg, dir),
            &cli::flatten(system, messages, tools),
            !tools.is_empty(),
            cancel,
            sink,
            None,
        );
        // The answer moved the plan's limits; have the panel's line follow.
        crate::app::chat::plan_usage::ask_again(
            octa::ui::settings::ChatProviderKind::CodexCli,
            &program,
        );
        result
    }
}

#[cfg(test)]
#[path = "codex_cli_tests.rs"]
mod tests;
