---
hide:
  - navigation
  - toc
---

# Octa

<!-- SCREENSHOT: hero-table-view.png: Octa's main window with a sample Parquet
file open in Table view. Light theme. Show multiple column types (numeric, date,
text), maybe a search bar with some filter applied. Aim for a friendly "this is
what data exploration looks like" hero shot. -->
![Octa main window in Table view](assets/screenshots/hero-table-view.png)

**Octa** is a native desktop application for viewing and editing tabular
data files. It opens Parquet, CSV, JSON, SQLite, DuckDB, Excel, and
around twenty more formats in a fast spreadsheet-like view, with
sorting, filtering, full-text search, inline editing, SQL queries, and
file comparison.

It also doubles as a command-line tool and an MCP server, so models
like Claude can answer questions about your local files.

[Get Octa :material-download:](getting-started/installation.md){ .md-button .md-button--primary }
[First Steps :material-rocket-launch:](getting-started/first-steps.md){ .md-button }

---

## What it's good for

<div class="grid cards" markdown>

- :material-table-eye:{ .lg .middle } **Look at data quickly**

    ---

    Drag a Parquet, a Stata `.dta`, a SQLite database, an Excel
    workbook, Octa figures out the format and opens it in a table.
    Multi-million-row Parquet files stream in the background while
    you scroll.

    [:octicons-arrow-right-24: Supported formats](getting-started/supported-formats.md)

- :material-database-search:{ .lg .middle } **Run SQL against any file**

    ---

    Every open file is exposed to DuckDB as a temp table called
    `data`. No schema setup, no import step needed. Press Ctrl+Enter
    and your `SELECT ... FROM data WHERE ...` runs against the loaded rows.

    [:octicons-arrow-right-24: SQL panel](usage/sql.md)

- :material-chart-line:{ .lg .middle } **Plot without leaving Octa**

    ---

    Histogram, bar, line, scatter, and box plots open in their own
    tab via **Analyse → Chart**. Style the title, axes, legend, and
    per-series colours, then export to PNG, SVG, or PDF for a report
    or slide deck.

    [:octicons-arrow-right-24: Charts](usage/chart.md)

- :material-console:{ .lg .middle } **Use it from the shell**

    ---

    `octa --schema data.parquet` prints columns + types. `octa --head`
    / `--tail` / `--sample` slice rows out (as TSV / JSON / CSV).
    `octa --diff a.csv b.csv` shows the rows that changed between two
    files, and `octa --convert in.csv out.parquet` round-trips through
    the same format readers the GUI uses.

    [:octicons-arrow-right-24: Command-line reference](cli/index.md)

- :material-robot-outline:{ .lg .middle } **Plug Claude into your data**

    ---

    `octa --mcp` is a Model Context Protocol server on stdio. A
    twenty-tool set, read_table, schema, run_sql, convert, profile,
    diff_tables, write_table, edit_table, and more, lets Claude Desktop,
    Claude Code, or any MCP client answer questions about (and edit) your
    local files. Runs in a [container](cli/docker.md) too.

    [:octicons-arrow-right-24: MCP server guide](mcp/index.md)

- :material-vector-difference:{ .lg .middle } **Compare two files**

    ---

    Compare a CSV to a Parquet by hashing matching columns. See exact
    line-by-line text diffs of two notebooks. Bucket rows into
    Left-only / Right-only / Shared and inspect each.

    [:octicons-arrow-right-24: Compare view](usage/view-modes/compare.md)

- :material-pencil:{ .lg .middle } **Edit and save back**

    ---

    Edit cells inline, insert and reorder columns, derive new ones with
    [formulas](usage/formulas.md) or
    [date/time calculation](usage/date-time-calculation.md), mark cells
    with colours, undo and redo. Even
    [Jupyter notebook](usage/view-modes/notebook.md) source cells are
    editable, and saving preserves their outputs. SQLite/DuckDB writes
    are diff-based, so only changed rows are touched.

    [:octicons-arrow-right-24: Editing](usage/editing.md)

- :material-translate:{ .lg .middle } **Use it in your language**

    ---

    The interface is available in 31 languages, English, German,
    Spanish, French, and more, switchable live under
    **Settings → Appearance**. Text-file encodings, date formats, and
    number separators are detected per file.

    [:octicons-arrow-right-24: Languages](reference/languages.md)

</div>

---

## Where to next

- New here? Start with **[Installation](getting-started/installation.md)** and
  **[First Steps](getting-started/first-steps.md)**.
- Looking for a specific feature? The **[View modes
  overview](usage/view-modes/overview.md)** lists every way Octa can
  display a file.
- Setting up MCP? **[MCP setup walkthrough](mcp/setup.md)** has
  step-by-step configs for Claude Desktop, Claude Code, and MCP
  Inspector.
- Power user? Jump to **[Keyboard
  shortcuts](reference/shortcuts.md)** or
  **[Tips & recipes](tips/workflows.md)**.

Octa is open source (MIT) and the source lives at
[github.com/thorstenfoltz/octa](https://github.com/thorstenfoltz/octa).
