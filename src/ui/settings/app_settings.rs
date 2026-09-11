//! [`AppSettings`]: the serialised settings record, its defaults, its load
//! and save paths, and the owner-only permission helpers.
//!
//! Split out of `ui/settings/mod.rs`, which had grown to 2,150 lines holding
//! three unrelated things: the presentation enums, `AppSettings` itself, and
//! generic dialog chrome used by around seventy dialogs that have nothing to
//! do with settings. Code moved unchanged.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::super::shortcuts::Shortcuts;
use super::super::theme::{BodyFont, ThemeMode};
use super::*;
use crate::data::{BinaryDisplayMode, MapMode, MarkColor, SearchMode};

/// Persistent application settings.
///
/// `#[serde(default)]` on the struct fills every missing field from
/// [`AppSettings::default`] when loading a TOML written by an older or newer
/// release. Combined with the parse-failure backup in [`AppSettings::load`],
/// this means upgrading Octa never silently wipes the user's settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Base font size in points (applied to Body, Button, Monospace).
    pub font_size: f32,
    /// Default theme when the application starts.
    pub default_theme: ThemeMode,
    /// Icon color variant.
    pub icon_variant: IconVariant,
    /// Default search mode for the filter bar.
    #[serde(default)]
    pub default_search_mode: SearchMode,
    /// Whether to show row numbers in the table view.
    #[serde(default = "default_true")]
    pub show_row_numbers: bool,
    /// When a filter is active, show a second row-number column counting the
    /// visible rows from 1 (alongside the original row numbers). Only appears
    /// while filtered; redundant otherwise.
    #[serde(default = "default_true")]
    pub show_sequential_row_numbers: bool,
    /// How "Filter to marked" treats marked *cells* (marked rows/columns always
    /// keep their row/column). Default `RowsOnly`.
    #[serde(default)]
    pub mark_filter_cell_mode: crate::data::mark_filter::MarkFilterCellMode,
    /// Whether to use alternating row background colors.
    #[serde(default = "default_true")]
    pub alternating_row_colors: bool,
    /// Whether negative numbers are displayed in red.
    pub negative_numbers_red: bool,
    /// Raise log verbosity to debug for the `octa` crate (live, no restart).
    #[serde(default)]
    pub debug_mode: bool,
    /// Whether `Int` / `Float` cells render with thousand separators
    /// (e.g. `1,234,567.89`) in the table view. Display-only - never alters
    /// saved / exported data. Default `true`.
    #[serde(default = "default_true")]
    pub thousands_separators_in_cells: bool,
    /// Grouping / decimal-mark convention for numeric cells: English
    /// (`1,234.56`) or European (`1.234,56`). The decimal mark follows this
    /// even when `thousands_separators_in_cells` is off. Default English.
    #[serde(default)]
    pub number_separator_style: crate::data::num_format::SeparatorStyle,
    /// What the relationship map's Export button writes. PDF by default; the
    /// picker beside the button changes it and the change sticks, so a user
    /// who always wants SVG (or the interactive HTML) sets it once.
    #[serde(default)]
    pub rel_map_export_format: crate::data::rel_map_export::RelMapExportFormat,
    /// Default search behaviour: `Filter` hides non-matching rows (table only),
    /// `Highlight` keeps every row and highlights matches in place. The
    /// search-bar toggle overrides this per session. Text/tree views always
    /// highlight regardless.
    #[serde(default)]
    pub search_result_mode: crate::data::SearchResultMode,
    /// How many recent search queries to remember across sessions (the search
    /// box history dropdown). 0 disables the history. Default 20.
    #[serde(default = "default_search_history_limit")]
    pub search_history_limit: usize,
    /// Whether edited cells are highlighted with a background color.
    #[serde(default)]
    pub highlight_edits: bool,
    /// Whether to color columns differently in aligned raw CSV/TSV view.
    #[serde(default = "default_true")]
    pub color_aligned_columns: bool,
    /// Layout for Jupyter notebook output cells.
    #[serde(default)]
    pub notebook_output_layout: NotebookOutputLayout,
    /// Maximum number of recently opened files shown in the File menu.
    pub max_recent_files: usize,
    /// Periodically write modified file-backed tabs to disk. Off by default.
    #[serde(default)]
    pub auto_save_enabled: bool,
    /// Minutes between auto-saves when `auto_save_enabled`. Clamped to >= 1 on
    /// apply; default 5.
    #[serde(default = "default_auto_save_interval")]
    pub auto_save_interval_minutes: u32,
    /// Whether to allow line breaks in table cells (wraps long text).
    #[serde(default)]
    pub cell_line_breaks: bool,
    /// Style cells that hold a web address as a hyperlink and open it on
    /// Ctrl+click. On by default.
    #[serde(default = "default_true")]
    pub clickable_links: bool,
    /// How to display binary data columns (Binary, Hex, or Text).
    #[serde(default)]
    pub binary_display_mode: BinaryDisplayMode,
    /// Number of spaces inserted when pressing Tab in the text editor.
    #[serde(default = "default_tab_size")]
    pub tab_size: usize,
    /// Body / heading font choice (egui built-in proportional vs monospace).
    #[serde(default)]
    pub body_font: BodyFont,
    /// Optional path to a user-provided .ttf/.otf font. Overrides `body_font`
    /// for proportional text when set and readable.
    #[serde(default)]
    pub custom_font_path: String,
    /// Default color used by the `Mark` shortcut when the user has not picked
    /// a specific color via the toolbar / context menu.
    #[serde(default = "default_mark_color")]
    pub default_mark_color: MarkColor,
    /// Whether the SQL panel should be open by default when a tabular file is
    /// loaded.
    #[serde(default)]
    pub sql_panel_default_open: bool,
    /// Where to dock the SQL panel (Bottom or Right of the table view).
    #[serde(default)]
    pub sql_panel_position: SqlPanelPosition,
    /// Default LIMIT used in the placeholder query for new tabs.
    #[serde(default = "default_sql_row_limit")]
    pub sql_default_row_limit: usize,
    /// Whether the SQL editor offers keyword + column-name autocomplete.
    #[serde(default = "default_true")]
    pub sql_autocomplete: bool,
    /// Which font face the SQL editor (and its line-number gutter) uses.
    /// Independent of the UI font so users can keep the rest of Octa on a
    /// proportional face while reading SQL in monospace.
    #[serde(default)]
    pub sql_editor_font: SqlEditorFont,
    /// Briefly highlight the cells a SQL mutation (INSERT/UPDATE/DELETE)
    /// changed, so the effect of a query is visible. Default on.
    #[serde(default = "default_true")]
    pub sql_row_diff_highlight_enabled: bool,
    /// How long (seconds) the post-mutation row-diff highlight stays before
    /// fading. Default 4.
    #[serde(default = "default_sql_row_diff_secs")]
    pub sql_row_diff_highlight_secs: u32,
    /// Where to dock the directory tree sidebar when a folder is open.
    #[serde(default)]
    pub directory_tree_position: DirectoryTreePosition,
    /// Whether to show a confirmation warning before toggling "Align Columns"
    /// off in the raw CSV/TSV view, which reloads the file and discards edits.
    #[serde(default = "default_true")]
    pub warn_raw_align_reload: bool,
    /// Whether to show a one-shot banner when date inference promotes a string
    /// column to typed `Date`/`DateTime` AND the canonical ISO display format
    /// differs from the source format on disk (e.g. stored as `02.05.2026` but
    /// displayed as `2026-05-02`). The banner explains the change and offers
    /// a Dismiss button. Disable here to silence it globally.
    #[serde(default = "default_true")]
    pub warn_on_date_format_change: bool,
    /// User-customizable keyboard shortcut bindings.
    #[serde(default)]
    pub shortcuts: Shortcuts,
    /// Initial window size. Only has a visible effect when
    /// [`AppSettings::start_maximized`] is off; otherwise it is the
    /// restore-from-maximize size.
    #[serde(default)]
    pub window_size: WindowSize,
    /// Whether to launch the window maximized. When off, the window
    /// comes up at [`AppSettings::window_size`] instead.
    #[serde(default = "default_true")]
    pub start_maximized: bool,
    /// Ask GitHub for the latest release once per launch. Default `true`.
    /// The check is a single background request and never installs anything
    /// by itself; a newer version is announced in the status bar, and the
    /// install is always the user's click in the update dialog.
    #[serde(default = "default_true")]
    pub check_updates_on_start: bool,
    /// Show the notes of the version the user is *running* once, in a window
    /// they can dismiss for good. Default `true`. The notes are baked into
    /// the binary, so this is independent of
    /// [`AppSettings::check_updates_on_start`] and of any network access:
    /// neither setting reads the other.
    #[serde(default = "default_true")]
    pub show_release_notes: bool,
    /// Version whose notes were last shown. Keeps the window to once per
    /// release rather than once per launch.
    #[serde(default)]
    pub last_release_notes_version: String,
    /// Whether to pop a confirmation modal each time read-only mode is
    /// toggled (via shortcut or menu). Setting to `false` silences the
    /// notice; the read-only state still flips, you just don't see the
    /// pop-up. Default `true`.
    #[serde(default = "default_true")]
    pub show_readonly_notice: bool,
    /// When `true`, Octa requests an undecorated viewport at startup and
    /// renders its own slim title bar (logo + title + min/max/close buttons),
    /// with drag-to-move and edge/corner resize handles. Default `true` - the
    /// in-toolbar controls free the vertical space a system title bar would
    /// take and look consistent across platforms; opt out for native window
    /// decorations.
    #[serde(default = "default_true")]
    pub use_custom_title_bar: bool,
    /// Hard cap (in bytes) for files where the raw editor still applies
    /// syntect syntax highlighting. Past this threshold the editor falls
    /// back to plain monospace because per-frame tokenisation gets laggy.
    /// Default 1 MB. Set to 0 to disable highlighting entirely; set very
    /// high to opt out of the guard.
    #[serde(default = "default_syntax_highlight_max_bytes")]
    pub syntax_highlight_max_bytes: usize,
    /// Files at least this big open in large-file mode, which keeps the rows
    /// on disk and pages them in instead of loading them into memory.
    /// Default 10 GB.
    #[serde(default = "default_large_file_min_bytes")]
    pub large_file_min_bytes: usize,
    /// Explain what large-file mode can and cannot do, and ask, before opening
    /// such a file. Default on.
    #[serde(default = "default_true")]
    pub show_large_file_notice: bool,
    /// Maximum number of rows loaded into the active `DataTable` on first
    /// open for streaming formats (Parquet, CSV, TSV). Additional rows
    /// load in the background as the user scrolls toward the bottom.
    /// Default 5,000,000. Setting this very high improves first-paint
    /// completeness but uses more memory; setting it lower makes the
    /// initial open faster but means the background loader has to do
    /// more work as you scroll. Ignored when
    /// [`initial_load_rows_unlimited`](Self::initial_load_rows_unlimited)
    /// is `true`.
    #[serde(default = "default_initial_load_rows")]
    pub initial_load_rows: usize,
    /// When `true`, disables the initial-load cap entirely - every row in
    /// the file is loaded up front. Trumps [`initial_load_rows`](Self::initial_load_rows).
    /// Default `false`. Power users on machines with plenty of RAM can flip
    /// this on so a single huge parquet/CSV opens in one shot.
    #[serde(default)]
    pub initial_load_rows_unlimited: bool,
    /// Rows fetched per request when reading a table from a **live database**
    /// connection. Default 100,000.
    ///
    /// Deliberately its own knob rather than
    /// [`initial_load_rows`](Self::initial_load_rows): that cap sizes a local
    /// streaming reader, where five million rows is a few seconds of disk. The
    /// same number sent to a warehouse is megabytes of JSON over HTTP, and
    /// Databricks refuses a result over 25 MiB outright. The sidebar loads one
    /// page, then the background loader fetches the next as you scroll, the
    /// same way a large Parquet does. Lower it if opening a table feels slow.
    #[serde(default = "default_db_page_rows")]
    pub db_page_rows: usize,
    /// Maximum file size (in bytes) for which Octa reads the full file text
    /// into the Raw view editor. Also gates the parse-error raw fallback and
    /// the Compare view's raw right side. Past this ceiling raw text is
    /// skipped to protect memory. Default 500 MB. Overridden by
    /// [`raw_view_max_bytes_unlimited`](Self::raw_view_max_bytes_unlimited).
    #[serde(default = "default_raw_view_max_bytes")]
    pub raw_view_max_bytes: usize,
    /// When `true`, removes the raw-view size ceiling entirely - any file is
    /// read into the raw editor regardless of size. Trumps
    /// [`raw_view_max_bytes`](Self::raw_view_max_bytes). Default `false`.
    #[serde(default)]
    pub raw_view_max_bytes_unlimited: bool,
    /// Decompressed-size cap (in bytes) for transparently opened `.gz` /
    /// `.zst` files. A tiny compressed file can inflate to terabytes
    /// (a "decompression bomb"); past this cap the open is refused with a
    /// clear error. Default 4 GB. Overridden by
    /// [`max_decompressed_unlimited`](Self::max_decompressed_unlimited).
    #[serde(default = "default_max_decompressed_bytes")]
    pub max_decompressed_bytes: u64,
    /// When `true`, removes the decompression cap entirely. Trumps
    /// [`max_decompressed_bytes`](Self::max_decompressed_bytes). Default `false`.
    #[serde(default)]
    pub max_decompressed_unlimited: bool,
    /// How many files a cloud folder-union downloads before it stops. Every
    /// file is fetched to a temp and then read fully into memory by the Union
    /// dialog, so an unbounded folder (a data lake with tens of thousands of
    /// parts) would OOM. Past this the union runs on the first N files and the
    /// status bar reports the rest as skipped. Default 500. Overridden by
    /// [`folder_union_max_files_unlimited`](Self::folder_union_max_files_unlimited).
    #[serde(default = "default_folder_union_max_files")]
    pub folder_union_max_files: usize,
    /// When `true`, removes the folder-union file cap entirely. Trumps
    /// [`folder_union_max_files`](Self::folder_union_max_files). Default `false`.
    #[serde(default)]
    pub folder_union_max_files_unlimited: bool,
    /// Master gate for modifying existing data. Default **true** (protected).
    /// While true: the assistant cannot write to existing files, the chat
    /// live-edit tool refuses, and schema-changing DuckDB/SQLite/GeoPackage
    /// saves are refused. While false, all three are permitted. Distinct from
    /// the F8 session read-only mode (which blocks every in-memory edit).
    #[serde(default = "default_true")]
    pub write_protection: bool,
    /// Ask before opening a web address that redirected somewhere else.
    /// Default **true**. A link can bounce you to a different address than
    /// the one you typed, and the file you get is the one at the end of that
    /// chain, so the confirmation is the only place the change is visible.
    #[serde(default = "default_true")]
    pub confirm_url_redirects: bool,
    /// Ask for confirmation before writing a database tab's edits back to the
    /// server, listing what would change. Default **true**. Turning it off
    /// makes Save apply the diff straight away, which is what a user who
    /// writes back constantly will want and what a user editing production
    /// will not; the write is still one transaction either way.
    #[serde(default = "default_true")]
    pub confirm_db_write_back: bool,
    /// Copy an existing file to `<name>.<ext>.bak-YYYYMMDD-HHMMSS` before any
    /// in-place modification (every format). Default **true**.
    #[serde(default = "default_true")]
    pub backup_before_modify: bool,
    /// User-extensible list of file extensions (no leading dot, lowercase)
    /// that Octa should treat as plain text. Files with these extensions
    /// are routed through `TextReader` regardless of any other reader that
    /// would normally claim them. Useful for unusual config or log
    /// extensions Octa doesn't ship native support for.
    #[serde(default)]
    pub text_mode_extensions: Vec<String>,
    /// Absolute paths of pinned tabs. Restored on next launch through the
    /// regular `load_file` path. Files that no longer exist on disk are
    /// silently dropped from this list. Unsaved changes in a pinned tab are
    /// **not** auto-saved at close - the standard unsaved-changes dialog
    /// still runs.
    #[serde(default)]
    pub pinned_tabs: Vec<String>,
    /// Default row cap applied by the MCP server (`octa --mcp`) when a tool
    /// call omits its `limit` parameter. `None` means "return every row";
    /// `Some(n)` caps the response and sets `truncated: true` in the JSON.
    /// Defaults to `Some(1000)`. Read once at server startup - changing this
    /// while a server is running needs an `octa --mcp` restart.
    ///
    /// Persisted as a plain integer with **`0` meaning unlimited**, matching
    /// what a per-call `limit: 0` already means. A bare `Option` would write
    /// nothing at all for `None`, and the absent key then re-reads as the
    /// `Some(1000)` default - so ticking Unlimited did not survive a restart.
    #[serde(default = "default_mcp_row_limit", with = "zero_is_unlimited")]
    pub mcp_default_row_limit: Option<usize>,
    /// Per-cell byte cap applied by the MCP server. Cells whose textual
    /// form exceeds this are replaced with a `[truncated: ...]` marker and
    /// the tool response flags `cell_truncated: true`. `0` means no cap.
    /// Default 65,536 bytes (64 KiB).
    #[serde(default = "default_mcp_cell_bytes")]
    pub mcp_default_cell_bytes: usize,
    /// Default rendering mode for the Map view when opening a GeoJSON file.
    /// `Tiles` shows a slippy-map background; `GeometryOnly` skips the
    /// network fetch and paints just the geometry on a blank canvas.
    /// Toggleable per tab via the Map toolbar.
    #[serde(default)]
    pub map_default_mode: MapMode,
    /// When the map mode is `Tiles` and tile fetching fails (offline / DNS
    /// block / server error), automatically fall back to geometry-only
    /// rendering instead of leaving the user staring at a grey grid.
    /// Default `true`.
    #[serde(default = "default_true")]
    pub map_fallback_to_geometry: bool,
    /// Tile URL template, `{z}/{x}/{y}` for zoom + tile coordinates. The
    /// default points at the OSM tile server - please honour the
    /// [OSM Tile Usage Policy](https://operations.osmfoundation.org/policies/tiles/)
    /// in production deployments (point at a self-hosted or commercial
    /// provider, or get an API key).
    #[serde(default = "default_map_tile_url")]
    pub map_tile_url_template: String,
    /// Per-file size cap (megabytes) for the directory scope of the
    /// multi-search panel. Files over this size are skipped silently
    /// during the scan. Default 50 MB. Overridden by
    /// [`grep_max_file_size_unlimited`](Self::grep_max_file_size_unlimited).
    #[serde(default = "default_grep_max_file_size_mb")]
    pub grep_max_file_size_mb: u32,
    /// When `true`, removes the multi-search per-file size cap entirely.
    /// Trumps [`grep_max_file_size_mb`](Self::grep_max_file_size_mb).
    /// Default `false`.
    #[serde(default)]
    pub grep_max_file_size_unlimited: bool,
    /// Maximum number of input rows the Chart tab will plot before
    /// evenly-spaced downsampling kicks in. Histogram, Line, and Scatter
    /// all honour this; Bar always aggregates the full input and is
    /// bounded by `chart_max_categories` instead. Default 100,000.
    /// `0` disables sampling - at your own risk for very large tables.
    #[serde(default = "default_chart_max_points")]
    pub chart_max_points: usize,

    /// How long a status message stays on screen, in seconds. Default 10.
    ///
    /// One value for every message. Confirmations and failures used to differ
    /// (10s vs 60s) so an error could be read and copied; that is now handled
    /// by pausing the countdown while the pointer is over the message, which
    /// gives an error as long as it is being read without making every other
    /// message linger.
    ///
    /// Deliberately cannot be disabled. Read it through
    /// [`AppSettings::status_message_duration`], never directly: that applies
    /// the `MIN_STATUS_MESSAGE_SECS` floor, so a `0` hand-written into
    /// `settings.toml` cannot pin a message on screen forever.
    #[serde(default = "default_status_message_secs")]
    pub status_message_secs: u64,
    /// Maximum distinct X categories a Bar chart will accept. Above this
    /// the chart refuses to draw rather than rendering a wall of
    /// unreadable bars - the user should filter or group before charting.
    /// Default 200.
    #[serde(default = "default_chart_max_categories")]
    pub chart_max_categories: usize,
    /// How many sheets of a multi-sheet Excel workbook to open automatically
    /// (each in its own tab) without prompting. If a workbook has more sheets
    /// than this, Octa shows a sheet picker so the user chooses which to open
    /// (they may pick more than this number, or all of them). Default 5.
    #[serde(default = "default_excel_max_auto_sheets")]
    pub excel_max_auto_sheets: usize,
    /// Whether to strip leading/trailing whitespace from string cells when a
    /// file is loaded. Interior whitespace is untouched. Default `false` -
    /// loads leave cell values exactly as stored unless the user opts in.
    #[serde(default)]
    pub trim_whitespace_on_load: bool,

    /// Default writer knobs (Parquet compression / row groups, CSV quoting and
    /// line endings). Applied by Save As, Convert and batch convert unless the
    /// dialog overrides them for one operation. The defaults reproduce what
    /// Octa wrote before these existed.
    #[serde(default)]
    pub write_options: crate::formats::write_options::WriteOptions,
    /// Whether to normalise column headers to lower snake_case identifiers when
    /// a file is loaded (trim, lowercase, non-alphanumeric runs -> `_`,
    /// de-collide repeats with `_2`). Default `false` - headers load verbatim
    /// unless the user opts in. See `octa::data::trim::clean_headers`.
    #[serde(default)]
    pub clean_headers_on_load: bool,
    /// Whether to show a dismissible banner listing the columns that had
    /// whitespace trimmed on load. Default `true`. Independent of
    /// [`trim_whitespace_on_load`] - trimming can run silently if this is off.
    #[serde(default = "default_true")]
    pub warn_on_whitespace_trim: bool,
    /// How many table rows the multi-table picker dialog (SQLite / DuckDB /
    /// other multi-table sources) should fit vertically by default. The
    /// dialog stays user-resizable - this only controls the initial height
    /// so the picker doesn't dominate the screen when a database has a
    /// handful of tables. Default 10.
    #[serde(default = "default_table_picker_visible_rows")]
    pub table_picker_visible_rows: usize,
    /// UI language code (e.g. "en", "de", "fr"). Drives `octa::i18n`. Unknown
    /// or unsupported codes fall back to English at apply time. Default "en".
    #[serde(default = "default_language")]
    pub language: String,
    /// When a delimited text file (CSV / TSV) reads but looks malformed
    /// (invalid encoding, a leading BOM, control characters, a delimiter that
    /// disagrees with the extension, or wildly ragged rows), offer an
    /// interactive repair prompt instead of failing or loading garbage. Off by
    /// default so normal loads are never interrupted; opt in under
    /// Settings -> Performance. The repair itself (lossy decode, delimiter
    /// re-detection, BOM/control stripping) is only applied if the user
    /// confirms it in the prompt.
    #[serde(default)]
    pub offer_repair_on_malformed: bool,
    /// Named chat model profiles: as many provider+model+params combinations as
    /// the user wants, each with its own name. This is what the assistant panel
    /// actually picks from. Empty only on legacy settings;
    /// [`chat_profiles::ensure_profiles`] seeds one from the `chat_provider` /
    /// `chat_models` / `chat_temperature` fields below on first load.
    #[serde(default)]
    pub chat_profiles: Vec<chat_profiles::ChatModelProfile>,
    /// [`ChatModelProfile::id`] of the profile the assistant is using.
    #[serde(default)]
    pub chat_active_profile: String,
    /// Legacy: the single selected chat provider. Superseded by
    /// `chat_profiles`, kept as the migration source and as the fallback that
    /// `seed_profile_from_legacy` reads.
    #[serde(default)]
    pub chat_provider: ChatProviderKind,
    /// Last-used model name per provider, keyed by [`ChatProviderKind::id`].
    /// Lets the panel restore each provider's model in one click.
    #[serde(default)]
    pub chat_models: std::collections::BTreeMap<String, String>,
    /// Base URL for the OpenAI-compatible provider (OpenRouter / Groq /
    /// LM Studio / a custom gateway). Ignored by the other providers.
    /// Example: `https://openrouter.ai/api/v1`.
    #[serde(default)]
    pub chat_base_url: String,
    /// Root URL of the local Ollama server (no `/v1` suffix). Octa appends
    /// `/v1/chat/completions` for chat and `/api/tags` for the model list.
    #[serde(default = "default_chat_ollama_url")]
    pub chat_ollama_url: String,
    /// Where to dock the chat panel. Default Right.
    #[serde(default)]
    pub chat_panel_position: ChatPanelPosition,
    /// Sampling temperature passed to the provider. Default 0.7.
    #[serde(default = "default_chat_temperature")]
    pub chat_temperature: f32,
    /// Maximum agentic tool-use iterations per turn before the loop stops.
    /// Guards against runaway tool loops. Default 3.
    #[serde(default = "default_chat_max_tool_iterations")]
    pub chat_max_tool_iterations: usize,
    /// Maximum response tokens requested from the provider per turn.
    /// Default 16384. Ignored when `chat_max_tokens_unlimited` is set.
    #[serde(default = "default_chat_max_tokens")]
    pub chat_max_tokens: usize,
    /// When `true`, no response-token cap is sent (providers use their own
    /// default; Anthropic, which requires the field, substitutes a high value).
    #[serde(default)]
    pub chat_max_tokens_unlimited: bool,
    /// How many rows a tool result (e.g. a `run_sql` SELECT) puts into the
    /// assistant's context. The result is capped to this many rows so a big
    /// query can't flood the conversation; when it bites, the tool tells the
    /// model to offer writing the full result to a file/tab. Default 200.
    /// Ignored when `chat_result_row_limit_unlimited` is set.
    #[serde(default = "default_chat_result_row_limit")]
    pub chat_result_row_limit: usize,
    /// Keep a per-connection list of the queries actually run, with their
    /// timing and row count. On by default: the value of a history is that it
    /// is already there when you want it.
    #[serde(default = "default_true")]
    pub sql_history_enabled: bool,
    /// How many queries to keep per connection. 0 = unlimited, the same
    /// convention `chat_result_row_limit` uses.
    #[serde(default = "default_sql_history_limit")]
    pub sql_history_limit: usize,
    /// When `true`, tool results are not row-capped (the numeric limit above is
    /// ignored). Mirrors `chat_max_tokens_unlimited`.
    #[serde(default)]
    pub chat_result_row_limit_unlimited: bool,
    /// Directory the chat assistant writes exported files into (new CSVs,
    /// charts, ...) when the model gives a bare filename. The agent can only
    /// write here or to an absolute path the user explicitly requests.
    #[serde(default = "default_chat_export_dir")]
    pub chat_export_dir: String,
    /// Assistant tools the user switched off, by wire name. Stored as the
    /// *disabled* set on purpose: a tool added by a later release is then on
    /// by default rather than silently absent from an existing install.
    ///
    /// A disabled tool is gone from every angle - it is not sent, not named in
    /// the `enable_tools` menu, and refused if the model calls it anyway. See
    /// `app::chat::tool_groups`.
    #[serde(default)]
    pub chat_disabled_tools: Vec<String>,
    /// Render assistant replies as Markdown (headings, lists, tables, code
    /// blocks, links) rather than as the raw source. Default on: the model is
    /// told to answer in Markdown and the transcript export already is
    /// Markdown, so showing the source was the odd one out. Off gives the
    /// literal text back.
    #[serde(default = "default_true")]
    pub chat_render_markdown: bool,
    /// Record every assistant tool call to `<config_dir>/chat_audit/`. Off by
    /// default. See `src/app/chat/audit.rs`.
    #[serde(default)]
    pub chat_audit_log_enabled: bool,
    /// Warn at startup when the total size of the chat audit logs exceeds this
    /// many bytes. Default 10 MB.
    #[serde(default = "default_chat_audit_warn_bytes")]
    pub chat_audit_log_warn_bytes: u64,
    /// Whether the audit-log size warning is shown at all. Default on.
    #[serde(default = "default_true")]
    pub chat_audit_log_warn_enabled: bool,
    /// Plaintext per-provider API keys, keyed by [`ChatProviderKind::id`].
    /// Only populated when the OS keyring is unavailable; the keyring is
    /// preferred. Storing a key here means it sits in `settings.toml` in the
    /// clear, which the UI warns about explicitly.
    #[serde(default)]
    pub chat_api_keys: std::collections::BTreeMap<String, String>,
    /// Which statistics the Analyse -> Summary tab shows, in addition to the
    /// always-present column name and type. Defaults to the full set. See
    /// `src/data/summary.rs`.
    #[serde(default = "default_summary_stats")]
    pub summary_stats: Vec<crate::data::summary::SummaryStat>,
    /// When a folder is open in the directory-tree sidebar, show only
    /// sub-folders and files Octa can open (by extension). Default `true`.
    /// Turn off to list every file regardless of type.
    #[serde(default = "default_true")]
    pub directory_tree_filter_enabled: bool,
    /// Saved cloud connections (no secrets here; secrets live in the keyring /
    /// `cloud_secrets` fallback, keyed by connection id).
    #[serde(default)]
    pub cloud_connections: Vec<crate::cloud::CloudConnection>,
    /// Plaintext per-connection cloud secrets (JSON-encoded `CloudSecret`),
    /// keyed by connection id. Only populated when the OS keyring is
    /// unavailable; the keyring is preferred and the UI warns about plaintext.
    #[serde(default)]
    pub cloud_secrets: std::collections::BTreeMap<String, String>,
    /// Saved live database connections (no secrets here; secrets live in the
    /// keyring / `db_secrets` fallback, keyed by connection id).
    #[serde(default)]
    pub db_connections: Vec<crate::db::DbConnection>,
    /// Plaintext per-connection database passwords/tokens, keyed by
    /// connection id. Only populated when the OS keyring is unavailable;
    /// the keyring is preferred and the UI warns about plaintext.
    #[serde(default)]
    pub db_secrets: std::collections::BTreeMap<String, String>,
}

