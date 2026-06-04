# Release notes

This release adds the **Chat Assistant**: a docked panel where you ask
questions about your data in plain language and a large language model answers
by driving Octa's own tools (read, run SQL, profile, search, diff, chart, and
more) against the tables you already have open. It is the in-application sibling
of the MCP server, and it works with cloud models or a model running entirely
on your own machine.

Open it from the **Analyse -> Assistant** menu or with
<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>A</kbd>, and dock it to any edge of the
window. See **Usage -> Chat Assistant** in the docs for the full guide.

## Bring your own model

The assistant talks to five kinds of backend, one at a time, and remembers the
model you picked per provider:

- **Ollama (local)**: open models running on your own machine, no key, nothing
  leaves your computer.
- **Anthropic (Claude)**, **OpenAI (GPT)**, **Google Gemini**.
- **OpenAI-compatible**: any endpoint that speaks the OpenAI dialect
  (OpenRouter, Groq, LM Studio, a self-hosted gateway) via a base URL.

Each provider has its own API key. Keys are kept in your operating system's
secret store when one is available (the Secret Service on Linux, the Keychain on
macOS, the Credential Manager on Windows), with an environment variable taking
precedence and a clearly-labelled plaintext fallback when no keyring exists. A
per-provider overview shows at a glance which providers are configured.

## First-class Ollama

[Ollama](https://ollama.com) is built in. Pick **Ollama (local)** and Octa can:

- start the server for you (**Start Ollama**) and stop it (**Stop Ollama**),
- list the models you have pulled and refresh that list on demand,
- keep the running/stopped status up to date on its own.

When Octa starts the Ollama server, it also shuts it down (and the loaded model)
when you close Octa, so nothing lingers in memory after you quit.

## Works on the data you have open

The assistant only sees and reads data you have **open in Octa**, so it never
reaches into arbitrary files on your disk. Open tabs appear as **chips** in the
panel header (the active one highlighted), each with a short handle (`#1`,
`#2`, ...). Point the assistant at a specific table by clicking its chip, typing
an `@` mention (`@#2`, a tab name, or `@column`), or just describing it. If a
file is not open, the assistant tells you to open it first.

It can reach the other sheets or tables of an open Excel workbook or
DuckDB/SQLite database, and it can run a single DuckDB query that **joins across
several open tabs** at once.

## Beyond reading: write results and draw charts

As well as reading, querying, profiling, searching, diffing, and exporting
schemas, the assistant can:

- **Save results to a file** (CSV, TSV, Parquet, JSON, Excel, ...): a query
  result, a transformed table, or a converted copy.
- **Create a chart** (histogram, bar, line, scatter, box) and save it as PNG,
  PDF, or SVG.

Files it creates go to your configured **Export folder** (Settings -> Chat /
Assistant; defaults to your Downloads folder) when you give just a filename, or
to a specific path when you ask for one. The assistant never writes back into a
tab you have open, so your in-tab edits are always yours.

## In the panel

- Replies stream in live; tool calls show as expandable rows with the exact
  arguments and raw result, and failures are shown expanded so you see the
  problem at once.
- **Cancel** stops a turn immediately.
- **Copy** any message with <kbd>Ctrl</kbd>+<kbd>C</kbd> or a right-click menu,
  or copy the whole conversation from the header.
- Conversations are saved automatically; reopen past chats from **History**,
  start a fresh one with **New chat**.

## Settings and localisation

All of the assistant's settings live in Octa's main **Settings** dialog under
**Chat / Assistant** (provider, model, temperature - default 0 for focused
answers, response-length cap with an Unlimited option, panel position, export
folder, and API keys). Every label has a hover hint, and the whole chat
interface is translated into all of Octa's supported languages.

## Privacy

With a cloud provider, your prompts, a short description of your open tabs, and
the results of any tools the assistant runs are sent to that provider; cell data
leaves your machine only when a tool returns rows the model needs. Choose
**Ollama** (or point the OpenAI-compatible provider at a local LM Studio model)
to keep everything on your own machine.
