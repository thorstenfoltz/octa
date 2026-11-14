//! GitHub Copilot through the user's own Copilot CLI (`copilot`).
//!
//! Runs on the Copilot plan the user signed that program in with; Octa holds
//! no token. See `cli.rs` for the rules shared with the Codex adapter.
//!
//! Options read from `copilot --help` on 2026-11-11: a prompt piped to stdin
//! runs non-interactively, `-s` prints the answer alone, and in that mode a
//! tool request no allow rule approves is denied, so Octa passes no allow
//! option at all. **The success path was never run against the service**: the
//! account it was tried on answered "Access denied by policy settings", which
//! is the error path the tests cover.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::super::types::{ChatEvent, Message, ToolDef};
use super::{ChatProvider, ProviderConfig, cli};

pub struct CopilotCli;

/// The arguments for one run in `dir`.
///
/// No `--allow-tool`, `--allow-all-*` or `--add-dir`: with none of them the
/// program can neither run commands nor touch files in non-interactive mode.
/// `--disable-builtin-mcps` keeps its GitHub servers out of the turn.
pub(crate) fn args(cfg: &ProviderConfig, dir: &Path) -> Vec<String> {
    let mut a: Vec<String> = ["-s", "--stream", "on", "--disable-builtin-mcps", "-C"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.push(dir.to_string_lossy().to_string());
    let model = cfg.model.trim();
    if !model.is_empty() {
        a.push("--model".into());
        a.push(model.to_string());
    }
    if let Some(effort) = cfg.reasoning.as_deref().map(str::trim)
        && !effort.is_empty()
    {
        a.push("--reasoning-effort".into());
        a.push(effort.to_string());
    }
    a
}

impl ChatProvider for CopilotCli {
    fn name(&self) -> &'static str {
        "copilot_cli"
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
            &cli::program(cfg, "copilot"),
            |dir| args(cfg, dir),
            &cli::flatten(system, messages, tools),
            !tools.is_empty(),
            cancel,
            sink,
            None,
        )
    }
}

#[cfg(test)]
#[path = "copilot_cli_tests.rs"]
mod tests;
