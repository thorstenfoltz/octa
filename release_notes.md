A maintenance release for the local assistant and for Linux installs. Ollama
was quietly answering a question it could no longer see, and on some models
refusing outright; on WSL the Open and Save buttons did nothing at all; and a
fresh install could keep reporting the previous version. All of that is fixed.
On the way, the assistant shows how fast a local model is answering, and a
clock on the spinner covers the minutes a CPU spends reading the prompt before
it says anything.

## What's new

### See how fast Ollama is answering

The token meter in the assistant header now shows the generation speed of an
Ollama answer in **tokens per second**. While the answer streams the number is
Octa's own live count of the tokens received so far, so on a machine where the
model runs on the CPU you can watch it work; when the turn ends it switches to
Ollama's exact figure, the one `ollama run --verbose` prints as *eval rate*. A
rate that drops from tens of tokens per second to a handful means part of the
model no longer fits your GPU. Hosted providers stream in chunks rather than
tokens and send no timing, so nothing changes for them.

### A clock for the silent part of a turn

The rate above starts with the first generated token. Before that Ollama is
reading the prompt and sends nothing at all, and on a CPU that is the long part
of a Data-mode turn: several minutes for the tool definitions and file context
a first request carries, with only a spinner to look at. The spinner label now
carries a clock, "Thinking... 4:32", that restarts with each phase of the turn
(reading, running tools, answering), so a model that is still reading is
distinguishable from a connection that died. Once the turn ends, the meter's
hover text says how the wait was spent: "Last prompt: 7,012 tokens, read at 16
tokens/s before the first token of the answer." A later round of the same turn
reads far fewer tokens, thanks to Ollama's prompt cache, which is why those
rounds start faster.

## Fixes

- **Ollama lost your question.** Ollama sizes its context window from the
  machine's VRAM, 4k tokens on a modest box, and the assistant's first request
  already carries about 6.5k tokens of tool definitions. When the conversation
  did not fit, Ollama dropped the oldest messages, and in a round that had run
  a tool the oldest one was your question. Most models answered a question they
  could no longer read; Qwen 3.8 and other models with a built-in prompt
  renderer refused with `500 no user query found in messages`. Octa now asks
  for a 32k window on every turn. That setting has no home in the
  OpenAI-compatible endpoint Octa used, so the Ollama connection now speaks
  Ollama's own API; nothing to configure.
- **Explain gave up after two minutes on a CPU.** Ollama sends nothing, not
  even the response headers, until it has loaded the model and read the whole
  prompt, and on a CPU or a partly offloaded model that takes minutes for a
  Data-mode turn. The two-minute limit that protects against a hosted API that
  never answers cut exactly those turns off with `timeout: receive response`.
  An Ollama turn now gets thirty minutes to start; **Cancel** works the whole
  time.
- **The token meter sat under the header buttons.** On a panel of the default
  width the header buttons painted over the token counter. The assistant panel
  opens wider by default, and the header lays its buttons out first and gives
  the title and meter what is left, truncating the meter with its full text on
  hover instead of hiding it.
- **WSL: Open, Save as and Export did nothing.** Octa asks the desktop for a
  file dialog through the XDG desktop portal, falling back to `zenity`. A
  default WSL install has neither, and the request failed the same way a
  cancelled dialog does: no window, no error. Octa now checks at startup and
  puts a note in the status bar naming the package to install
  (`xdg-desktop-portal-gtk`, or `zenity`). Passing a file on the command line
  worked all along and still does.
- **A fresh install reported the previous version.** The installer was writing
  the new binary where it said; an older `octa` earlier in `PATH`
  (`~/.local/bin` or `~/.cargo/bin` after a system-wide install, `/usr/bin`
  on Arch) was what the shell kept running. `install.sh` now prints the
  version it installed and warns when `octa` still resolves somewhere else or
  the prefix is not on `PATH`, including under `sudo`, where the script's own
  `PATH` is not yours. The troubleshooting page explains the `type -a octa`
  and `hash -r` checks.

## Under the hood

- **The Ollama adapter is native `/api/chat`**, streamed as newline-delimited
  JSON, rather than a reuse of the OpenAI chat-completions code. `num_ctx` has
  no field in Ollama's OpenAI-compatible request and unknown keys are dropped,
  so the native endpoint is the only way to raise the window per request.
  Thinking uses Ollama's `think` level; a numeric budget is refused locally
  with a plain message rather than by the server with a cryptic one. The
  streaming loop was split so the SSE providers and the NDJSON one share the
  HTTP part, and the line parser is a pure function with tests for tool calls,
  mid-stream errors, the live rate and the prompt-reading speed.
- **The first-token timeout is per provider** instead of one number for all:
  hosted APIs keep two minutes, Ollama gets thirty.
- **The file-dialog probe** checks for a session bus plus an installed portal
  activation file, or an executable `zenity` on `PATH`; a non-executable file
  of that name does not count, and a test says so.
- **CI runs on pushes to `master`** as well as on pull requests, so the CI
  badge reflects `master` rather than whichever pull request ran last.
- **The committed `Cargo.lock` carried a placeholder version** that differed
  from `Cargo.toml`, so every local build dirtied the tree. Corrected.