fn default_summary_stats() -> Vec<crate::data::summary::SummaryStat> {
    crate::data::summary::SummaryStat::default_enabled()
}

fn default_true() -> bool {
    true
}

fn default_chat_temperature() -> f32 {
    0.0
}

fn default_chat_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_chat_max_tool_iterations() -> usize {
    3
}

fn default_chat_max_tokens() -> usize {
    16_384
}

/// Twenty queries: enough to find the one from earlier this morning, short
/// enough that the list is still scannable without a search box.
fn default_sql_history_limit() -> usize {
    20
}

fn default_chat_result_row_limit() -> usize {
    200
}

/// Default chat export directory: the user's Downloads folder if it exists,
/// otherwise the home directory. Empty string if neither resolves.
fn default_chat_audit_warn_bytes() -> u64 {
    10 * 1024 * 1024
}

fn default_chat_export_dir() -> String {
    if let Some(home) = dirs_path_home() {
        let downloads = home.join("Downloads");
        if downloads.is_dir() {
            return downloads.to_string_lossy().into_owned();
        }
        return home.to_string_lossy().into_owned();
    }
    String::new()
}

fn default_language() -> String {
    "en".to_string()
}

fn default_auto_save_interval() -> u32 {
    5
}

fn default_search_history_limit() -> usize {
    5
}

