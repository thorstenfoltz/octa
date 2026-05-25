# Screenshot Index

This file is the shopping list for screenshots referenced
throughout the documentation. Every entry below is a `<!-- SCREENSHOT:
filename.png — description -->` comment somewhere in `docs/`.

**To fill a placeholder**: drop a PNG matching the filename into
`docs/assets/screenshots/`. The mkdocs build then resolves the
image automatically; no edits to the markdown source needed.

**Total placeholders:** 26 across 21 unique filenames

## All screenshots

| Filename   | Page                                     | Description                                                                                                                                                                                                                                                                                                                               |
|------------|------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `claude`   | `mcp/setup.md`                           | desktop-tools-list.png — Claude Desktop's MCP tools panel open with octa's six tools (read_table, schema, list_tables, count_rows, run_sql, convert) listed.                                                                                                                                                                              |
| `colour`    | `usage/colour-marking.md`                 | marking-example.png — Table with a few cells marked Yellow, an entire row marked Red, and a column marked Blue. Show how the precedence works visually.                                                                                                                                                                                   |
| `compare`  | `usage/view-modes/compare.md`            | view-text-diff.png — Compare view in Text Diff mode. Two panes side-by-side with line numbers, +/-/~ markers in the gutter, added lines in green, removed in red, modified in yellow.                                                                                                                                                     |
| `csv`      | `reference/csv-quote-escape.md`          | quote-escape-toolbar.png — Raw view of a CSV file with the Delimiter / Quote / Escape combos visible in the toolbar, all dropdowns showing their options.                                                                                                                                                                                 |
| `date`     | `reference/date-inference.md`            | ambiguity-dialog.png — Modal dialog asking the user to pick between European DD/MM/YYYY and US MM/DD/YYYY for an ambiguous column, with a "Leave as text" escape hatch. Show a few sample values from the column.                                                                                                                         |
| `epub`     | `usage/view-modes/epub-reader.md`        | reader-view.png — EPUB Reader view of a chapter. Show the toolbar at top (book title, Previous / Next buttons, chapter combo with the current chapter highlighted, position N/M), and a chapter body rendered as paragraphs of flowing text. If possible, include an embedded image (e.g. a cover) in the thumbnail strip below the text. |
| `first`    | `getting-started/first-steps.md`         | steps-file-menu.png — File menu open, showing Open / Open Directory / Recent Files / Save / Save As entries.                                                                                                                                                                                                                              |
| `hero`     | `index.md`                               | table-view.png — Octa's main window with a sample Parquet file open in Table view. Light theme. Show multiple column types (numeric, date, text), maybe a search bar with some filter applied. Aim for a friendly "this is what data exploration looks like" hero shot.                                                                   |
| `insert`   | `usage/formulas.md`                      | column-formula.png — Insert Column dialog with Name, Type, Formula fields filled in (e.g. Name=margin, Type=Float64, Formula==B1-C1).                                                                                                                                                                                                     |
| `json`     | `usage/view-modes/json-and-yaml-tree.md` | tree-view.png — JSON Tree view with several levels expanded, showing keys, nested objects, arrays, mixed value types.                                                                                                                                                                                                                     |
| `map`      | `usage/view-modes/map.md`                | view-tiles.png — Map view with OSM tiles loaded, several feature geometries painted on top (e.g. a polygon outlining a district, some points marking cities, a line between two points). Default steel-blue palette.                                                                                                                      |
| `markdown` | `usage/view-modes/markdown.md`           | view-split.png — Markdown view in Split mode: a TextEdit on the left with raw Markdown, a rendered preview on the right showing headings, bold text, a list, an inline code span.                                                                                                                                                         |
| `mcp`      | `mcp/setup.md`                           | inspector.png — MCP Inspector browser UI showing the tool list (read_table, schema, list_tables, count_rows, run_sql, convert) on the left, a tool selected with its input form filled in on the right, and a JSON response below.                                                                                                        |
| `notebook` | `usage/view-modes/notebook.md`           | view.png — A notebook view with code cells (Python), a Markdown heading + paragraph, output text below a cell, and a small image output.                                                                                                                                                                                                  |
| `raw`      | `usage/view-modes/raw-text.md`           | text-view.png — Raw view of a Python file with syntect highlighting on. Show line numbers, the gutter, syntax-highlighted keywords/strings.                                                                                                                                                                                               |
| `search`   | `usage/search-and-filter.md`             | toolbar.png — Toolbar with the search box focused, a few characters typed, mode dropdown visible (Plain/Wildcard/Regex), table below showing filtered results.                                                                                                                                                                            |
| `settings` | `reference/settings.md`                  | dialog.png — Settings dialog open showing the section headers (Appearance, Table View, Search & Editor, etc.) with one section expanded.                                                                                                                                                                                                  |
| `sql`      | `usage/sql.md`                           | view.png — SQL panel docked at the bottom of the window. Editor on top with a multi-line SELECT query, result table below showing a few rows. Line numbers in the editor gutter, autocomplete chip row visible under the editor.                                                                                                          |
| `table`    | `usage/table-view.md`                    | view-overview.png — Octa main window in Table view. Show a file with maybe 8-10 columns, a few rows highlighted (selection), a sort indicator on one column. Light theme.                                                                                                                                                                 |
| `tabs`     | `usage/tabs-and-sidebar.md`              | and-sidebar.png — Window with the folder sidebar docked on the left, expanded down a few levels, and several tabs open in the strip across the top.                                                                                                                                                                                       |
| `view`     | `usage/view-modes/overview.md`           | menu.png — View menu open in the toolbar, showing the radio buttons for Table / Raw Text / Markdown / Notebook / EPUB Reader / Map / JSON Tree / YAML Tree / Compare / Read-only mode.                                                                                                                                                    |

