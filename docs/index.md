---
hide:
  - navigation
  - toc
---

# Octa

![Octa main window in Table view](assets/screenshots/hero-table-view.png)

**Octa** is a native desktop application for viewing and editing tabular
data files. It opens Parquet, CSV, JSON, SQLite, DuckDB, Excel, and
around twenty more formats (log files among them) in a fast
spreadsheet-like view, with
sorting, filtering, full-text search, inline editing, SQL queries, and
file comparison.

It also doubles as a command-line tool and an MCP server, so models
like Claude can answer questions about your local files.

[Get Octa :material-download:](getting-started/installation.md){ .md-button .md-button--primary }
[First Steps :material-rocket-launch:](getting-started/first-steps.md){ .md-button }

---

## What you can do

The [user guide](guide/index.md) is sorted by task. Each topic lists its
jobs with where to find them in the app, on the command line, and as an
MCP tool.

<div class="grid cards" markdown>

- :material-folder-open-outline:{ .lg .middle } **Open files**

    ---

    Parquet, CSV, Excel, SQLite, a PDF's tables, a log file: Octa works out
    the format and opens it in a table. Files bigger than memory stream in
    the background.

    [:octicons-arrow-right-24: Open files](guide/open/index.md)

- :material-cloud-outline:{ .lg .middle } **Connect**

    ---

    Browse S3, Azure, GCS, SFTP and WebDAV, twelve live database engines and
    REST APIs in a sidebar, and open what you find like a local file.

    [:octicons-arrow-right-24: Connect](guide/connect/index.md)

- :material-table-eye:{ .lg .middle } **View and find**

    ---

    Sort, filter, search, mark and bookmark rows, or switch to a raw text,
    JSON tree, map, timeline or record view.

    [:octicons-arrow-right-24: View and find](guide/view/index.md)

- :material-pencil:{ .lg .middle } **Edit and reshape**

    ---

    Edit cells inline, add columns from formulas, change types, pivot and
    transpose, with undo for every step.

    [:octicons-arrow-right-24: Edit and reshape](guide/edit/index.md)

- :material-broom:{ .lg .middle } **Clean and protect**

    ---

    Find duplicates, outliers, missing values and personal data, fix them,
    and anonymise a table before you share it.

    [:octicons-arrow-right-24: Clean and protect](guide/clean/index.md)

- :material-table-merge-cells:{ .lg .middle } **Combine and split**

    ---

    Union, join, fuzzy and spatial joins, the key finder and the
    relationship map, merging edited copies, and splitting into files.

    [:octicons-arrow-right-24: Combine and split](guide/combine/index.md)

- :material-chart-line:{ .lg .middle } **Analyse**

    ---

    Summaries, value counts, correlations, time buckets, charts, forecasts
    and a shareable HTML report.

    [:octicons-arrow-right-24: Analyse](guide/analyse/index.md)

- :material-vector-difference:{ .lg .middle } **Compare and history**

    ---

    Diff two files or a file and a server table, spot schema and data drift,
    and follow a cell back through Git.

    [:octicons-arrow-right-24: Compare and history](guide/compare/index.md)

- :material-content-save-outline:{ .lg .middle } **Save and export**

    ---

    Save back, convert to another format, export to PDF, a workbook or DDL,
    and keep earlier versions.

    [:octicons-arrow-right-24: Save and export](guide/save/index.md)

- :material-database-search:{ .lg .middle } **SQL and assistant**

    ---

    Query any open file with DuckDB SQL, or ask a language model about your
    data in plain words.

    [:octicons-arrow-right-24: SQL and assistant](guide/sql-assistant/index.md)

- :material-repeat:{ .lg .middle } **Automate**

    ---

    Record a clean-up once and replay it as a recipe, convert whole folders,
    and script it all from the shell.

    [:octicons-arrow-right-24: Automate](guide/automate/index.md)

- :material-console:{ .lg .middle } **Command line**

    ---

    Every job that makes sense headless is an `octa --flag`, with TSV, CSV
    or JSON output for pipelines.

    [:octicons-arrow-right-24: Command line](cli/index.md)

- :material-robot-outline:{ .lg .middle } **MCP server**

    ---

    `octa --mcp` lets Claude Desktop, Claude Code or any MCP client read,
    query and (if you allow it) edit your local files.

    [:octicons-arrow-right-24: MCP server](mcp/index.md)

</div>

---

## Where to next

- New here? Start with **[Installation](getting-started/installation.md)** and
  **[First Steps](getting-started/first-steps.md)**.
- Want a complete walk-through? **[Common tasks](tips/workflows.md)** goes from
  a messy CSV to a clean Parquet, compares two files, and more.
- Setting up MCP? **[MCP setup](mcp/setup.md)** has step-by-step configs for
  Claude Desktop, Claude Code, and MCP Inspector.
- Power user? Jump to **[Keyboard shortcuts](reference/shortcuts.md)** or the
  **[Settings reference](reference/settings.md)**.

Octa is open source (MIT) and the source lives at
[github.com/thorstenfoltz/octa](https://github.com/thorstenfoltz/octa).
