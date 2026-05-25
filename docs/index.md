---
hide:
  - navigation
  - toc
---

# Octa

<!-- SCREENSHOT: hero-table-view.png — Octa's main window with a sample Parquet
file open in Table view. Light theme. Show multiple column types (numeric, date,
text), maybe a search bar with some filter applied. Aim for a friendly "this is
what data exploration looks like" hero shot. -->
![Octa main window in Table view](assets/screenshots/hero-table-view.png){ .screenshot-placeholder }

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

    `octa --schema data.parquet` prints columns + types. `octa --head
    file.csv -n 50 -f json` slices rows out as JSON. `octa --convert
    in.csv out.parquet` round-trips through the same format readers
    the GUI uses.

    [:octicons-arrow-right-24: Command-line reference](cli/index.md)

- :material-robot-outline:{ .lg .middle } **Plug Claude into your data**

    ---

    `octa --mcp` is a Model Context Protocol server on stdio. Six
    tools, read_table, schema, list_tables, count_rows, run_sql,
    convert, let Claude Desktop, Claude Code, or any MCP client
    answer questions about your local files.

    [:octicons-arrow-right-24: MCP server guide](mcp/index.md)

- :material-vector-difference:{ .lg .middle } **Compare two files**

    ---

    Compare a CSV to a Parquet by hashing matching columns. See exact
    line-by-line text diffs of two notebooks. Bucket rows into
    Left-only / Right-only / Shared and inspect each.

    [:octicons-arrow-right-24: Compare view](usage/view-modes/compare.md)

- :material-pencil:{ .lg .middle } **Edit and save back**

    ---

    Edit cells inline, insert and reorder columns, mark cells with
    colours, undo and redo. SQLite/DuckDB writes are diff-based, so only
    changed rows are touched.

    [:octicons-arrow-right-24: Editing](usage/editing.md)

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
