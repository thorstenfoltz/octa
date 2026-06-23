# Settings Reference

Open the Settings dialog via **Help → Settings** (default
shortcut **F3**). Settings are grouped into collapsible sections.

Settings persist to a TOML file:

| Platform | Path                                                                               |
|----------|------------------------------------------------------------------------------------|
| Linux    | `$XDG_CONFIG_HOME/octa/settings.toml` (defaults to `~/.config/octa/settings.toml`) |
| macOS    | `~/Library/Application Support/Octa/settings.toml`                                 |
| Windows  | `%APPDATA%\Octa\settings.toml`                                                     |

The TOML file is created on first launch with defaults. You can edit
it by hand if you prefer; Octa picks up changes on next launch.
Unknown / removed fields are tolerated (new versions add defaults
for missing keys; old versions ignore unknown keys).

<!-- SCREENSHOT: settings-dialog.png: Settings dialog open showing the section headers (Appearance, Files, File-Specific, Table View, etc.) with one section expanded. -->
![Settings dialog](../assets/screenshots/settings-dialog.png)

The sections below are listed in the same order as the dialog.

## Appearance

| Setting              | Default      | Notes                                                                                                                                                                                                                                                        |
|----------------------|--------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Language**         | English      | UI language for menus and dialogs. 13 Latin-script languages; switches live, no restart. See [Languages](languages.md). TOML key: `language`.                                                                                                                |
| **Font size**        | 13 pt        | Base font size. Applied to body / button / monospace text.                                                                                                                                                                                                   |
| **Default theme**    | Light        | `Light`, `Dark` and more. Applied when you press **Apply**, and at startup.                                                                                                                                                                                  |
| **Body font**        | Proportional | `Proportional` or `Monospace`.                                                                                                                                                                                                                               |
| **Custom font path** | *(empty)*    | Optional path to a TTF/OTF font. Overrides Body font for proportional text.                                                                                                                                                                                  |
| **Icon variant**     | Rose         | Window icon colour. Several options.                                                                                                                                                                                                                         |
| **Custom title bar** | on           | Replaces the OS window frame with Octa's own slim title bar (min/max/close in the toolbar), with drag-to-move and edge/corner resize. Frees the vertical space a system title bar takes. Turn off for native window decorations. Takes effect after restart. |

## Files

| Setting              | Default   | Notes                                                                                                                                                                          |
|----------------------|-----------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Max recent files** | 10        | How many entries to show in **File → Recent Files**.                                                                                                                           |
| **Open as text**     | *(empty)* | Comma- or space-separated list of file extensions that should always open as plain text. Useful for unusual config or log extensions Octa doesn't ship a dedicated reader for. |

## File-Specific

