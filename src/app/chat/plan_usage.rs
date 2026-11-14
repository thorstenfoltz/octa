//! Which plan a subscription profile runs on and how much of its limits is
//! used, as the vendor's own program reports it.
//!
//! Octa measures nothing here and holds no token: every number is the
//! program's. Where each one comes from (all checked live on 2026-11-11):
//!
//! - **Claude**: the plan from `claude auth status` (local, no model call).
//!   The limits ride along with every answer as a `rate_limit_event` line
//!   and the program has no command that prints them. So the first time the
//!   panel shows such a profile, Octa sends ONE fixed four-word question to
//!   the smallest model ([`claude_first_limits`]) to have numbers to show;
//!   after that every real answer refreshes them. The user's report the day
//!   it was built: "it shows something for OpenAI, but not for Anthropic".
//! - **Codex**: plan and limits from `codex app-server`, asked
//!   `account/rateLimits/read` over stdio (no model call). Asked when the
//!   panel first shows such a profile and again after each answer.
//! - **Copilot**: nothing. Its usage file holds only what one run cost, and
//!   what its event stream carries on a working account could not be looked
//!   at (the account it was tried on is denied by policy). So the panel says
//!   that it does not know, instead of guessing.
//!
//! The numbers cover everything done on the plan, in Octa or elsewhere.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde_json::Value;

use octa::ui::settings::ChatProviderKind;

use super::providers::cli;

/// One limit: how much of a time window is used and when it starts afresh.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageWindow {
    /// How long the window is. 300 is the five-hour session, 10,080 a week.
    pub minutes: u64,
    pub used_percent: f32,
    /// Unix seconds, when the program says.
    pub resets_at: Option<i64>,
}

/// What is known about one kind of subscription right now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanUsage {
    pub plan: Option<String>,
    /// Shortest window first. Any number of them: a free ChatGPT plan has
    /// one of thirty days, a Claude plan a session and a week.
    pub windows: Vec<UsageWindow>,
    /// A question to the program is out.
    pub pending: bool,
    /// The program was asked at least once.
    asked: bool,
}

// ponytail: one entry per kind, so two profiles of one kind that name
// different programs (two accounts) share a line. Key by program path if
// anyone runs it that way.
static STORE: LazyLock<Mutex<HashMap<&'static str, PlanUsage>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn with_entry(kind: ChatProviderKind, change: impl FnOnce(&mut PlanUsage)) {
    change(STORE.lock().unwrap().entry(kind.id()).or_default());
}

/// What is known for `kind`.
pub fn get(kind: ChatProviderKind) -> PlanUsage {
    STORE
        .lock()
        .unwrap()
        .get(kind.id())
        .cloned()
        .unwrap_or_default()
}

/// Whether `kind`'s program reports a plan or limits at all.
pub fn reported_by(kind: ChatProviderKind) -> bool {
    matches!(
        kind,
        ChatProviderKind::ClaudeCli | ChatProviderKind::CodexCli
    )
}

/// Ask the program once, the first time a profile of `kind` is on screen.
pub fn ensure(kind: ChatProviderKind, program: &str) {
    if reported_by(kind) && !get(kind).asked {
        ask_again(kind, program);
    }
}

/// Ask the program on a thread of its own, so neither a frame nor the end of
/// a turn waits for it.
pub fn ask_again(kind: ChatProviderKind, program: &str) {
    if !reported_by(kind) {
        return;
    }
    with_entry(kind, |e| {
        e.asked = true;
        e.pending = true;
    });
    let program = program.to_string();
    std::thread::spawn(move || {
        match kind {
            ChatProviderKind::ClaudeCli => {
                let plan = claude_plan(&program);
                let signed_in = plan.is_some();
                with_entry(kind, |e| e.plan = plan.or(e.plan.take()));
                // Not signed in: a question would only fail.
                if signed_in && get(kind).windows.is_empty() {
                    claude_first_limits(&program);
                }
            }
            ChatProviderKind::CodexCli => {
                if let Some((plan, windows)) = codex_limits(&program) {
                    with_entry(kind, |e| {
                        e.plan = plan;
                        e.windows = windows;
                    });
                }
            }
            _ => {}
        }
        with_entry(kind, |e| e.pending = false);
    });
}

/// Take the limits out of one line of Claude's output, when it is the line
/// that carries them. Called for every line of every answer.
pub fn note_claude_line(line: &Value) {
    let windows = parse_claude_limits(line);
    if !windows.is_empty() {
        with_entry(ChatProviderKind::ClaudeCli, |e| e.windows = windows);
    }
}