## Grouped by section

### (root)

- **`hero`** (index.md) — table-view.png — Octa's main window with a sample Parquet file open in Table view. Light theme. Show multiple column types (numeric, date, text), maybe a search bar with some filter applied. Aim for a friendly "this is what data exploration looks like" hero shot.

### getting-started

- **`first`** (getting-started/first-steps.md) — steps-file-menu.png — File menu open, showing Open / Open Directory / Recent Files / Save / Save As entries.

### mcp

- **`claude`** (mcp/setup.md) — desktop-tools-list.png — Claude Desktop's MCP tools panel open with octa's six tools (read_table, schema, list_tables, count_rows, run_sql, convert) listed.
- **`mcp`** (mcp/setup.md) — inspector.png — MCP Inspector browser UI showing the tool list (read_table, schema, list_tables, count_rows, run_sql, convert) on the left, a tool selected with its input form filled in on the right, and a JSON response below.

### reference

- **`csv`** (reference/csv-quote-escape.md) — quote-escape-toolbar.png — Raw view of a CSV file with the Delimiter / Quote / Escape combos visible in the toolbar, all dropdowns showing their options.
- **`date`** (reference/date-inference.md) — ambiguity-dialog.png — Modal dialog asking the user to pick between European DD/MM/YYYY and US MM/DD/YYYY for an ambiguous column, with a "Leave as text" escape hatch. Show a few sample values from the column.
- **`settings`** (reference/settings.md) — dialog.png — Settings dialog open showing the section headers (Appearance, Table View, Search & Editor, etc.) with one section expanded.

### usage

- **`colour`** (usage/colour-marking.md) — marking-example.png — Table with a few cells marked Yellow, an entire row marked Red, and a column marked Blue. Show how the precedence works visually.
- **`insert`** (usage/formulas.md) — column-formula.png — Insert Column dialog with Name, Type, Formula fields filled in (e.g. Name=margin, Type=Float64, Formula==B1-C1).
- **`search`** (usage/search-and-filter.md) — toolbar.png — Toolbar with the search box focused, a few characters typed, mode dropdown visible (Plain/Wildcard/Regex), table below showing filtered results.
- **`table`** (usage/table-view.md) — view-overview.png — Octa main window in Table view. Show a file with maybe 8-10 columns, a few rows highlighted (selection), a sort indicator on one column. Light theme.
- **`tabs`** (usage/tabs-and-sidebar.md) — and-sidebar.png — Window with the folder sidebar docked on the left, expanded down a few levels, and several tabs open in the strip across the top.
- **`compare`** (usage/view-modes/compare.md) — view-text-diff.png — Compare view in Text Diff mode. Two panes side-by-side with line numbers, +/-/~ markers in the gutter, added lines in green, removed in red, modified in yellow.
- **`epub`** (usage/view-modes/epub-reader.md) — reader-view.png — EPUB Reader view of a chapter. Show the toolbar at top (book title, Previous / Next buttons, chapter combo with the current chapter highlighted, position N/M), and a chapter body rendered as paragraphs of flowing text. If possible, include an embedded image (e.g. a cover) in the thumbnail strip below the text.
- **`json`** (usage/view-modes/json-and-yaml-tree.md) — tree-view.png — JSON Tree view with several levels expanded, showing keys, nested objects, arrays, mixed value types.
- **`map`** (usage/view-modes/map.md) — view-tiles.png — Map view with OSM tiles loaded, several feature geometries painted on top (e.g. a polygon outlining a district, some points marking cities, a line between two points). Default steel-blue palette.
- **`markdown`** (usage/view-modes/markdown.md) — view-split.png — Markdown view in Split mode: a TextEdit on the left with raw Markdown, a rendered preview on the right showing headings, bold text, a list, an inline code span.
- **`notebook`** (usage/view-modes/notebook.md) — view.png — A notebook view with code cells (Python), a Markdown heading + paragraph, output text below a cell, and a small image output.
- **`view`** (usage/view-modes/overview.md) — menu.png — View menu open in the toolbar, showing the radio buttons for Table / Raw Text / Markdown / Notebook / EPUB Reader / Map / JSON Tree / YAML Tree / Compare / Read-only mode.
- **`raw`** (usage/view-modes/raw-text.md) — text-view.png — Raw view of a Python file with syntect highlighting on. Show line numbers, the gutter, syntax-highlighted keywords/strings.
- **`sql`** (usage/sql.md) — view.png — SQL panel docked at the bottom of the window. Editor on top with a multi-line SELECT query, result table below showing a few rows. Line numbers in the editor gutter, autocomplete chip row visible under the editor.