fn default_sql_row_diff_secs() -> u32 {
    4
}

fn default_tab_size() -> usize {
    4
}

fn default_sql_row_limit() -> usize {
    100
}

fn default_syntax_highlight_max_bytes() -> usize {
    1024 * 1024
}

fn default_large_file_min_bytes() -> usize {
    10 * 1024 * 1024 * 1024
}

fn default_initial_load_rows() -> usize {
    5_000_000
}

fn default_db_page_rows() -> usize {
    100_000
}

fn default_raw_view_max_bytes() -> usize {
    500_000_000
}

fn default_max_decompressed_bytes() -> u64 {
    crate::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES
}

fn default_folder_union_max_files() -> usize {
    500
}

// Kept literal here (rather than referencing `crate::mcp::DEFAULT_*`)
// because `mcp` lives in the binary side of the crate split and the
// settings module is in the library. The values are mirrored by
// `src/mcp/mod.rs::DEFAULT_ROW_LIMIT` / `DEFAULT_CELL_BYTE_LIMIT`.
fn default_mcp_row_limit() -> Option<usize> {
    Some(1000)
}

/// Serde adapter for an optional cap that TOML has to hold as a plain number:
/// `0` on the wire is `None` ("no cap") in memory. Needed because serde omits
/// a `None` field entirely, and an omitted key falls back to the `default`
/// function rather than staying `None`.
mod zero_is_unlimited {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<usize>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(v.unwrap_or(0) as u64)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<usize>, D::Error> {
        let n = usize::deserialize(d)?;
        Ok((n > 0).then_some(n))
    }
}

