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

## Model profiles

A **profile** is one saved setup: a provider, a model, a temperature, an
optional thinking budget, and a name you choose. The panel header has a single
**Profile** dropdown that switches between them.

You can create as many as you like, **including several for the same
provider**. A typical set:

| Profile          | Provider  | Model    | Temperature | Thinking |
|------------------|-----------|----------|-------------|----------|
| Opus, deep       | Anthropic | Opus     | (empty)     | `xhigh`  |
| Sonnet, quick    | Anthropic | Sonnet   | (empty)     | `low`    |
| Local, free      | Ollama    | llama3.2 | 0.2         | (none)   |
| GPT, high effort | OpenAI    | GPT      | (empty)     | `high`   |

Switching model is then one click, with no re-editing of settings.

Octa speaks to five kinds of backend:

| Provider               | Notes                                                                                                                                                                                                                                                                                                                                                                        |
|------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Ollama (local)**     | Runs entirely on your machine. No API key. Octa can start the server and lists the models you have installed. See [Using Ollama](#using-ollama).                                                                                                                                                                                                                             |
| **Anthropic (Claude)** | Claude models via the Anthropic Messages API.                                                                                                                                                                                                                                                                                                                                |
| **OpenAI**             | GPT models via the Responses API (reasoning + tools together).                                                                                                                                                                                                                                                                                                               |
| **OpenAI-compatible**  | Any other endpoint that speaks the OpenAI dialect: OpenRouter, Groq, LM Studio, a self-hosted gateway. Set a Base URL. This is how you reach the open-weight models (DeepSeek, GLM, Kimi, Qwen, Nemotron, gpt-oss, Gemma); the model dropdown lists current ones. See [When an OpenAI-compatible endpoint does not work](#when-an-openai-compatible-endpoint-does-not-work). |
| **Google Gemini**      | Gemini models via the `generateContent` API.                                                                                                                                                                                                                                                                                                                                 |

### Creating a profile

In **Settings → Chat / Assistant**, under **Model profiles**, fill in the form
and click **Save profile**. The **Profiles...** button in the panel header
jumps straight there. Each profile has:

- **Name**: how it appears in the dropdown ("Opus, deep").
- **Description** (optional): a note to yourself, shown beside the name in the
  dropdown ("cheap, for bulk work").
- **Provider** and **Model**: a dropdown of common models plus a free-text box,
  so you can always type the exact model name (model names change often).
- **Temperature** (0 to 2). **Leave it empty and no temperature is sent at
  all.** That is not the same as sending 0: the newest models (Claude Opus 4.7
  and later) reject the parameter outright and answer with an error, so an
  empty field is what makes them work. When you do give a number, 0 is best for
  data tasks where you want consistent, focused answers.
- **Thinking / reasoning**: see below.
- **Base URL** (OpenAI-compatible and Ollama only).
- **Use its own API key**: see [Setting an API key](#setting-an-api-key).
- **Allow writes**: whether this profile's assistant may modify files, open
  tabs, and writable database connections. Off by default; see
  [Editing open data](#editing-open-data).

**Edit** re-opens a profile in the same form; **Remove** deletes it, along with
its own key if it had one. The profile the assistant is currently using is
marked with an asterisk.

### Testing a profile

**Test connection**, next to **Save profile**, sends one tiny message with
exactly the settings on screen (including edits you have not saved yet) and
shows what comes back. It uses the same code path as a real question, so it
catches a wrong or missing key, a model name that does not exist, a thinking
value in the wrong dialect, a temperature the model refuses, and an Ollama
server that is not running. A green line means the profile works; a red one
carries the provider's own error message.

Under a failed test, Octa adds one line naming the field to fix. What it says
for the failures that actually happen:

| The error mentions                     | What it means                   | Fix                                                                     |
|----------------------------------------|---------------------------------|-------------------------------------------------------------------------|
| `temperature`                          | The model refuses the parameter | Clear the **Temperature** field.                                        |
| `reasoning` / `thinking` / `effort`    | Wrong thinking value            | Hover the field for what this provider takes; empty turns thinking off. |
| `401` / `403` / `api key`              | The key was rejected            | Set the shared key under **API keys**, or give the profile its own.     |
| `429` / `quota` / `credit`             | Out of allowance, or throttled  | Nothing in the profile is wrong; it is the account.                     |
| `404` / `no endpoints` / `not found`   | The endpoint has no such model  | Use the model name **this** endpoint uses (see below).                  |
| `connection refused` / `dns` / timeout | The server was never reached    | Check the **Base URL** and your network. For Ollama, start the server.  |

### When an OpenAI-compatible endpoint does not work

This provider is the one with the most ways to go wrong, because "speaks the
OpenAI dialect" is all the gateways have in common. In order of how often each
is the actual cause:

1. **The Base URL is not the API root.** It almost always has to end in `/v1`
   (`https://openrouter.ai/api/v1`), and never in `/chat/completions`, because
   Octa appends the path itself. A wrong root shows up as a 404.
2. **The model name is the gateway's, not the vendor's.** OpenRouter says
   `deepseek/deepseek-v4-pro`; the same model is `deepseek-ai/DeepSeek-V4-Pro`
   on one host, `deepseek-v4` on another, and a local file path under vLLM. The
   dropdown is a list of OpenRouter ids as a starting point; the free-text field
   below it is what you use for anything else.
3. **The key is missing or is the wrong provider's.** OpenAI-compatible has its
   own key slot: pick **OpenAI-compatible** in the **API keys** dropdown, not
   OpenAI. Some local gateways (LM Studio, vLLM without auth) take any string.
4. **The gateway does not do tool calling.** Octa's assistant is agentic: it
   answers by calling tools. A model or gateway without function-calling support
   connects and chats, but never reads your data. Test connection passes and the
   assistant still seems useless: that is this.
5. **Thinking is not supported.** The **Thinking / reasoning** value goes out as
   `reasoning_effort`, which many gateways reject. Empty it if you get a 400.
6. **Temperature is not supported.** Same story; empty the field.

For a local Ollama server, use the **Ollama** provider rather than
OpenAI-compatible: it discovers your installed models and can start the server
for you.

Your existing setup is carried over automatically: on first run after
upgrading, Octa turns your old provider, model and temperature into a single
profile, so nothing changes until you add more.

### Thinking / reasoning

The **Thinking / reasoning** field is free text, passed to the provider as-is.
**Type a word, not a number**: every current model takes an effort level, and
the value's *shape* decides which knob Octa sends.

| Provider                       | A word means                                                                  | A number means                                                                                                                      |
|--------------------------------|-------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------|
| **OpenAI**                     | `reasoning.effort` — `none` `low` `medium` `high` `xhigh`                     | Nothing OpenAI accepts; Octa says so before sending.                                                                                |
| **Anthropic**                  | `output_config.effort` — `low` `medium` `high` `xhigh` `max` (default `high`) | A thinking-token budget, minimum 1024. Only Claude 4.5 and older, such as Haiku 4.5, still take one; Opus 4.7 and later answer 400. |
| **Gemini**                     | `thinkingConfig.thinkingLevel` — `minimal` `low` `medium` `high` (Gemini 3+)  | `thinkingConfig.thinkingBudget` for Gemini 2.5: `0` off, `-1` let the model decide.                                                 |
| **OpenAI-compatible / Ollama** | `reasoning_effort`                                                            | Sent as-is; some gateways accept a number here.                                                                                     |

Leave it **empty** for no thinking, which is the default. The tooltip on the
field says all of this for whichever provider the profile uses, so you do not
have to remember which is which.

So "is 8000 low, medium or high?" no longer has to be answered: on a current
model you type `medium` and mean it. A number is a token allowance, and only
the older Claude and Gemini generations still read it.

The field is free text rather than a fixed list on purpose: providers keep
adding levels, and a hard-coded dropdown would go stale. The cost is that a
wrong value is only caught when it is used, so a level a provider does not know
comes back as that provider's own error message. **Test connection** is the
quickest way to find that out.

When Anthropic gets a *token budget*, Octa also raises the response-token cap
above it and pins temperature to 1 if the profile sends a temperature at all
(an empty field still sends none), because the API demands both. An Anthropic
*effort word* needs none of that and leaves the rest of the request alone. For
OpenAI, setting an effort level omits temperature from the request (reasoning
models reject it). You do not need to do anything: Octa adjusts the request for
you.

OpenAI requests go through the **Responses API** (`/v1/responses`), which is
the endpoint where reasoning and tools work together on current models
(gpt-5.x refuses `reasoning_effort` with tools on the older Chat Completions
endpoint). OpenAI-compatible providers and Ollama stay on Chat Completions.

### The preset model list

The model dropdown's preset list is **hand-editable**. It lives in a plain
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
straight there.

Provider, model, temperature and thinking belong to a
[model profile](#model-profiles), not to this section. What is set here applies
to every profile:

- **Ollama URL**: the address of the local Ollama server Octa probes and can
  start or stop. A profile may override it with its own Base URL.
- **Max tool iterations**: how many rounds of tool calls the assistant may run
  within a single message before it has to stop. It is a safety guard against
  runaway loops, not a limit you normally need to touch (default 12).
- **Max response tokens**: a cap on the length of each reply, with an
  **Unlimited** checkbox. Unlimited lets the model use its own maximum.
- **Result row limit** (default 200): how many rows a tool result, such as a
  SQL query, puts into the assistant's context. The query still runs over
  every row; this only caps what the model sees so a large result never floods
  the conversation. When a result is capped, the assistant tells you how many
  of how many rows it saw and offers to write the full result to a file or a
  tab. Tick the **Unlimited** checkbox for no cap (it may flood the chat).
- **Panel position** (right / left / bottom / top).
- **Export folder**: where the assistant writes files it creates. See
  [Saving results and charts](#saving-results-and-charts).
- **API keys**, with a per-provider overview of which providers are configured.

!!! note "Click Apply"
    Settings, including a newly entered API key, take effect when you click
    **Apply**. (A key saved to your OS keyring is stored at once, but the
    plaintext fallback and every other setting commit on Apply.)

## Setting an API key

In the **Chat / Assistant** settings section, open **API keys**, pick the
provider in the **Provider** dropdown, paste your key into the **API key**
field and click **Save key**, then **Apply**. The dropdown is independent of
which profile the panel is using, so every provider's key is reachable without
switching profiles.
Each provider has its own key; you only enter it once, and entering a new key
for a provider replaces that provider's old key (keys for other providers
are kept). A small **Stored keys** list shows, for every provider, whether a key
is configured and where it resolves from.

**Keys are shared by every profile of a provider.** Three Anthropic profiles
all use the one Anthropic key, which is what you want almost always: you enter
it once.

### A key for one profile only

A profile can opt out of the shared key. Tick **Use its own API key** on the
profile and give it a key of its own. Use it when a profile should bill a
different account, or when you want one profile on a spend-limited key.

The profile's key is stored under the profile, separately from the shared
provider key, so the two never overwrite each other, and deleting the profile
deletes its key. Untick the box and the key is removed, and the profile goes
back to the shared one.

### Where a key comes from

Octa resolves a shared provider key in this order:

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
    - A **profile's own key** follows the same keyring-then-plaintext chain,
      but has no environment variable: it is an explicit override, and the
      environment variable already backs the shared key.

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
- It can read and list **cloud objects** in buckets you have saved as a
  connection (**Settings > Cloud storage**) by URL (`s3://`, `az://`, `gs://`).
  Buckets you have not saved are refused, so the assistant stays confined to
  the clouds you configured. It can also **write** to those buckets once
  the connection's **Allow writes** is on. See
  [Cloud Storage](cloud-storage.md).

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

## Editing open data

With **Allow writes** ticked on the active model profile (see below), the
assistant can change your data directly:

- Edit the open tab live (`edit_open_tab`). Ask it to add a computed column
  (a DuckDB expression such as a moving average), insert rows, set cells,
  delete rows, or drop columns, and the change appears in the tab immediately.
  It is a normal edit, so <kbd>Ctrl</kbd>+<kbd>Z</kbd> undoes it, and nothing
  is written to disk until **you** save.
- Edit a file on disk that is not open (`edit_table`), including adding or
  dropping a column on a DuckDB, SQLite, or GeoPackage file (a schema change).
- Write to live databases (`write_db_table`, `query_db` mutations, and the
  server-to-server `copy_db_table`) whose connection also has **Allow
  writes** on under **Settings > Databases**; both switches must permit it.

**Write permission is per model profile and off by default.** A profile
without **Allow writes** never even sees the write tools: ask it to change a
table and it says so and offers to save the result as a new file in your
[Export folder](#saving-results-and-charts) instead. Tick **Allow writes** on
the profile (Settings > Chat / Assistant) to allow in-place edits - you might
keep a trusted "editor" profile with writes on and leave your everyday one
read-only. The global **Write protection** switch no longer applies to the
assistant; it still governs GUI file saves and the MCP server default.

Before the assistant (or a schema-changing database save) overwrites an
existing file, Octa first copies it to a timestamped `.bak-*` sidecar next to
it (controlled by **Back up before modifying**, on by default, under
**Settings > Chat / Assistant**). Routine manual saves are not backed up.

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

## Exporting a conversation

The **Export** button in the panel header saves the current conversation to a
file you choose. The save dialog offers two formats, picked by the file
extension:

- **Markdown (`.md`)**: a readable transcript. It includes your prompts, the
  assistant's replies, every SQL query it sent (rendered in `sql` code
  blocks), any other tool calls, and each tool's result. Results are truncated
  to keep the file manageable.
- **JSON (`.json`)**: the exact saved session, the same format Octa stores on
  disk, for archiving or feeding into other tools.

Use Markdown when you want a human-readable record of the analysis (including
the SQL that produced each answer); use JSON when you want a faithful,
machine-readable copy.

## Saved prompts

The **Prompts** button next to **Send** opens a small manager window for
reusable prompts. **Save current prompt...** names and stores whatever is in
the input box; each saved prompt then has **Insert** (drop it into the input)
and **x** (delete). The window has the usual minimise / maximise / close
controls and is resizable. Prompts persist across sessions as
`chat_prompts.json` in your config directory, the same way SQL snippets do.
Handy for repeated tasks like "profile every open table" or a house-style
analysis request.

## Tool-call audit log

For auditing, you can record **every tool the assistant runs**. Turn on
**Settings -> Chat / Assistant -> Tool-call audit log** (off by default).
While on, each tool call appends one JSON line (tool name, argument and
result byte counts, duration, error flag, timestamp) to a per-session file:

- Linux: `~/.config/octa/chat_audit/<session-id>.jsonl`
- macOS: `~/Library/Application Support/Octa/chat_audit/<session-id>.jsonl`
- Windows: `%APPDATA%\Octa\chat_audit\<session-id>.jsonl`

The log only records *that* a tool ran and how big its input/output were, not
the cell contents themselves. Because it grows over time, Octa shows a
one-time warning at startup once the audit files exceed a size limit
(**Warn when logs exceed**, default 10 MB; the warning can be turned off).
Delete the files in `chat_audit/` to reset.

## Privacy

The assistant sends your prompts, a short description of your open tabs (names,
row/column counts, and column types), and the results of any tools it runs to
the provider you chose. Cell data leaves your machine only when a tool returns
rows (for example after `read_table` or `run_sql`) and the model needs them to
answer. If that matters for your data, use **Ollama** (or point the
**OpenAI-compatible** provider at a local LM Studio model) so nothing leaves
your machine at all.