| Setting                             | Default | Notes                                                                                                                                                                                                                                                                                                        |
|-------------------------------------|---------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Colour aligned columns**          | on      | In [Raw view](../usage/view-modes/raw-text.md) of CSV/TSV files, tint each column with a subtle background.                                                                                                                                                                                                  |
| **Warn on un-align reload**         | on      | Confirmation dialog when toggling **Align Columns** off (the buffer is re-loaded).                                                                                                                                                                                                                           |
| **Warn on date format change**      | on      | One-shot banner when date inference promotes a non-ISO column.                                                                                                                                                                                                                                               |
| **Trim whitespace on load**         | off     | Strip leading/trailing whitespace from string cells and column titles when a file is opened (interior spaces kept). Off by default, so loaded values match what is stored. TOML key: `trim_whitespace_on_load`.                                                                                              |
| **Warn on whitespace trim**         | on      | Banner listing which columns had whitespace trimmed on load. Independent of the trim setting. TOML key: `warn_on_whitespace_trim`.                                                                                                                                                                           |
| **Offer repair on malformed files** | off     | Prompt to repair a CSV/TSV that reads but looks malformed (bad encoding, BOM, control chars, delimiter mismatch, ragged rows). The file on disk is never changed. See [Supported formats](../getting-started/supported-formats.md#repairing-malformed-csv-tsv-files). TOML key: `offer_repair_on_malformed`. |
| **Read-only mode notice**           | on      | Show the read-only intro modal on **F8** the first time per session.                                                                                                                                                                                                                                         |
| **Notebook output layout**          | Beneath | Where notebook output cells render: `Below cell` or `Side-by-side`.                                                                                                                                                                                                                                          |

## Table View

| Setting                     | Default | Notes                                                                                                                                                                                                                                                            |
|-----------------------------|---------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Show row numbers**        | on      | Hide the grey row-number gutter on the left.                                                                                                                                                                                                                     |
| **Show sequential numbers** | on      | While a filter is active, add a second gutter column numbering the *visible* rows from 1 (the first column keeps the original data row numbers). Only appears when filtered, off it would just duplicate the originals. TOML key: `show_sequential_row_numbers`. |
| **Alternating row colours** | on      | Subtle zebra striping.                                                                                                                                                                                                                                           |
| **Negative numbers in red** | on      | Colour negative numeric cells red.                                                                                                                                                                                                                               |
| **Thousand separators**     | on      | Render numeric cells with thousand separators (e.g. `1,234,567.89`). Display only, saved data is unchanged. TOML key: `thousands_separators_in_cells`.                                                                                                           |
| **Number style**            | English | Grouping + decimal marks for numeric cells: English (`1,234.56`) or European (`1.234,56`). The decimal mark follows this even with separators off. TOML key: `number_separator_style`.                                                                           |
| **Highlight edited cells**  | off     | Background colour for cells with unsaved edits.                                                                                                                                                                                                                  |
| **Cell line breaks**        | off     | Render `\n` inside cells as actual line breaks. Rows have variable height when on.                                                                                                                                                                               |
| **Binary display mode**     | Binary  | How `Binary` columns render: `Binary` (010101…), `Hex` (`0xab`), or `Text` (UTF-8 if printable, fallback to hex).                                                                                                                                                |
| **Default mark colour**     | Green   | Colour used by the `Mark` shortcut (Ctrl+M).                                                                                                                                                                                                                     |

## Summary

The **Analyse -> Summary** tab shows one row of statistics per column.
Each statistic below has a checkbox; turn any off to drop that column.
**Column** and **Type** are always shown. TOML key: `summary_stats`.

| Statistic          | Notes                                            |
|--------------------|--------------------------------------------------|
| **Min / Max**      | Smallest and largest value.                      |
| **Mean / Median**  | Average and middle value (numeric columns).      |
| **Std dev**        | Standard deviation (numeric columns).            |
| **Q25 / Q75**      | Lower and upper quartiles (numeric columns).     |
| **Not null**       | Count of present (non-null) values.              |
| **Nulls / Null %** | Count and share of missing values.               |
| **Unique**         | Exact count of distinct values (nulls excluded). |
| **Distinct ratio** | Unique values divided by total rows.             |
| **Total rows**     | Row count of the whole table.                    |

## Search & Editor

| Setting                   | Default | Notes                                                                                                                                                                                                                                                                        |
|---------------------------|---------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Default search mode**   | Plain   | Initial mode for the toolbar search: Plain / Wildcard / Regex.                                                                                                                                                                                                               |
| **Search result display** | Filter  | How search results show in the table: **Filter** hides non-matching rows; **Highlight** keeps every row and highlights matches in place (with a count and next/previous navigation). The search-bar toggle overrides this per session. Text and tree views always highlight. |
| **Search history size**   | 5       | How many recent search queries to remember across sessions (the **Recent** dropdown beside the search box). `0` disables the history. Stored in `search_history.json`. TOML key: `search_history_limit`.                                                                     |
| **Tab size**              | 4       | Number of spaces inserted when pressing Tab inside text editors (the Raw text editor and the Markdown Edit/Split editor; Tab indents in place rather than moving focus).                                                                                                     |

## SQL

| Setting                       | Default        | Notes                                                                                                                               |
|-------------------------------|----------------|-------------------------------------------------------------------------------------------------------------------------------------|
| **Open SQL panel by default** | off            | Auto-open the [SQL panel](../usage/sql.md) when opening a tabular file.                                                             |
| **Panel position**            | Bottom         | Where the SQL panel docks: `Bottom` / `Top` / `Left` / `Right`.                                                                     |
| **Default row limit**         | 100            | Placeholder query is `SELECT * FROM data LIMIT N`.                                                                                  |
| **Autocomplete**              | on             | Show keyword + column-name suggestion chips under the editor.                                                                       |
| **Editor font**               | JetBrains Mono | `JetBrainsMono` (bundled), `MatchUiFont`, or `SystemMonospace`.                                                                     |
| **Highlight SQL changes**     | on             | After an `INSERT`/`UPDATE`/`DELETE`, briefly mark the changed cells and new rows green. TOML key: `sql_row_diff_highlight_enabled`. |
| **Highlight duration**        | 4 s            | How long the mutation highlight stays before clearing. TOML key: `sql_row_diff_highlight_secs`.                                     |

## MCP

For the `octa --mcp` server. Both settings are read **once at server
startup**, so changes require restarting the MCP server (`octa --mcp`
process).

| Setting               | Default         | Notes                                                                                                                  |
|-----------------------|-----------------|------------------------------------------------------------------------------------------------------------------------|
| **Default row limit** | 1000            | Maximum rows returned by `read_table` / `run_sql` when the caller omits `limit`.                                       |
| **Unlimited**         | off             | When checked, the server returns every row by default (greys out the row-limit input).                                 |
| **Cell byte cap**     | 65,536 (64 KiB) | Per-cell on-wire size cap. Cells larger than this are replaced with a `[truncated: ...]` marker. `0` disables the cap. |

See [Limits & truncation](../mcp/limits-and-truncation.md) for the
full semantics.

## Chat / Assistant

Settings for the in-GUI [Assistant](../usage/chatbot.md) panel. All live
in the main Settings dialog under the **Chat / Assistant** section.

| Setting                      | Default                  | Notes                                                                                                                                                                                                                                                                                                                                                                               |
|------------------------------|--------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Provider**                 | Anthropic                | LLM backend: `Anthropic`, `OpenAI`, `OpenAI-compatible`, `Gemini`, or `Ollama` (local). TOML key: `chat_provider`.                                                                                                                                                                                                                                                                  |
| **Model**                    | per-provider default     | Model id for the active provider. A dropdown of presets (from `models.toml`) plus a free-text field for any model. Stored per provider. TOML key: `chat_models`.                                                                                                                                                                                                                    |
| **Base URL**                 | *(empty)*                | Endpoint override for the OpenAI-compatible provider. TOML key: `chat_base_url`.                                                                                                                                                                                                                                                                                                    |
| **Ollama URL**               | `http://localhost:11434` | Base URL of the local Ollama server. TOML key: `chat_ollama_url`.                                                                                                                                                                                                                                                                                                                   |
| **Panel position**           | Right                    | Where the chat panel docks: `Right` / `Left` / `Bottom`. TOML key: `chat_panel_position`.                                                                                                                                                                                                                                                                                           |
| **Temperature**              | 0.0                      | Sampling temperature passed to the model. TOML key: `chat_temperature`.                                                                                                                                                                                                                                                                                                             |
| **Max tool iterations**      | 3                        | How many tool-call rounds the agent runs per turn before stopping. TOML key: `chat_max_tool_iterations`.                                                                                                                                                                                                                                                                            |
| **Max tokens**               | 16,384                   | Cap on the model's response length. **Unlimited** omits the field (Anthropic substitutes a high value). TOML keys: `chat_max_tokens`, `chat_max_tokens_unlimited`.                                                                                                                                                                                                                  |
| **Export directory**         | `~/Downloads`            | Where the assistant writes files (charts, exports, `write_text`). TOML key: `chat_export_dir`.                                                                                                                                                                                                                                                                                      |
| **Write protection**         | On                       | When on (the default), the assistant cannot modify existing files, its live-edit tool (`edit_open_tab`) is disabled, and schema-changing DuckDB / SQLite / GeoPackage saves are refused. Turn off to let the assistant and database saves change your files. Manual GUI edits and saves are never blocked. The MCP server reads this once at startup. TOML key: `write_protection`. |
| **Back up before modifying** | On                       | When on (the default), Octa copies a file to a timestamped `.bak-*` sidecar before the assistant (or a schema-changing database save) overwrites it. Routine manual saves are **not** backed up. TOML key: `backup_before_modify`.                                                                                                                                                  |
| **Tool-call audit log**      | off                      | Record every assistant tool call (name, arg/result byte counts, duration) to `chat_audit/<session>.jsonl` in the config dir. TOML key: `chat_audit_log_enabled`. See [Assistant → audit log](../usage/chatbot.md#tool-call-audit-log).                                                                                                                                              |
| **Warn when logs exceed**    | 10 MB (on)               | Show a one-time startup warning when the audit logs grow past this size. TOML keys: `chat_audit_log_warn_enabled`, `chat_audit_log_warn_bytes`.                                                                                                                                                                                                                                     |
| **API key**                  | *(none)*                 | Per-provider key. Resolved **env → OS keyring → plaintext `settings.toml`**. **Clear API key** needs a second click to confirm. TOML key: `chat_api_keys` (plaintext fallback only).                                                                                                                                                                                                |

See [Assistant](../usage/chatbot.md) for the full workflow, tool list,
and the filesystem sandbox.

## Map

| Setting                  | Default                                          | Notes                                                                                                                                              |
|--------------------------|--------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------|
| **Default mode**         | Tiles                                            | Initial Map mode for new GeoJSON tabs: `Tiles` (slippy map) or `Geometry only` (no tile fetch).                                                    |
| **Fallback to geometry** | on                                               | If tile fetch fails, switch to geometry-only rendering automatically. Currently advisory; see notes on the [Map view](../usage/view-modes/map.md). |
| **Tile URL template**    | `https://tile.openstreetmap.org/{z}/{x}/{y}.png` | XYZ-style template. `{z}`, `{x}`, `{y}` are substituted with zoom and tile coordinates.                                                            |

## Directory Tree

| Setting                      | Default | Notes                                                                                                                                                                                 |
|------------------------------|---------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Sidebar position**         | Left    | Side the directory tree sidebar docks on (`Left` or `Right`).                                                                                                                         |
| **Show only openable files** | On      | List only sub-folders and files Octa can open (by extension). Files without an extension are hidden while on. Turn off to list every file. TOML key: `directory_tree_filter_enabled`. |

## Shortcuts

Every action is rebindable. Click **Record** next to an action,
press the new key combination (with Ctrl / Shift / Alt as needed),
and Octa saves the binding. **Escape** cancels recording. **Clear**
leaves an action unbound.

The dialog flags conflicting bindings (two actions on the same
combo) so you can resolve them before saving.

The full list of actions lives on the
[Keyboard shortcuts](shortcuts.md) page.

## Performance

| Setting                        | Default   | Notes                                                                                                                                                                                                                                                                                                                                                                                              |
|--------------------------------|-----------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Initial-load row cap**       | 5,000,000 | Max rows loaded into memory on first open for streaming readers (Parquet, CSV, TSV). Additional rows stream in the background. Numeric input accepts comma separators (`5,000,000`).                                                                                                                                                                                                               |
| **Syntax-highlight size cap**  | 1 MB      | Files larger than this fall back to plain monospace in the [Raw view](../usage/view-modes/raw-text.md) (syntect tokenisation gets laggy on huge files). Unit picker: Bytes / KB / MB. `0` disables highlighting entirely.                                                                                                                                                                          |
| **Raw view size cap (MB)**     | 500       | Largest file (in MB) whose full text is read into the [Raw view](../usage/view-modes/raw-text.md) editor. Also gates the parse-error raw fallback and the Compare view's raw side. Bigger files still open in the table view, just without raw text. Tick **Unlimited** to remove the ceiling (reads any file fully into memory). TOML keys: `raw_view_max_bytes`, `raw_view_max_bytes_unlimited`. |
| **Multi-search file cap (MB)** | 50        | Per-file size cap for the directory scope of the [Multi-search panel](../usage/search-and-filter.md#multi-search). Files larger than this are skipped silently during the scan. `0` disables the cap. TOML key: `grep_max_file_size_mb`.                                                                                                                                                           |
| **Chart max points**           | 100,000   | Maximum rows the [Chart tab](../usage/chart.md) will plot before evenly-spaced downsampling kicks in (Histogram, Line, Scatter). Bar always aggregates the full input; Box computes the 5-number summary over the full input. `0` disables sampling. TOML key: `chart_max_points`.                                                                                                                 |
| **Chart max categories**       | 250       | Maximum distinct X categories a [Bar chart](../usage/chart.md#categorical-x-axes) will accept before refusing to draw. Filter or aggregate the table before charting if you exceed this. TOML key: `chart_max_categories`.                                                                                                                                                                         |
| **Tables visible in picker**   | 10        | How many table rows the multi-table picker dialog (SQLite, DuckDB, …) fits vertically at its default size. The dialog stays user-resizable, so drag the corner to grow it when a database has more tables. Minimum 1. TOML key: `table_picker_visible_rows`.                                                                                                                                       |
| **Excel sheets to auto-open**  | 5         | How many sheets of a multi-sheet [Excel workbook](../getting-started/supported-formats.md#excel-multi-sheet-workbooks) open automatically (each in its own tab). Workbooks with more sheets show a picker so you choose which to open. Minimum 1. TOML key: `excel_max_auto_sheets`.                                                                                                               |

## Window

| Setting                 | Default       | Notes                                                                                               |
|-------------------------|---------------|-----------------------------------------------------------------------------------------------------|
| **Initial window size** | (auto-detect) | Pixel size of the window when it is **not** maximised, also used as the restore-from-maximise size. |
| **Start maximized**     | on            | Launch with the window maximised.                                                                   |

!!! note "Why every window size can look the same"
    A maximised window always fills the whole screen, so the **Initial window size** has no visible
    effect while the window is maximised (on a 4K screen it stays 4K whichever size you pick). The
    setting only takes effect on the restored (non-maximised) window: turn **Start maximized** off, or
    click the un-maximise button, to see it applied.

## Reset to defaults

The Settings dialog footer has a **Reset to defaults** button (red,
in the right corner). It replaces every value with its default in
the draft; nothing is written to disk until you click **Apply**,
so **Cancel** still reverts.

A confirmation dialog protects against misfires.

## See also

- [Keyboard shortcuts](shortcuts.md) is the full table of remappable
  actions.
- [CSV Quote / Escape modes](csv-quote-escape.md) is the visual
  guide to the Raw CSV/TSV view's quote/escape combos.
- [Date inference](date-inference.md) explains what the inference
  pass detects and when the ambiguity dialog appears.
- [Assistant](../usage/chatbot.md) is the full guide to the in-GUI chat
  panel whose settings are listed above.