fn default_mcp_cell_bytes() -> usize {
    64 * 1024
}

fn default_grep_max_file_size_mb() -> u32 {
    50
}

fn default_chart_max_points() -> usize {
    100_000
}

/// Shortest status-message lifetime the UI will honour.
///
/// The setting has no "off": a message that never expires is a message that
/// covers the status bar until restart. Three seconds is long enough to notice
/// a confirmation, and hovering pauses the countdown for anything longer.
pub const MIN_STATUS_MESSAGE_SECS: u64 = 3;

fn default_status_message_secs() -> u64 {
    10
}

fn default_chart_max_categories() -> usize {
    crate::data::chart::DEFAULT_MAX_BAR_CATEGORIES
}

fn default_table_picker_visible_rows() -> usize {
    10
}

fn default_excel_max_auto_sheets() -> usize {
    5
}

fn default_map_tile_url() -> String {
    // Stock OSM tile server. Walkers ships a `sources::OpenStreetMap`
    // helper that points at the same URL; we duplicate the literal here
    // so the user can edit it without juggling a `walkers::sources` type.
    "https://tile.openstreetmap.org/{z}/{x}/{y}.png".to_string()
}

fn default_mark_color() -> MarkColor {
    MarkColor::Green
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            font_size: 13.0,
            default_theme: ThemeMode::Light,
            icon_variant: IconVariant::Rose,
            default_search_mode: SearchMode::Plain,
            show_row_numbers: true,
            show_sequential_row_numbers: true,
            mark_filter_cell_mode: crate::data::mark_filter::MarkFilterCellMode::default(),
            alternating_row_colors: true,
            negative_numbers_red: true,
            debug_mode: false,
            thousands_separators_in_cells: true,
            number_separator_style: crate::data::num_format::SeparatorStyle::default(),
            rel_map_export_format: crate::data::rel_map_export::RelMapExportFormat::default(),
            search_result_mode: crate::data::SearchResultMode::default(),
            search_history_limit: default_search_history_limit(),
            highlight_edits: false,
            cell_line_breaks: false,
            clickable_links: true,
            binary_display_mode: BinaryDisplayMode::default(),
            color_aligned_columns: true,
            notebook_output_layout: NotebookOutputLayout::default(),
            max_recent_files: 10,
            auto_save_enabled: false,
            auto_save_interval_minutes: default_auto_save_interval(),
            tab_size: 4,
            body_font: BodyFont::Proportional,
            custom_font_path: String::new(),
            default_mark_color: default_mark_color(),
            sql_panel_default_open: false,
            sql_panel_position: SqlPanelPosition::default(),
            sql_default_row_limit: 100,
            sql_autocomplete: true,
            sql_editor_font: SqlEditorFont::default(),
            sql_row_diff_highlight_enabled: true,
            sql_row_diff_highlight_secs: default_sql_row_diff_secs(),
            directory_tree_position: DirectoryTreePosition::default(),
            warn_raw_align_reload: true,
            warn_on_date_format_change: true,
            shortcuts: Shortcuts::default(),
            window_size: WindowSize::default(),
            start_maximized: true,
            check_updates_on_start: true,
            show_release_notes: true,
            last_release_notes_version: String::new(),
            show_readonly_notice: true,
            use_custom_title_bar: true,
            syntax_highlight_max_bytes: default_syntax_highlight_max_bytes(),
            large_file_min_bytes: default_large_file_min_bytes(),
            show_large_file_notice: true,
            initial_load_rows: default_initial_load_rows(),
            initial_load_rows_unlimited: false,
            db_page_rows: default_db_page_rows(),
            raw_view_max_bytes: default_raw_view_max_bytes(),
            raw_view_max_bytes_unlimited: false,
            max_decompressed_bytes: default_max_decompressed_bytes(),
            max_decompressed_unlimited: false,
            folder_union_max_files: default_folder_union_max_files(),
            folder_union_max_files_unlimited: false,
            write_protection: true,
            confirm_url_redirects: true,
            confirm_db_write_back: true,
            backup_before_modify: true,
            text_mode_extensions: Vec::new(),
            pinned_tabs: Vec::new(),
            mcp_default_row_limit: default_mcp_row_limit(),
            mcp_default_cell_bytes: default_mcp_cell_bytes(),
            map_default_mode: MapMode::default(),
            map_fallback_to_geometry: true,
            map_tile_url_template: default_map_tile_url(),
            grep_max_file_size_mb: default_grep_max_file_size_mb(),
            grep_max_file_size_unlimited: false,
            chart_max_points: default_chart_max_points(),
            status_message_secs: default_status_message_secs(),
            chart_max_categories: default_chart_max_categories(),
            table_picker_visible_rows: default_table_picker_visible_rows(),
            excel_max_auto_sheets: default_excel_max_auto_sheets(),
            trim_whitespace_on_load: false,
            write_options: crate::formats::write_options::WriteOptions::default(),
            clean_headers_on_load: false,
            warn_on_whitespace_trim: true,
            offer_repair_on_malformed: false,
            language: default_language(),
            chat_profiles: Vec::new(),
            chat_active_profile: String::new(),
            chat_provider: ChatProviderKind::default(),
            chat_models: std::collections::BTreeMap::new(),
            chat_base_url: String::new(),
            chat_ollama_url: default_chat_ollama_url(),
            chat_panel_position: ChatPanelPosition::default(),
            chat_temperature: default_chat_temperature(),
            chat_max_tool_iterations: default_chat_max_tool_iterations(),
            chat_max_tokens: default_chat_max_tokens(),
            chat_max_tokens_unlimited: false,
            chat_disabled_tools: Vec::new(),
            chat_result_row_limit: default_chat_result_row_limit(),
            sql_history_enabled: true,
            sql_history_limit: default_sql_history_limit(),
            chat_result_row_limit_unlimited: false,
            chat_export_dir: default_chat_export_dir(),
            chat_render_markdown: true,
            chat_audit_log_enabled: false,
            chat_audit_log_warn_bytes: default_chat_audit_warn_bytes(),
            chat_audit_log_warn_enabled: true,
            chat_api_keys: std::collections::BTreeMap::new(),
            summary_stats: default_summary_stats(),
            directory_tree_filter_enabled: true,
            cloud_connections: Vec::new(),
            cloud_secrets: std::collections::BTreeMap::new(),
            db_connections: Vec::new(),
            db_secrets: std::collections::BTreeMap::new(),
        }
    }
}

