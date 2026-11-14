//! Shared plumbing for the providers that start a vendor's own command-line
//! program instead of calling an API (`claude_cli`, `copilot_cli`,
//! `codex_cli`).
//!
//! The program runs on whatever plan the user signed it in with. Octa never
//! sees a token: it starts the user's installed program, writes one prompt to
//! its standard input and reads the answer from its standard output.
//!
//! Three rules every such provider follows, all enforced here:
//! - **Octa runs the tools, the program only writes text.** A CLI cannot call
//!   Octa's tools itself, so the prompt lists them and the model asks for one
//!   in a fenced block ([`TOOL_OPEN`]). [`Reply`] turns that block into the
//!   same `ChatEvent::ToolCall` an API provider emits, and the agent loop
//!   runs it under the same gates. Each round starts the program again.
//! - **The prompt goes in on stdin, never as an argument.** A whole file does
//!   not fit on a Windows command line (about 32,000 characters).
//! - **The working directory is a fresh, empty temporary folder**, removed
//!   afterwards, so the program finds no project of the user's to act on.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;

use super::super::types::{ChatEvent, ContentBlock, Message, Role, StopReason, ToolDef};
use super::ProviderConfig;

/// How long one run may take. The Ask boxes have no cancel button, so a
/// program that hangs must end by itself. As generous as Ollama's limit,
/// since a program may print nothing until its whole answer is ready.
const RUN_LIMIT: Duration = Duration::from_secs(30 * 60);

/// How much of the program's error output is kept for the message.
const STDERR_KEEP: usize = 2000;

/// The program to start: the path the profile names, else `default` looked up
/// on `PATH`.
pub fn program(cfg: &ProviderConfig, default: &str) -> String {
    cfg.base_url
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or(default)
        .to_string()
}

/// The line that opens a tool call in a reply. Six tildes, like Writing's
/// `octa-file` block: tool arguments often hold SQL or Markdown with backtick
/// fences of their own.
pub const TOOL_OPEN: &str = "~~~~~~octa-tool";
/// The line that closes it.
pub const TOOL_CLOSE: &str = "~~~~~~";

/// One tool call written the way the model is asked to write it.
fn tool_block(name: &str, arguments: &Value) -> String {
    let call = serde_json::json!({ "name": name, "arguments": arguments });
    format!("{TOOL_OPEN}\n{call}\n{TOOL_CLOSE}")
}

/// What the model is told about the tools: how to ask for one, and each
/// tool's name, purpose and argument schema.
pub fn tools_prompt(tools: &[ToolDef]) -> String {
    // Worded after live runs on 2026-11-11. A softer text got Claude Haiku to
    // write the call without its first and last line, followed by a row count
    // it made up. Codex, which has tools of its own, looked for Octa's among
    // them and answered that the tool "isn't available in this session",
    // hence the sentence saying where they are not.
    let mut out = format!(
        "\n\n=== Tools ===\n\
You are connected to Octa, a desktop data viewer. The user's open tabs and files are \
reachable ONLY through the Octa tools listed below. They are NOT in your own list of tools \
or functions, so do not look for them there: the only way to use one is to write the call \
as plain text in your answer, in the block shown below. Octa reads your answer, runs the \
call and sends you the result. Writing that block always works. Do not use a shell or any \
other tool of your own, and never say that a tool or the data is not available.\n\n\
To call a tool, write a block of exactly this shape, with the arguments as one JSON \
object:\n\n{}\n\n\
Rules:\n\
- The first and the last line of the block are required, exactly as shown.\n\
- After the block, STOP. Write nothing more. You do not know the result yet, so never \
write one yourself. Octa runs the call and sends you the real result in the next message.\n\
- Several calls: several blocks, one after another, then stop.\n\
- When you need no tool, answer normally and write no block.\n",
        tool_block("tool_name", &serde_json::json!({}))
    );
    for tool in tools {
        out.push_str(&format!(
            "\n--- {} ---\n{}\nArguments (JSON Schema): {}\n",
            tool.name, tool.description, tool.input_schema
        ));
    }
    out
}

