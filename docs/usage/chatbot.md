# Chat Assistant

The chat assistant is a docked panel where you talk to a large language model
(LLM) in plain language, and it answers questions about your data by driving
Octa's own tools behind the scenes. It is the in-application sibling of the
[MCP server](../mcp/index.md): it reuses the same data tools (read, schema,
profile, run SQL, find duplicates, search, diff, and more), but here they run
against the files you already have open, with no external client to set up. It
can also build a chart from your data, save results to a file, and read or edit
the [text, code, and Markdown files](#text-code-and-markdown-files) you have open.

Everything is local-first and provider-agnostic: pick a cloud model (Claude,
GPT, Gemini) or run a model entirely on your own machine with
[Ollama](#using-ollama).

<!-- SCREENSHOT: chat-panel.png: The chat assistant docked on the right of the table view. -->
![Chat Assistant](../assets/screenshots/chatbot.png)

## Opening and closing the panel

There are three ways to open it, and it stays where it is across tabs:

- the **Assistant** entry under the **Analyse** menu,
- the keyboard shortcut <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>A</kbd>
  (rebindable under Settings, like every other shortcut),
- pressing the same shortcut again, the **x** button in the panel header, or
  the menu entry, to close it.

You can dock the panel to the right (the default), left, bottom, or top of the
window from Settings.

## Choosing a provider and model

Octa speaks to five kinds of backend, one at a time:

| Provider               | Notes                                                                                                                                            |
|------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------|
| **Ollama (local)**     | Runs entirely on your machine. No API key. Octa can start the server and lists the models you have installed. See [Using Ollama](#using-ollama). |
| **Anthropic (Claude)** | Claude models via the Anthropic Messages API.                                                                                                    |
| **OpenAI**             | GPT models via the Chat Completions API.                                                                                                         |
| **OpenAI-compatible**  | Any other endpoint that speaks the OpenAI dialect: OpenRouter, Groq, LM Studio, a self-hosted gateway. Set a Base URL.                           |
| **Google Gemini**      | Gemini models via the `generateContent` API.                                                                                                     |

Switch the active provider and pick a model straight from the panel header. The
model picker offers a dropdown of common models for the provider plus a
free-text box, so you can always type the exact model name (model names change
often). Octa remembers the last model you used per provider, so flipping
between, say, Claude and a local Ollama model never makes you retype anything.

The dropdown's preset list is **hand-editable**. It lives in a plain
`models.toml` next to your `settings.toml` (`~/.config/octa/` on Linux,
`~/Library/Application Support/Octa/` on macOS, `%APPDATA%\Octa\` on Windows),
seeded on first run from Octa's built-in lists. Add or remove model names there
and they show up without waiting for a new Octa release. After editing, click
**Reload models.toml** in the Chat / Assistant settings to pick up the change
without restarting. A missing or empty entry falls back to the built-in list, so
a stray edit can never leave a provider with no usable model.

!!! tip "Pick a tool-capable model"
    The assistant works by calling tools (run SQL, profile, search, ...). Very
    small or older models sometimes ignore tool calls and just chat. If the
    assistant answers without ever looking at your data, switch to a more
    capable model.

## Settings

The assistant's settings live in Octa's main **Settings** dialog under the
**Chat / Assistant** section. Click **Settings** in the panel header to jump
straight there. From there you can set:

- **Provider** and **Default model** (per provider).
- **Base URL** (OpenAI-compatible) or **Ollama URL** (Ollama).
- **Temperature** (0 to 2; default 0, which is best for data tasks where you
  want consistent, focused answers).
- **Max tool iterations**: how many rounds of tool calls the assistant may run
  within a single message before it has to stop. It is a safety guard against
  runaway loops, not a limit you normally need to touch (default 12).
- **Max response tokens**: a cap on the length of each reply, with an
  **Unlimited** checkbox. Unlimited lets the model use its own maximum.
- **Panel position** (right / left / bottom / top).
- **Export folder**: where the assistant writes files it creates. See
  [Saving results and charts](#saving-results-and-charts).
- **API keys**, with a per-provider overview of which providers are configured.

!!! note "Click Apply"
    Settings, including a newly entered API key, take effect when you click
    **Apply**. (A key saved to your OS keyring is stored at once, but the
    plaintext fallback and every other setting commit on Apply.)

## Setting an API key

In the **Chat / Assistant** settings section, paste your key into the **API
key** field for the selected provider and click **Save key**, then **Apply**.
Each provider has its own key; you only enter it once, and entering a new key
for a provider replaces that provider's old key (keys for other providers
are kept). A small **Stored keys** list shows, for every provider, whether a key
is configured and where it resolves from.

Octa resolves a key in this order:

1. an **environment variable** (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`,
   `GEMINI_API_KEY`, or `OCTA_OPENAI_COMPAT_API_KEY`),
2. your operating system's **secret store** (keyring),
3. a **plaintext** entry in Octa's `settings.toml`.

When you save a key, Octa tries the OS keyring first. If that is not available
(for example on a minimal or headless box), it falls back to storing the key in
plaintext and tells you exactly which file it went into, so there is never any
doubt about where your secret lives.

!!! note "Where keys live"
    - On Linux the keyring uses the freedesktop **Secret Service** (GNOME
      Keyring / KWallet), so keys persist across reboots. macOS uses the
      Keychain; Windows uses the Credential Manager.
    - The plaintext fallback (only used when no keyring is reachable) lives in
      `settings.toml`: `~/.config/octa/` on Linux,
      `~/Library/Application Support/Octa/` on macOS, `%APPDATA%\Octa\` on
      Windows.
    - To avoid any on-disk secret, set the matching environment variable
      instead; it always wins.

## Using Ollama

[Ollama](https://ollama.com) runs open models (Llama, Mistral, Qwen, Gemma,
and many more) entirely on your own machine. Nothing you ask the assistant
leaves your computer, and there is no API key or account.

### Install Ollama

Download it from [ollama.com/download](https://ollama.com/download) (Linux,
macOS, Windows). On Linux you can also use the one-line installer:

```bash
curl -fsSL https://ollama.com/install.sh | sh
```

### Pull at least one model

Ollama starts with no models installed. Pull one from a terminal (this is a
one-off download, a few gigabytes):

```bash
ollama pull llama4        # a good small default
ollama pull qwen3.5         # strong at tool use / structured output
```

Browse the full catalogue at [ollama.com/library](https://ollama.com/library).
Any model you have pulled shows up in Octa.

### Use it in Octa

1. In the panel header, set the provider to **Ollama (local)**.
2. If the server is not running, Octa shows **Ollama is not running** and a
   **Start Ollama** button. Click it and Octa launches `ollama serve` in the
   background for you. (You can also start it yourself with `ollama serve`.)
3. The **model dropdown** fills with the models you have pulled. Pick one.
   Click **Refresh models** after pulling a new model so it appears.
4. Type your question and send. No key, no base URL to configure.

The status updates on its own every few seconds, so if you stop Ollama
elsewhere the panel notices. A **Stop Ollama** button stops the local server,
and Octa automatically stops the Ollama server **it started** when you close
Octa, so a loaded model never lingers in memory after you quit.

If you run Ollama on another host or a non-default port, set the **Ollama URL**
in settings (default `http://localhost:11434`).

!!! warning "If a model fails to load"
    A `500 ... llama-server binary not found` error on the first request is an
    Ollama installation problem (its model-runner is missing), not an Octa one.
    Octa shows you Ollama's own error verbatim. Reinstall or update Ollama, or
    test it directly with `ollama run <model> "hi"`.

## What the assistant can access

By design, the assistant can only see and read data you have **open in Octa**.
This keeps it from quietly reaching into arbitrary files on your disk.

- Every open tab is available as context. The panel shows a row of tab
  chips under the header, with the active tab highlighted, so you always know
  what the assistant can see.
- Each tab has a short, stable handle (`#1`, `#2`, ...). To steer the assistant
  at a specific tab, click its chip, type an `@` mention (`@#2`, or a tab name
  or `@column`), or just describe it. This is handy when two tabs have similar
  or identical names.
- For a multi-table source you have open (an Excel workbook, a DuckDB or SQLite
  database), the assistant can reach the other sheets or tables of that same
  file, even if only one is open in a tab.
- It cannot open files that are not open in Octa. If you ask about a file
  that is not open, the assistant will tell you to open it first
  (**File -> Open**).

<!-- SCREENSHOT: chat-tab-chips.png: The chat panel header with two open tabs shown as chips ("#1 sales.csv" highlighted as active, "#2 returns.csv"), illustrating how the assistant addresses multiple open tables. -->

## What the assistant can do

It can read, query (DuckDB SQL), profile, describe, sample, search, find
duplicates, diff two tables, compare and export schemas, validate against a
schema, and list unique columns. Beyond reading, it can:

- Run SQL across several open tabs at once. Ask it to join, union, or
  cross-check two or more open tables and it builds one DuckDB query over them.
- Save results to a file. It can write a query result, a transformed table,
  or a converted copy to CSV, TSV, Parquet, JSON, Excel, and more. See
  [Saving results and charts](#saving-results-and-charts).
- Create a chart. Ask for a histogram, bar, line, scatter, or box plot of
  your data and it renders one and saves it as a PNG, PDF, or SVG.

It deliberately does not write back into an open tab. If you ask it to
change a table you have open, it will say so and offer to save a new file
instead, so your in-tab edits are never modified by the assistant.

## Text, code, and Markdown files

The assistant is not limited to tables. Plain-text, source-code, and Markdown
files you have open (anything Octa loads into the Raw or Markdown view) are
available to it as well. Open such a file and you can ask the assistant to:

- read and summarise it, explain what a piece of code does, or answer questions
  about its contents,
- refactor, reformat, translate, or otherwise rewrite it.

When it writes the result it either saves a **new file** in your
[Export folder](#saving-results-and-charts) or, if you ask, writes the change
**back to the open file on disk**. Octa's live editor does not refresh on its
own in that case: reload the tab with <kbd>Ctrl</kbd>+<kbd>R</kbd> to see the new
content.

!!! note "It still cannot touch files you have not opened"
    Just like with tables, the assistant only reaches files that are **open in
    Octa**. Writing back to disk replaces the open file you pointed it at, never
    an arbitrary path you did not open.

## Saving results and charts

When the assistant writes a file (a new CSV, a chart, a converted copy) it
writes into your configured **Export folder** (Settings -> Chat / Assistant;
the default is your Downloads folder). Give the assistant just a filename and it
lands there. Writes anywhere else on disk are refused: the export folder is the
only place the assistant can create files, and writing back to a file you have
open in a tab is the only exception. Change the export folder in Settings if
you want new files somewhere else.

Examples of things you can ask:

- "Join the two tables I have open on `customer_id` and save the result as
  `joined.csv`."
- "Convert the active tab to Parquet."
- "Make a bar chart of total sales by region and save it as a PNG."

## During a turn

Replies stream in live. When the model decides to call a tool you see a spinner
("Running tools") and, once it answers, a collapsible **Tool call** row you can
expand to see the exact arguments and the raw result. If a tool fails, its
result is shown **expanded** so you can read the error straight away. Press
**Cancel** to stop a turn at any point; the panel frees up immediately.

## Sessions

Every conversation is saved automatically. Use **New chat** to start fresh and
**History** to reopen a past conversation, delete a single one with its **x**,
or wipe them all with **Clear all**. Sessions are stored as JSON files under
your config directory:

- Linux: `~/.config/octa/chat_sessions/`
- macOS: `~/Library/Application Support/Octa/chat_sessions/`
- Windows: `%APPDATA%\Octa\chat_sessions\`

## Privacy

The assistant sends your prompts, a short description of your open tabs (names,
row/column counts, and column types), and the results of any tools it runs to
the provider you chose. Cell data leaves your machine only when a tool returns
rows (for example after `read_table` or `run_sql`) and the model needs them to
answer. If that matters for your data, use **Ollama** (or point the
**OpenAI-compatible** provider at a local LM Studio model) so nothing leaves
your machine at all.