impl AppSettings {
    /// Whether a file of `size_bytes` is small enough to load its full text
    /// into the Raw view (and the parse-error / Compare raw paths). Honours
    /// the "unlimited" override. Single source of truth for the size ceiling.
    /// The live-database page size actually used, clamped to something a
    /// connector will really return.
    ///
    /// One place, because two call sites (the sidebar open and the
    /// scroll-to-load-more worker) must agree: the open decides there are
    /// more rows by comparing what came back against the page size, so a page
    /// larger than the connectors' own row cap would come back short and be
    /// read as "that was the whole table".
    pub fn db_page_size(&self) -> usize {
        self.db_page_rows
            .max(1)
            .min(crate::formats::initial_load_rows())
    }

    pub fn raw_view_allows(&self, size_bytes: u64) -> bool {
        self.raw_view_max_bytes_unlimited || size_bytes <= self.raw_view_max_bytes as u64
    }

    /// How many files a folder-union may take, with "unlimited" folded in as
    /// `usize::MAX` so callers can just `truncate` to it.
    pub fn folder_union_cap(&self) -> usize {
        if self.folder_union_max_files_unlimited {
            usize::MAX
        } else {
            self.folder_union_max_files
        }
    }

    /// Per-file byte ceiling for the Multi-search directory scan, where `0`
    /// means "no ceiling". Both the Unlimited checkbox and a legacy `0` in
    /// `settings.toml` (the old way to switch the cap off, before the
    /// checkbox existed) resolve to it.
    pub fn grep_max_file_bytes(&self) -> u64 {
        if self.grep_max_file_size_unlimited {
            0
        } else {
            (self.grep_max_file_size_mb as u64).saturating_mul(1024 * 1024)
        }
    }