/// A plan as the program spells it (`pro`, `self_serve_business`) made
/// readable: `Pro`, `Self serve business`.
fn plan_label(raw: &str) -> Option<String> {
    let words = raw.trim().replace('_', " ");
    let mut chars = words.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

/// `claude auth status`: `{"loggedIn": true, "subscriptionType": "pro", ..}`.
pub(crate) fn parse_claude_status(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    plan_label(v["subscriptionType"].as_str()?)
}

/// Claude's `rate_limit_event` line. `utilization` is a share of one.
pub(crate) fn parse_claude_limits(line: &Value) -> Vec<UsageWindow> {
    if line["type"] != "rate_limit_event" {
        return Vec::new();
    }
    // ponytail: the two windows Claude plans have today. A window under
    // another name is left out until its length is known.
    let known = [("five_hour", 300), ("seven_day", 10_080)];
    let windows = &line["rate_limit_info"]["unifiedWindows"];
    known
        .iter()
        .filter_map(|(key, minutes)| {
            let w = windows.get(key)?;
            Some(UsageWindow {
                minutes: *minutes,
                used_percent: (w["utilization"].as_f64()? * 100.0) as f32,
                resets_at: w["resetsAt"].as_i64(),
            })
        })
        .collect()
}

/// The `result` of Codex's `account/rateLimits/read`.
pub(crate) fn parse_codex(result: &Value) -> (Option<String>, Vec<UsageWindow>) {
    let limits = &result["rateLimits"];
    let mut windows: Vec<UsageWindow> = ["primary", "secondary"]
        .iter()
        .filter_map(|key| {
            let w = limits.get(key)?;
            Some(UsageWindow {
                minutes: w["windowDurationMins"].as_u64()?,
                used_percent: w["usedPercent"].as_f64()? as f32,
                resets_at: w["resetsAt"].as_i64(),
            })
        })
        .collect();
    windows.sort_by_key(|w| w.minutes);
    (limits["planType"].as_str().and_then(plan_label), windows)
}

fn claude_plan(program: &str) -> Option<String> {
    let args = ["auth".to_string(), "status".to_string()];
    let child = cli::spawn(program, &args, &std::env::temp_dir()).ok()?;
    // `wait_with_output` closes stdin first, so the program is not left
    // waiting for input.
    let out = child.wait_with_output().ok()?;
    parse_claude_status(&String::from_utf8_lossy(&out.stdout))
}

/// Claude prints its limits only beside an answer, so get one: a fixed
/// question with nothing of the user's in it, to the smallest model. Its
/// lines go through [`note_claude_line`] like any answer's. Costs a few
/// hundred tokens of the plan, once per run of Octa.
fn claude_first_limits(program: &str) {
    use super::providers::{ProviderConfig, claude_cli};
    let cfg = ProviderConfig {
        model: "haiku".into(),
        base_url: None,
        api_key: String::new(),
        temperature: None,
        max_tokens: None,
        reasoning: None,
        verbosity: None,
        pro_mode: false,
    };
    let _ = cli::run(
        program,
        |dir| claude_cli::args(&cfg, dir),
        "Reply with: ok",
        &std::sync::atomic::AtomicBool::new(false),
        &mut |line| {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                note_claude_line(&v);
            }
        },
    );
}

/// How long Codex's app-server gets to answer. It answered in under a second
/// when tried; a program that says nothing must not keep a thread forever.
const CODEX_ANSWER_LIMIT: Duration = Duration::from_secs(15);

fn codex_limits(program: &str) -> Option<(Option<String>, Vec<UsageWindow>)> {
    let args = ["app-server".to_string()];
    let mut child = cli::spawn(program, &args, &std::env::temp_dir()).ok()?;
    let mut stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let version = env!("CARGO_PKG_VERSION");
    let asked = writeln!(
        stdin,
        "{}\n{}\n{}",
        serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"clientInfo": {"name": "octa", "title": "Octa", "version": version}}}),
        serde_json::json!({"jsonrpc": "2.0", "method": "initialized"}),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read"}),
    )
    .and_then(|()| stdin.flush());

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if v["id"] == 2 {
                let _ = tx.send(v);
                break;
            }
        }
    });
    let answer = asked
        .ok()
        .and_then(|()| rx.recv_timeout(CODEX_ANSWER_LIMIT).ok());
    // The server runs until it is stopped; stdin stays open until here so
    // it does not leave before answering.
    crate::app::chat::ollama::stop_child_group(&mut child);
    drop(stdin);
    let answer = answer?;
    answer.get("result").map(parse_codex)
}

/// How a window is named: the session, the week, or its length in days.
fn window_name(minutes: u64) -> String {
    use octa::i18n::t;
    const DAY: u64 = 24 * 60;
    match minutes {
        m if m < DAY => t("chat.plan_session"),
        m if m == 7 * DAY => t("chat.plan_week"),
        m => t("chat.plan_days").replace("{n}", &(m.div_ceil(DAY)).to_string()),
    }
}

/// When a window resets, in local time: the clock time when that is within a
/// day, the date with it otherwise. Digits only, so it reads the same in
/// every language.
fn reset_text(resets_at: i64, now: chrono::DateTime<chrono::Local>) -> Option<String> {
    let at = chrono::DateTime::from_timestamp(resets_at, 0)?.with_timezone(&chrono::Local);
    let soon = at.signed_duration_since(now) < chrono::Duration::hours(24);
    let format = if soon { "%H:%M" } else { "%Y-%m-%d %H:%M" };
    Some(at.format(format).to_string())
}

/// The line the panel shows: `Pro   Session 47% (resets 14:00)   Week 59%
/// (resets 2026-11-15 16:00)`. `None` when there is nothing to say yet.
pub fn line(usage: &PlanUsage, now: chrono::DateTime<chrono::Local>) -> Option<String> {
    use octa::i18n::t;
    let mut parts: Vec<String> = usage.plan.iter().cloned().collect();
    for w in &usage.windows {
        let mut part = format!("{} {:.0}%", window_name(w.minutes), w.used_percent);
        if let Some(when) = w.resets_at.and_then(|at| reset_text(at, now)) {
            part.push_str(&format!(
                " ({})",
                t("chat.plan_resets").replace("{when}", &when)
            ));
        }
        parts.push(part);
    }
    (!parts.is_empty()).then(|| parts.join("   "))
}

#[cfg(test)]
#[path = "plan_usage_tests.rs"]
mod tests;
