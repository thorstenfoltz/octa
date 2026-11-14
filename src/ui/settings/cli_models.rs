//! The model lists of the subscription providers, asked from the programs
//! themselves: `codex debug models` and `copilot help config`. Claude Code has
//! no such command; it takes the family names in [`CLAUDE_ALIASES`] and
//! resolves each to its newest model itself.
//!
//! A table of names here would be wrong the day a plan changes what it
//! offers, and the programs already know. Both commands are local and need no
//! sign-in. They run once per program on a worker thread, so the Settings form
//! never waits for one; until the answer is in, the dropdown is simply absent
//! and the free-text field beside it works as always.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};

use super::ChatProviderKind;

/// The names `claude --model` resolves to the newest model of each family,
/// largest first. Aliases on purpose: a version typed here would age.
pub(crate) const CLAUDE_ALIASES: &[&str] = &["fable", "opus", "sonnet", "haiku"];

/// Program -> its models. `None` while the question is still out.
static CACHE: LazyLock<Mutex<HashMap<String, Option<Vec<String>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The models `kind`'s program offers, or an empty list while it has not
/// answered, is not installed, or `kind` is not a program at all.
///
/// `path` is the profile's own program path; empty means the one on `PATH`.
pub fn models(ctx: &egui::Context, kind: ChatProviderKind, path: &str) -> Vec<String> {
    let Some(default) = kind.cli_program() else {
        return Vec::new();
    };
    if kind == ChatProviderKind::ClaudeCli {
        return CLAUDE_ALIASES.iter().map(|s| s.to_string()).collect();
    }
    let path = path.trim();
    let program = if path.is_empty() { default } else { path }.to_string();

    let mut cache = CACHE.lock().unwrap();
    if let Some(known) = cache.get(&program) {
        return known.clone().unwrap_or_default();
    }
    cache.insert(program.clone(), None);
    drop(cache);

    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let list = ask(kind, &program);
        CACHE.lock().unwrap().insert(program, Some(list));
        ctx.request_repaint();
    });
    Vec::new()
}

/// Run the program's listing command and read the names out of it. Any
/// failure is an empty list: the form then offers the text field alone.
fn ask(kind: ChatProviderKind, program: &str) -> Vec<String> {
    let args: &[&str] = match kind {
        ChatProviderKind::CodexCli => &["debug", "models"],
        ChatProviderKind::CopilotCli => &["help", "config"],
        _ => return Vec::new(),
    };
    let run = |name: &str| {
        let mut cmd = Command::new(name);
        cmd.args(args).stdin(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            // No console window flashing up behind Octa.
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        cmd.output()
    };
    let out = match run(program) {
        // npm installs these programs on Windows as `<name>.cmd`.
        Err(e) if cfg!(windows) && e.kind() == std::io::ErrorKind::NotFound => {
            run(&format!("{program}.cmd"))
        }
        other => other,
    };
    let Ok(out) = out else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    match kind {
        ChatProviderKind::CodexCli => parse_codex(&text),
        _ => parse_copilot(&text),
    }
}

/// `codex debug models`: `{"models": [{"slug": .., "visibility": "list"}]}`.
/// Only the ones Codex itself lists in its picker.
pub(crate) fn parse_codex(json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    value["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["visibility"].as_str() == Some("list"))
        .filter_map(|m| m["slug"].as_str())
        .map(str::to_string)
        .collect()
}

/// `copilot help config`: the quoted names listed under the `model` setting,
/// with `auto` (Copilot picks) in front.
pub(crate) fn parse_copilot(help: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut inside = false;
    for line in help.lines() {
        let line = line.trim();
        if line.starts_with("`model`:") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        // The list ends at the first line that is not a quoted bullet.
        let Some(name) = line
            .strip_prefix("- \"")
            .and_then(|rest| rest.strip_suffix('"'))
        else {
            break;
        };
        names.push(name.to_string());
    }
    if !names.is_empty() {
        names.insert(0, "auto".to_string());
    }
    names
}

#[cfg(test)]
#[path = "cli_models_tests.rs"]
mod tests;