    /// Platform-specific config directory.
    pub fn config_dir() -> Option<PathBuf> {
        // `OCTA_CONFIG_DIR` wins on every platform and is used verbatim (no
        // `octa` subdirectory appended). It is the one lever a container has:
        // a distroless image typically has no `HOME`, `XDG_CONFIG_HOME` or
        // `APPDATA`, so without it every branch below returns `None` and the
        // CLI / MCP server silently has no settings at all.
        if let Some(dir) = env_path("OCTA_CONFIG_DIR") {
            return Some(dir);
        }
        #[cfg(target_os = "linux")]
        {
            env_path("XDG_CONFIG_HOME")
                .or_else(|| dirs_path_home().map(|h| h.join(".config")))
                .map(|d| d.join("octa"))
        }
        #[cfg(target_os = "windows")]
        {
            env_path("APPDATA").map(|d| d.join("Octa"))
        }
        #[cfg(target_os = "macos")]
        {
            dirs_path_home().map(|h| h.join("Library/Application Support/Octa"))
        }
    }

    fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("settings.toml"))
    }

    /// Load settings from disk, falling back to defaults.
    ///
    /// Robustness: missing/extra fields are tolerated via `#[serde(default)]`
    /// at the struct level. Hard parse failures (e.g. an enum variant the
    /// current binary no longer knows) cause the broken file to be copied
    /// alongside as `settings.toml.bak-<unix-timestamp>` before defaults are
    /// returned, so the user can recover their values manually.
    /// How long a status message stays on screen, floored so it can never be
    /// switched off.
    ///
    /// The single chokepoint for message expiry, in the spirit of
    /// `OctaApp::is_readonly()`: every caller funnels through here rather than
    /// reading `status_message_secs` directly, so the floor cannot be bypassed
    /// by a hand-edited config.
    pub fn status_message_duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.status_message_secs.max(MIN_STATUS_MESSAGE_SECS))
    }

    pub fn load() -> Self {
        let existed = Self::config_path().is_some_and(|p| p.exists());
        let mut settings = Self::load_raw();
        // Every load path (fresh install, unreadable config, parse failure,
        // normal load) must come out with at least one chat profile and a
        // valid active one, so the assistant panel always has a model to use.
        chat_profiles::ensure_profiles(&mut settings);
        // First run: write the defaults out so the file exists to hand-edit.
        // Octa otherwise only writes `settings.toml` on Settings-Apply, which
        // leaves a fresh install with no file at all - and therefore no way to
        // change a setting when the GUI itself is what is misbehaving.
        if !existed {
            settings.save();
        }
        settings
    }

    /// The raw load, before any post-load normalisation. See [`Self::load`].
    fn load_raw() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        let contents = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => return Self::default(),
        };
        match toml::from_str::<Self>(&contents) {
            Ok(s) => s,
            Err(err) => {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let backup = path.with_file_name(format!("settings.toml.bak-{ts}"));
                let _ = std::fs::copy(&path, &backup);
                eprintln!(
                    "octa: failed to parse {} ({err}); backed up to {} and using defaults.",
                    path.display(),
                    backup.display(),
                );
                Self::default()
            }
        }
    }

    /// Persist settings to disk. The file may carry plaintext API keys (the
    /// keyring fallback), so it is restricted to the owning user.
    /// Persist, ignoring failures. The GUI's path: it saves on every Apply and
    /// on exit, where a message box about a read-only config directory would be
    /// noise. Use [`Self::save_result`] where the caller can report.
    pub fn save(&self) {
        let _ = self.save_result();
    }

    /// Persist, reporting what went wrong. The CLI uses this: `--add-connection`
    /// claiming success while writing nothing would be worse than an error, and
    /// in a container "no writable config directory" is the likely outcome
    /// rather than an exotic one.
    pub fn save_result(&self) -> Result<PathBuf, String> {
        let path = Self::config_path().ok_or_else(|| {
            "no config directory: set OCTA_CONFIG_DIR to a writable path (or HOME / \
             XDG_CONFIG_HOME on Linux, APPDATA on Windows)"
                .to_string()
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("creating {}: {e}", parent.display()))?;
        }
        // Narrow the window: clamp an existing file before rewriting it.
        if path.exists() {
            restrict_file_to_owner(&path);
        }
        let contents =
            toml::to_string_pretty(self).map_err(|e| format!("serialising settings: {e}"))?;
        std::fs::write(&path, contents).map_err(|e| format!("writing {}: {e}", path.display()))?;
        restrict_file_to_owner(&path);
        Ok(path)
    }
}