/// One prompt out of a system prompt, the tools and a conversation. A CLI
/// takes a single text, so the earlier turns, their tool calls and the
/// results are written out above the newest message.
pub fn flatten(system: &str, messages: &[Message], tools: &[ToolDef]) -> String {
    // A result names its call only by id; the reader needs the tool's name.
    let name_of = |id: &str| {
        messages
            .iter()
            .flat_map(|m| &m.blocks)
            .find_map(|b| match b {
                ContentBlock::ToolUse { id: call, name, .. } if call == id => Some(name.as_str()),
                _ => None,
            })
            .unwrap_or("tool")
    };
    let text_of = |m: &Message| {
        m.blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.clone()),
                ContentBlock::ToolUse { name, input, .. } => Some(tool_block(name, input)),
                ContentBlock::ToolResult {
                    id,
                    content,
                    is_error,
                } => Some(format!(
                    "[Result of {}{}]\n{content}",
                    name_of(id),
                    if *is_error { ", it FAILED" } else { "" }
                )),
                ContentBlock::ProviderData { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut out = String::from(system);
    if !tools.is_empty() {
        out.push_str(&tools_prompt(tools));
    }
    let (last, earlier) = match messages.split_last() {
        Some(parts) => parts,
        None => return out,
    };
    if !earlier.is_empty() {
        out.push_str("\n\n=== The conversation so far ===\n");
        for m in earlier {
            let who = match m.role {
                Role::Assistant => "Assistant",
                _ => "User",
            };
            out.push_str(&format!("\n[{who}]\n{}\n", text_of(m)));
        }
    }
    let results = last
        .blocks
        .iter()
        .any(|b| matches!(b, ContentBlock::ToolResult { .. }));
    out.push_str(if results {
        "\n\n=== The results of your tool calls, continue from here ===\n\n"
    } else {
        "\n\n=== The user's message now ===\n\n"
    });
    out.push_str(&text_of(last));
    out
}

/// How a call starts when the model left the block's first line out.
const BARE_CALL: &str = "{\"name\"";

/// Whether `line` starts a tool call: the block's opening line, or the call
/// itself written bare.
fn opens_call(line: &str) -> bool {
    line.trim_end() == TOOL_OPEN || line.starts_with(BARE_CALL)
}

/// Read one call: a JSON object with a `name` and, when present, an object of
/// `arguments`.
fn call_in(json: &str) -> Option<(String, Value)> {
    let call: Value = serde_json::from_str(json).ok()?;
    let name = call.get("name")?.as_str()?.to_string();
    let arguments = match call.get("arguments") {
        None | Some(Value::Null) => serde_json::json!({}),
        Some(args @ Value::Object(_)) => args.clone(),
        Some(_) => return None,
    };
    Some((name, arguments))
}

/// The tool calls in the tail of a reply, from its first call on. `None` when
/// any block is not closed or does not hold a JSON object with a `name`: a
/// call that cannot be read is never run.
///
/// A call written without its block (one line, the JSON object alone) counts
/// too, and what follows it is dropped with the rest of the tail. That is the
/// one way a model was seen to get the format wrong, and what followed was a
/// result it had made up.
pub fn tool_calls_in(tail: &str) -> Option<Vec<(String, Value)>> {
    let mut calls = Vec::new();
    let mut lines = tail.lines();
    while let Some(line) = lines.next() {
        if line.starts_with(BARE_CALL) {
            match call_in(line) {
                Some(call) => calls.push(call),
                // The first line of the tail must be a call; a later line
                // that only looks like one is the model's prose.
                None if calls.is_empty() => return None,
                None => {}
            }
            continue;
        }
        if line.trim_end() != TOOL_OPEN {
            continue;
        }
        let mut json = String::new();
        loop {
            let inner = lines.next()?;
            if inner.trim_end() == TOOL_CLOSE {
                break;
            }
            json.push_str(inner);
            json.push('\n');
        }
        calls.push(call_in(&json)?);
    }
    Some(calls)
}

/// The reply as it arrives. Prose is handed on for the transcript at once;
/// from the first tool block on it is held back, so the user sees the tool
/// run and not its raw request.
#[derive(Default)]
pub struct Reply {
    text: String,
    /// How much of `text` was handed on.
    shown: usize,
    /// Where the first tool block starts, once one did.
    tools_at: Option<usize>,
    /// Whether this turn may call tools at all.
    with_tools: bool,
}

impl Reply {
    pub fn new(with_tools: bool) -> Self {
        Reply {
            with_tools,
            ..Default::default()
        }
    }

    /// Take the next piece of the answer and return what may be shown now.
    pub fn push(&mut self, chunk: &str) -> Option<String> {
        self.text.push_str(chunk);
        if self.tools_at.is_some() {
            return None;
        }
        let mut upto = self.text.len();
        if self.with_tools {
            // Only a whole line can open a block, so look at line starts from
            // the last shown line on.
            let from = self.text[..self.shown].rfind('\n').map_or(0, |i| i + 1);
            let mut start = from;
            loop {
                let rest = &self.text[start..];
                let (line, ended) = match rest.find('\n') {
                    Some(i) => (&rest[..i], true),
                    None => (rest, false),
                };
                if opens_call(line) && (ended || line == TOOL_OPEN) {
                    self.tools_at = Some(start);
                    upto = start;
                    break;
                }
                if !ended {
                    // A last line that may still become the start of a call
                    // waits for its rest.
                    let may_open = TOOL_OPEN.starts_with(line)
                        || BARE_CALL.starts_with(line)
                        || line.starts_with(BARE_CALL);
                    if !line.is_empty() && may_open {
                        upto = start;
                    }
                    break;
                }
                start += line.len() + 1;
            }
        }
        let upto = upto.max(self.shown);
        let out = self.text[self.shown..upto].to_string();
        self.shown = upto;
        (!out.is_empty()).then_some(out)
    }

    /// The end of the answer: what was still held back, the tool calls, and
    /// how the turn stopped.
    pub fn finish(self) -> Vec<ChatEvent> {
        let rest = &self.text[self.shown..];
        let calls = self
            .tools_at
            .and_then(|at| tool_calls_in(&self.text[at..]))
            .unwrap_or_default();
        if calls.is_empty() {
            // ponytail: a block that cannot be read is shown as the text it
            // is and ends the turn. Sending the model a "write it again"
            // round is the upgrade if models turn out to fumble the format.
            let mut events = Vec::new();
            if !rest.is_empty() {
                events.push(ChatEvent::TextDelta(rest.to_string()));
            }
            events.push(ChatEvent::Done {
                stop_reason: StopReason::EndTurn,
            });
            return events;
        }
        let mut events: Vec<ChatEvent> = calls
            .into_iter()
            .map(|(name, input)| ChatEvent::ToolCall {
                id: next_call_id(),
                name,
                input,
            })
            .collect();
        events.push(ChatEvent::Done {
            stop_reason: StopReason::ToolUse,
        });
        events
    }
}

/// An id for a tool call. An API hands these out; here Octa does. The start
/// time keeps ids of a session saved in an earlier run apart from new ones.
fn next_call_id() -> String {
    use std::sync::atomic::AtomicU64;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    static STARTED: std::sync::LazyLock<u128> = std::sync::LazyLock::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis())
    });
    format!(
        "octa-{:x}-{}",
        *STARTED,
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// One whole turn through a program: run it on `prompt` (see [`flatten`]),
/// hand the answer on and end with its tool calls, which are only looked for
/// when the prompt offered tools (`with_tools`).
///
/// `parse` reads one line of the program's output into an event, for a
/// program that prints a stream of JSON. `None` means the output is the
/// answer itself, line by line.
pub fn turn(
    program: &str,
    args: impl Fn(&Path) -> Vec<String>,
    prompt: &str,
    with_tools: bool,
    cancel: &AtomicBool,
    sink: &mut dyn FnMut(ChatEvent),
    parse: Option<fn(&str) -> Option<ChatEvent>>,
) -> Result<(), String> {
    let mut reply = Reply::new(with_tools);
    let mut first = true;
    let mut failed: Option<String> = None;
    run(program, args, prompt, cancel, &mut |line| {
        let event = match parse {
            Some(parse) => parse(&line),
            None => {
                // The line break goes in front of every line but the
                // first, so the answer does not end in one.
                let chunk = if first { line } else { format!("\n{line}") };
                first = false;
                Some(ChatEvent::TextDelta(chunk))
            }
        };
        match event {
            Some(ChatEvent::TextDelta(chunk)) => {
                if let Some(show) = reply.push(&chunk) {
                    sink(ChatEvent::TextDelta(show));
                }
            }
            Some(ChatEvent::Error(e)) => failed = Some(e),
            Some(other) => sink(other),
            None => {}
        }
    })?;
    if let Some(e) = failed {
        return Err(e);
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(());
    }
    // Seen with Codex: success, and not a word. A turn that ends in silence
    // looks like Octa hanging, so say it.
    if reply.text.trim().is_empty() {
        return Err(format!(
            "'{program}' finished without an answer. Ask again."
        ));
    }
    for event in reply.finish() {
        sink(event);
    }
    Ok(())
}

/// Start `program` in an empty temporary folder, write `prompt` to its stdin
/// and hand its stdout to `on_line` line by line.
///
/// `args` gets that folder, for programs that take their working directory as
/// an option. Returns `Ok` when the program exits with success or the turn was
/// cancelled, and the program's own error output otherwise.
pub fn run(
    program: &str,
    args: impl Fn(&Path) -> Vec<String>,
    prompt: &str,
    cancel: &AtomicBool,
    on_line: &mut dyn FnMut(String),
) -> Result<(), String> {
    let dir = tempfile::Builder::new()
        .prefix("octa-assistant-")
        .tempdir()
        .map_err(|e| format!("could not create a temporary folder: {e}"))?;

    let mut child = spawn(program, &args(dir.path()), dir.path())?;

    // Feed the prompt from its own thread: a program that prints before it
    // has read everything would otherwise block both sides on full pipes.
    let mut stdin = child.stdin.take().expect("stdin was piped");
    let prompt = prompt.to_string();
    std::thread::spawn(move || {
        let _ = stdin.write_all(prompt.as_bytes());
        // Dropping `stdin` closes the pipe, which is the program's end of input.
    });

    let mut stderr = child.stderr.take().expect("stderr was piped");
    let (err_tx, err_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        let _ = err_tx.send(text);
    });

    let stdout = child.stdout.take().expect("stdout was piped");
    let (line_tx, line_rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if line_tx.send(line).is_err() {
                break;
            }
        }
    });

    let started = std::time::Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            crate::app::chat::ollama::stop_child_group(&mut child);
            return Ok(());
        }
        if started.elapsed() > RUN_LIMIT {
            crate::app::chat::ollama::stop_child_group(&mut child);
            return Err(format!(
                "'{program}' did not finish within {} minutes and was stopped.",
                RUN_LIMIT.as_secs() / 60
            ));
        }
        match line_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => on_line(line),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let status = child
        .wait()
        .map_err(|e| format!("waiting for '{program}' failed: {e}"))?;
    if !status.success() {
        let err = err_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_default();
        return Err(failure_message(program, &status.to_string(), &err));
    }
    Ok(())
}

/// What to tell the user when the program failed: its own words, since they
/// name the cause (not signed in, plan does not cover it, unknown model).
fn failure_message(program: &str, status: &str, stderr: &str) -> String {
    let err = stderr.trim();
    if err.is_empty() {
        return format!("'{program}' stopped without an answer ({status}).");
    }
    // The end of the output is where a program says why it gave up.
    let start = err
        .char_indices()
        .rev()
        .nth(STDERR_KEEP)
        .map_or(0, |(i, _)| i);
    format!("'{program}' reported: {}", &err[start..])
}

pub(crate) fn spawn(
    program: &str,
    args: &[String],
    dir: &Path,
) -> Result<std::process::Child, String> {
    let build = |name: &str| {
        let mut cmd = Command::new(name);
        cmd.args(args)
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            // Its own process group, so cancelling stops the helpers it
            // starts as well (see `ollama::stop_child_group`).
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        #[cfg(windows)]
        {
            // No console window flashing up behind Octa.
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        cmd
    };
    let not_found = |e: &std::io::Error| e.kind() == std::io::ErrorKind::NotFound;
    let result = match build(program).spawn() {
        // ponytail: npm installs these programs on Windows as `<name>.cmd`,
        // which `Command` does not find by the bare name. One retry covers
        // it; a full PATHEXT search only if another wrapper kind turns up.
        Err(e) if cfg!(windows) && not_found(&e) => build(&format!("{program}.cmd")).spawn(),
        other => other,
    };
    result.map_err(|e| {
        if not_found(&e) {
            format!(
                "'{program}' was not found. Install it and sign in with it, or name its full \
path in the model profile."
            )
        } else {
            format!("could not start '{program}': {e}")
        }
    })
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