/// Best-effort chmod 0600: only the owning user may read files that can hold
/// secrets (plaintext API keys in `settings.toml`, chat transcripts). No-op on
/// non-Unix platforms, where ACLs already scope `%APPDATA%` to the user.
pub fn restrict_file_to_owner(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// Best-effort chmod 0700 for directories holding sensitive files. See
/// [`restrict_file_to_owner`].
pub fn restrict_dir_to_owner(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// A path from an environment variable, treating "set but empty" as unset.
///
/// On Unix an exported-but-empty variable reads as `Ok("")`, not `Err`, and
/// `PathBuf::from("").join("octa")` is the *relative* path `octa` - so an empty
/// `HOME` or `XDG_CONFIG_HOME` used to drop `settings.toml`, plaintext secrets
/// and all, into whatever directory Octa happened to be started from.
fn env_path(var: &str) -> Option<PathBuf> {
    non_empty_path(std::env::var(var).ok())
}

/// The decision `env_path` makes, split out so it is testable without
/// mutating the process environment.
pub(super) fn non_empty_path(value: Option<String>) -> Option<PathBuf> {
    value.filter(|v| !v.trim().is_empty()).map(PathBuf::from)
}

/// Helper: get the user's home directory without pulling in the `dirs` crate.
fn dirs_path_home() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        env_path("HOME")
    }
    #[cfg(windows)]
    {
        env_path("USERPROFILE")
    }
}

/// Shared slot a "Test connection" worker writes its outcome into
/// (`Ok(())` or `Err(message)`), drained by the DB form per frame.
pub(crate) type DbTestSlot = std::sync::Arc<std::sync::Mutex<Option<Result<(), String>>>>;

/// Shared slot a chat "Test connection" worker writes its outcome into: the
/// model's reply on success, the provider's error message on failure.
pub type ChatTestSlot = std::sync::Arc<std::sync::Mutex<Option<Result<String, String>>>>;
