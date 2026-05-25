//! Core application state types: [`OctaApp`], [`TabState`], and the
//! update-install state machine.

use std::sync::{Arc, Mutex};

use eframe::egui;

use octa::data::{self, DataTable, ViewMode};
use octa::formats::FormatRegistry;
use octa::ui;
use ui::settings::{AppSettings, DialogSize, IconVariant, SettingsDialog};
use ui::table_view::TableViewState;
use ui::theme::ThemeMode;

/// Maximum number of recently-closed tabs Octa remembers for the
/// `ReopenLastClosedTab` shortcut. Matches the convention browsers use.
pub(crate) const MAX_CLOSED_TAB_HISTORY: usize = 10;

/// Snapshot of a tab that was just closed, used to power the
/// `ReopenLastClosedTab` (Ctrl+Shift+T) shortcut.
///
/// For tabs backed by a file on disk, the path is retained — reopening
/// rereads the file, which is cheaper than holding a full `TabState` clone
/// and keeps any concurrent edits visible. For scratch tabs (no source
/// path: parsed-in-new-tab, raw edits, empty welcome tab) only the textual
/// payload (`raw_content` + view mode + format label) is kept — enough to
/// recreate the visible state without trying to deep-clone egui textures,
/// commonmark caches, etc. Truly empty tabs are not snapshotted.
pub(crate) enum ClosedTabSnapshot {
    Path(std::path::PathBuf),
    Scratch {
        raw_content: String,
        view_mode: ViewMode,
        format_name: Option<String>,
    },
}

/// Sort order for the Column Inspector dialog. View-only — does not mutate
/// the underlying column order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ColumnInspectorSort {
    #[default]
    Default,
    Asc,
    Desc,
}

/// What to do with duplicate rows once `find_duplicate_rows` has
/// returned them. `Highlight` marks each row in orange so the user can
/// see them in place; `NewTab` opens a new tab containing only those
/// rows, leaving the original untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FindDuplicatesMode {
    #[default]
    Highlight,
    NewTab,
}

/// Open Schema Export dialog state. Carries the currently-shown
/// target so the user can switch between renderings (Postgres ↔
/// MySQL ↔ Pydantic ↔ …) without closing the dialog, plus the
/// window-size mode. Held on `OctaApp` rather than `TabState`
/// because the dialog operates on the active tab's column list
/// rather than per-tab persistent state.
pub(crate) struct SchemaExportState {
    pub(crate) target: octa::data::schema_export::SchemaTarget,
    pub(crate) size: ui::settings::DialogSize,
}

/// One-shot per-file prompt shown after loading a CSV/TSV whose size is
/// likely to make column coloring or column alignment laggy. The user can
/// either keep the slow features on (we honor their choice and don't ask
/// again for this tab) or disable them just for the current file. Choice is
/// transient — never written back to `AppSettings`.
pub(crate) struct RawPerfPrompt {
    pub(crate) tab_idx: usize,
    pub(crate) file_size: u64,
    pub(crate) file_name: String,
}

/// One promoted column whose stored canonical ISO display differs from the
/// detected source format. Collected during `run_date_inference_pass` and
/// surfaced together as a single dismissible banner above the table.
/// `original_values` carries the source strings for every row (None for
/// pre-existing nulls) so dismissing the banner can revert the column back
/// to its on-disk shape.
#[derive(Debug, Clone)]
pub(crate) struct DatePromotionInfo {
    pub(crate) col_idx: usize,
    pub(crate) column_name: String,
    pub(crate) source_label: &'static str,
    pub(crate) original_values: Vec<Option<String>>,
}

/// Aggregate set of date promotions to surface to the user as a single
/// non-modal banner. `None` means no banner is currently pending. Cleared
/// when the user clicks Dismiss or opens a new file.
#[derive(Debug, Clone, Default)]
pub(crate) struct DateWarning {
    pub(crate) tab_idx: usize,
    pub(crate) entries: Vec<DatePromotionInfo>,
}

/// One pending date-format ambiguity dialog request: a column whose values
/// are consistent with more than one date layout (e.g. DD/MM/YYYY and
/// MM/DD/YYYY). The user picks one, or chooses to leave the column as
/// strings.
pub(crate) struct DateAmbiguity {
    pub(crate) tab_idx: usize,
    pub(crate) col_idx: usize,
    pub(crate) col_name: String,
    pub(crate) samples: Vec<String>,
    pub(crate) date_candidates: Vec<octa::data::date_infer::DateLayout>,
    pub(crate) datetime_candidates: Vec<octa::data::date_infer::DateTimeLayout>,
}

/// Quoting convention recognized by the raw CSV/TSV alignment view. Drives
/// the inline tokenizer in `format_delimited_text` so a delimiter inside a
/// quoted field doesn't split the cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RawCsvQuote {
    /// RFC 4180 default — fields may be wrapped in `"`.
    #[default]
    Double,
    /// Fields may be wrapped in `'` (some dialects).
    Single,
    /// Either `"` or `'` opens a quoted span; whichever opens it must close it.
    Both,
    /// Quote characters carry no meaning — split purely on the delimiter.
    None,
}

/// How an embedded quote inside a quoted field is escaped. Determines whether
/// `""` collapses to `"`, whether `\"` collapses to `"`, or whether the first
/// matching quote always closes the span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RawCsvEscape {
    /// RFC 4180 default — `""` inside a `"..."` span is a literal quote.
    #[default]
    Doubled,
    /// C-style `\"` (and `\\`) escape inside the quoted span.
    Backslash,
    /// No escapes — the first matching quote closes the span.
    None,
}

#[derive(Clone)]
pub(crate) enum UpdateState {
    /// No check in progress
    Idle,
    /// Checking GitHub for latest version
    Checking,
    /// A newer version is available
    Available(String),
    /// Already on the latest version
    UpToDate,
    /// Currently downloading and installing
    Updating,
    /// Linux only: the new binary has been downloaded to `tmp_path`, but the
    /// install directory is not writable by the current user. Prompt the user
    /// to elevate so we can place the binary at `install_path`.
    NeedsElevation {
        version: String,
        install_path: std::path::PathBuf,
        tmp_path: std::path::PathBuf,
    },
    /// Update completed successfully
    Updated(String),
    /// An error occurred
    Error(String),
}

pub(crate) struct TabState {
    pub(crate) table: DataTable,
    pub(crate) table_state: TableViewState,
    pub(crate) search_text: String,
    pub(crate) search_mode: data::SearchMode,
    pub(crate) show_replace_bar: bool,
    pub(crate) replace_text: String,
    pub(crate) filtered_rows: Vec<usize>,
    pub(crate) filter_dirty: bool,
    pub(crate) view_mode: ViewMode,
    pub(crate) raw_content: Option<String>,
    pub(crate) raw_content_modified: bool,
    /// Snapshot of the file content as it was on disk at load time. Used by
    /// the raw CSV/TSV view to switch between aligned and un-aligned forms,
    /// or to re-format under a different quote/escape mode, without going
    /// back to disk. `None` for files that weren't loaded as raw text.
    pub(crate) raw_content_original: Option<String>,
    /// Per-tab gate for raw-view column coloring. Defaults to `true`; flipped
    /// off by the slow-file prompt when the user enters the raw view of a
    /// large CSV/TSV. Not persisted — only governs this tab.
    pub(crate) raw_color_enabled: bool,
    /// Source-file size in bytes captured at load time. Used by the
    /// slow-file prompt that appears the first time the user enters raw view
    /// for a CSV/TSV above the threshold. `None` for non-text formats.
    pub(crate) raw_file_size: Option<u64>,
    /// Whether the slow-file prompt has already been shown (and either
    /// answered or dismissed) for this tab. Prevents re-prompting every time
    /// the user toggles back into the raw view.
    pub(crate) raw_perf_prompt_resolved: bool,
    pub(crate) raw_view_formatted: bool,
    pub(crate) csv_delimiter: u8,
    /// Quote convention used by the raw CSV/TSV column-alignment view.
    pub(crate) raw_csv_quote: RawCsvQuote,
    /// Escape convention for quoted fields in the raw CSV/TSV view.
    pub(crate) raw_csv_escape: RawCsvEscape,
    pub(crate) bg_row_buffer: Option<Arc<Mutex<Vec<Vec<data::CellValue>>>>>,
    pub(crate) bg_loading_done: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) bg_can_load_more: bool,
    pub(crate) bg_file_exhausted: Arc<std::sync::atomic::AtomicBool>,
    /// Pending vertical scroll offset for the markdown view's ScrollArea —
    /// set when the user clicks a `#fragment` link, applied next frame.
    pub(crate) markdown_scroll_target: Option<f32>,
    /// Layout mode for the Markdown view (Preview / Split / Edit). Default
    /// is `Split` so live editing is the out-of-the-box experience.
    pub(crate) markdown_layout: data::MarkdownLayout,
    /// Cached output of `pre_render_html` keyed by content hash. Avoids
    /// re-running 8+ regex passes on every keystroke when the user edits
    /// markdown in the split view.
    pub(crate) markdown_render_cache: Option<(u64, String)>,
    pub(crate) json_tree_expanded: std::collections::HashSet<String>,
    pub(crate) json_value: Option<serde_json::Value>,
    /// Parsed YAML root, converted to a `serde_json::Value` so the same tree
    /// renderer handles both formats. Populated at load time for `.yaml`/`.yml`
    /// files and consumed by `render_yaml_tree_view`. `None` for non-YAML tabs.
    pub(crate) yaml_value: Option<serde_json::Value>,
    pub(crate) json_expand_depth: usize,
    pub(crate) json_expand_depth_str: String,
    /// Maximum nesting depth of `json_value`, computed once at load. Cached
    /// here so the tree renderer doesn't walk the whole tree every frame just
    /// to label the depth slider.
    pub(crate) json_file_max_depth: usize,
    pub(crate) json_edit_path: Option<String>,
    pub(crate) json_edit_buffer: String,
    /// Width snapshot of the displayed JSON value when entering edit mode,
    /// so the TextEdit doesn't shrink as the user types.
    pub(crate) json_edit_width: Option<f32>,
    /// Key currently being renamed in the JSON/YAML tree. Stored as the
    /// key's *full path* (e.g. `users[0].name`); the parent path and old
    /// key name are derived by `split_key_path` at commit time.
    pub(crate) tree_key_edit_path: Option<String>,
    /// Live buffer for the key-rename TextEdit. Initialized with the key
    /// being renamed; committed via Enter, cancelled via Escape.
    pub(crate) tree_key_edit_buffer: String,
    /// One-shot scratch state for the "Add new key" prompt rendered on
    /// expanded objects. Tracks which container path is being targeted
    /// plus the new-key buffer. `None` when no add prompt is active.
    pub(crate) tree_add_key_path: Option<String>,
    pub(crate) tree_add_key_buffer: String,
    pub(crate) show_add_column_dialog: bool,
    pub(crate) new_col_name: String,
    pub(crate) new_col_type: String,
    pub(crate) new_col_formula: String,
    pub(crate) insert_col_at: Option<usize>,
    /// Live buffer for the "Insert at position" TextEdit in the Insert
    /// Column dialog. Reset to empty on dialog close so the next open
    /// re-derives the default position from `insert_col_at`.
    pub(crate) insert_col_at_text: String,
    pub(crate) show_delete_columns_dialog: bool,
    pub(crate) delete_col_selection: Vec<bool>,
    pub(crate) sql_query: String,
    pub(crate) sql_result: Option<DataTable>,
    pub(crate) sql_error: Option<String>,
    /// Whether the SQL panel is currently visible alongside the table view.
    pub(crate) sql_panel_open: bool,
    /// Autocomplete popup: currently highlighted suggestion index (clamped
    /// to the live suggestion list each frame).
    pub(crate) sql_ac_selected: usize,
    /// Autocomplete popup: set to `false` by Escape to hide the popup until
    /// the user types again. Reset to `true` on any text change.
    pub(crate) sql_ac_visible: bool,
    /// Whether the first data row in the file is being treated as column
    /// headers (the default for most readers). When toggled off, the headers
    /// are pushed back into row 0 and column names become `column_1..N`.
    pub(crate) first_row_is_header: bool,
    /// Whether the Column Inspector modal is open for this tab.
    pub(crate) show_column_inspector: bool,
    /// Sort order applied inside the Column Inspector (view-only).
    pub(crate) column_inspector_sort: ColumnInspectorSort,
    /// Window-size mode for the Column Inspector (Normal/Maximized/Minimized).
    pub(crate) column_inspector_size: ui::settings::DialogSize,
    /// Selected row indices (display-position indices) inside the Column
    /// Inspector. Drives Ctrl+C / context-menu copy. Cleared when the dialog
    /// closes.
    pub(crate) column_inspector_selected: std::collections::HashSet<usize>,
    /// Anchor index for Shift+click range selection in the Column Inspector.
    pub(crate) column_inspector_anchor: Option<usize>,
    /// Column index whose value-frequency dialog is currently open for this
    /// tab. `None` = dialog closed. Set by Ctrl+Shift+I, column-header
    /// right-click → "Value frequency…", or the Edit menu.
    pub(crate) value_frequency_col: Option<usize>,
    /// Top-N cap shown in the value-frequency dialog. `None` means "all
    /// distinct values". Defaults to `Some(50)` per the F3 plan.
    pub(crate) value_frequency_top_n: Option<usize>,
    /// Whether numeric columns are auto-binned (Sturges) in the value-
    /// frequency dialog. Ignored for non-numeric columns.
    pub(crate) value_frequency_bin_numeric: bool,
    /// Window-size mode for the Value Frequency dialog.
    pub(crate) value_frequency_size: ui::settings::DialogSize,
    /// Whether the "Find duplicates…" dialog is open on this tab.
    pub(crate) show_find_duplicates: bool,
    /// Column indices selected as the dedupe key in the Find Duplicates
    /// dialog. Re-seeded from the active selection when the dialog opens;
    /// empty until the user picks columns.
    pub(crate) find_duplicates_key_cols: std::collections::HashSet<usize>,
    /// Output mode: highlight the duplicate rows in place, or open them
    /// in a new tab.
    pub(crate) find_duplicates_mode: FindDuplicatesMode,
    /// Columns hidden from the table view. Indices map into
    /// `table.columns`. Hidden columns keep their data intact (Save still
    /// writes them); the renderer just zeroes their visible width so they
    /// disappear from view. Transient — not persisted across sessions, same
    /// precedent as `column_filters`.
    pub(crate) hidden_columns: std::collections::HashSet<usize>,
    /// Whether this tab is pinned. Pinned tabs render with a 📌 prefix,
    /// hide their × close button, and refuse to close via Ctrl+W or the
    /// unsaved-changes path. File-backed pinned tabs survive across
    /// restarts via `AppSettings.pinned_tabs` (scratch tabs cannot be
    /// pinned).
    pub(crate) pinned: bool,
    /// Whether this tab is a *chart tab* — created via the **Analyse →
    /// Chart** toolbar button rather than loaded from a file. Chart tabs
    /// hold a snapshot of the source table, render only the Chart view,
    /// don't appear in the file-save / pin paths, and their title is
    /// derived from the source filename.
    pub(crate) is_chart_tab: bool,
    /// Display label for a chart tab. Set when the tab is opened so the
    /// tab strip can show e.g. "Chart \u{2014} sales.parquet". Ignored on
    /// non-chart tabs.
    pub(crate) chart_tab_label: Option<String>,
    /// Excel-style per-column value-set filters. Keys are column indices;
    /// values are the set of cell `to_string()` representations that should
    /// remain visible. Absent key = no filter on that column. Empty set is
    /// never written (an "allow nothing" filter would just hide every row, so
    /// we interpret it as "remove the filter" on Apply / Clear).
    pub(crate) column_filters: std::collections::HashMap<usize, std::collections::HashSet<String>>,
    /// Whether the Column Filter modal is open for this tab.
    pub(crate) show_column_filter: bool,
    /// Window-size mode for the Column Filter dialog.
    pub(crate) column_filter_size: ui::settings::DialogSize,
    /// Which column the dialog is currently editing. `None` means no column
    /// is selectable (table has zero columns) — the dialog won't open in that
    /// case.
    pub(crate) column_filter_picker_col: Option<usize>,
    /// Type-to-filter buffer for the value list inside the dialog.
    pub(crate) column_filter_value_search: String,
    /// Draft set of allowed values for the currently picked column. Committed
    /// to `column_filters[picker_col]` on Apply; discarded on Cancel.
    pub(crate) column_filter_draft_allowed: std::collections::HashSet<String>,
    /// One-shot flag: when true, the dialog's next render seeds the draft
    /// with the column's full set of unique values (so the user sees every
    /// checkbox ticked). Set by `open_column_filter_dialog` and by column
    /// switches; consumed (set back to false) by the dialog after seeding.
    /// Without this, "Select none" + frame-flip would immediately re-seed
    /// and undo the user's intent.
    pub(crate) column_filter_needs_seed: bool,
    /// Set to true when this tab represents an empty (0-byte) file. Renders
    /// the easter-egg ASCII art instead of the table view.
    pub(crate) empty_file_placeholder: bool,
    /// Dismissible warning banner shown above the raw text editor when the
    /// originally-detected format failed to parse and we fell back to plain
    /// text. Contains the format name and the parser's error message. `None`
    /// when no banner is active.
    pub(crate) parse_error_banner: Option<String>,
    /// Right-side path for the Compare view. `None` means the user hasn't
    /// picked a comparison target yet — the menu entry "View → Compare
    /// with…" sets this and the active `view_mode` to `Compare`.
    pub(crate) compare_right_path: Option<std::path::PathBuf>,
    /// Right-side raw text content for the Compare view's TextDiff mode.
    /// Loaded eagerly when "Compare with…" is invoked.
    pub(crate) compare_right_raw: Option<String>,
    /// Right-side `DataTable` for the Compare view's RowHashDiff mode.
    /// Boxed so the inline size of `TabState` doesn't grow noticeably
    /// when compare isn't in use.
    pub(crate) compare_right_table: Option<Box<data::DataTable>>,
    /// Which Compare sub-mode is active (Text Diff / Row Hash Diff).
    pub(crate) compare_mode: data::CompareMode,
    /// Column indices on the LEFT (active) table fed into the row hasher.
    /// Empty means "hash every column" (the default until the user picks).
    pub(crate) compare_columns_left: Vec<usize>,
    /// Column indices on the RIGHT table fed into the row hasher.
    /// Empty means "hash every column".
    pub(crate) compare_columns_right: Vec<usize>,
    /// Error banner shown above the Compare view (e.g. failed to load
    /// the right-side file). Dismissable.
    pub(crate) compare_error: Option<String>,
    /// Markdown payload for each EPUB chapter, in spine order. Populated by
    /// `apply_loaded_table` from `epub_reader::read_with_extras`; consumed
    /// by `view_modes::epub_reader::render_epub_view`. Empty for non-EPUB
    /// tabs.
    pub(crate) epub_chapters_md: Vec<String>,
    /// Best-effort per-chapter labels (manifest href filename or
    /// `"Chapter N"`). Same order as `epub_chapters_md`.
    pub(crate) epub_chapter_titles: Vec<String>,
    /// Decoded image bytes keyed by manifest href. The reading view
    /// resolves `![](href)` references from the chapter Markdown against
    /// this map at paint time. Empty for non-EPUB tabs.
    pub(crate) epub_image_bytes: std::collections::HashMap<String, Vec<u8>>,
    /// Texture cache for images already uploaded to egui. Keyed by manifest
    /// href. Populated on first paint of a chapter that references the
    /// image; survives chapter switches so we don't re-decode every flip.
    pub(crate) epub_image_textures: std::collections::HashMap<String, egui::TextureHandle>,
    /// Currently-displayed chapter index (0-based) in the EPUB view.
    pub(crate) epub_active_chapter: usize,
    /// Best-effort EPUB book title (from `<dc:title>`). Shown in the
    /// reading view's chapter list header. `None` for non-EPUB tabs and
    /// EPUBs with no title meta.
    pub(crate) epub_title: Option<String>,
    /// Parsed GeoJSON features for the Map view, in the same order as the
    /// flat table rows. Populated by `apply_loaded_table` from
    /// `geojson_reader::read_with_features`. Empty for non-GeoJSON tabs.
    pub(crate) geojson_features: Vec<octa::formats::geojson_reader::MapFeature>,
    /// Per-tab map rendering mode. Initialised from
    /// `AppSettings.map_default_mode`; flipped by the Map toolbar's
    /// Tiles/Geometry toggle.
    pub(crate) map_mode: data::MapMode,
    /// `walkers::HttpTiles` is lazily instantiated when the Map view
    /// first renders (needs the egui `Context`). `None` until then or
    /// while the user is in `GeometryOnly` mode.
    pub(crate) map_tiles: Option<Box<walkers::HttpTiles>>,
    /// `walkers::MapMemory` tracks per-frame state (zoom, pan, etc.).
    /// `None` until the Map view renders.
    pub(crate) map_memory: Option<Box<walkers::MapMemory>>,
    /// Per-tab Chart view config (kind, X/Y columns, aggregation). Transient
    /// — not persisted, so the chart doesn't reappear on the wrong file next
    /// session. Seeded on first entry to the Chart view by
    /// `render_chart_view::seed_defaults`.
    pub(crate) chart_config: octa::data::chart::ChartConfig,
    /// Staging buffers for the Customise numeric inputs. egui's `DragValue`
    /// always flashes the horizontal-resize cursor on hover, which reads as
    /// "drag to adjust" — confusing here. We render each input as a plain
    /// `TextEdit` instead and parse the string back into `chart_config` on
    /// every change. Each buffer is empty when the corresponding `Option`
    /// is `None`, otherwise holds the f64 / usize formatted for display.
    pub(crate) chart_buffers: ChartInputBuffers,
}

/// Text-input staging buffers for the Chart Customise section. Kept on
/// `TabState` (not on `ChartConfig`) because they're UI scratch state that
/// shouldn't end up in any serialisation of the chart config.
#[derive(Default, Debug, Clone)]
pub(crate) struct ChartInputBuffers {
    pub hist_bins: String,
    pub y_min: String,
    pub y_max: String,
    pub y_step: String,
}

pub(crate) struct OctaApp {
    pub(crate) tabs: Vec<TabState>,
    pub(crate) active_tab: usize,
    pub(crate) pending_close_tab: Option<usize>,
    pub(crate) registry: FormatRegistry,
    pub(crate) theme_mode: ThemeMode,
    pub(crate) settings: AppSettings,
    /// The concrete icon variant in use for this session. Equals
    /// `settings.icon_variant` for non-Random; for Random, holds the
    /// once-per-launch rolled color so toolbar/window icons stay consistent.
    pub(crate) resolved_icon: IconVariant,
    pub(crate) settings_dialog: SettingsDialog,
    /// Whether the search text field should be focused next frame.
    pub(crate) search_focus_requested: bool,
    /// "Unsaved changes" dialog state
    pub(crate) show_close_confirm: bool,
    /// Whether we already decided to quit (skip further confirm)
    pub(crate) confirmed_close: bool,
    /// System clipboard handle (shared, lazily initialized)
    pub(crate) os_clipboard: Option<Arc<Mutex<arboard::Clipboard>>>,
    /// Logo texture for toolbar (small, native SVG size)
    pub(crate) logo_texture: Option<egui::TextureHandle>,
    /// Logo texture for welcome screen (large, rendered from SVG at high resolution)
    pub(crate) welcome_logo_texture: Option<egui::TextureHandle>,
    /// File paths passed via command line (loaded on first frame). Each path
    /// opens in its own tab; the first replaces the empty welcome tab.
    pub(crate) initial_files: Vec<std::path::PathBuf>,
    /// Pending file to open after unsaved-changes dialog resolves
    pub(crate) pending_open_file: bool,
    /// Show unsaved-changes dialog before opening a new file
    pub(crate) show_open_confirm: bool,
    /// Show the About dialog
    pub(crate) show_about_dialog: bool,
    /// Show the Documentation dialog
    pub(crate) show_documentation_dialog: bool,
    /// Window-size mode for the Documentation dialog.
    pub(crate) documentation_size: DialogSize,
    /// Index of the active documentation section (sidebar selection). Reset
    /// to 0 each time the dialog opens.
    pub(crate) docs_active_section: usize,
    /// Show the Update dialog
    pub(crate) show_update_dialog: bool,
    /// Confirm before reloading the raw CSV/TSV file when un-aligning columns.
    pub(crate) show_unalign_confirm: bool,
    /// Update check state shared with background thread
    pub(crate) update_state: Arc<Mutex<UpdateState>>,
    pub(crate) status_message: Option<(String, std::time::Instant)>,
    /// Recently opened file paths (most recent first).
    pub(crate) recent_files: Vec<String>,
    /// Zoom level in percent (100 = default, steps of 5).
    pub(crate) zoom_percent: u32,
    /// Status bar navigation input buffer.
    pub(crate) nav_input: String,
    /// Focus the status-bar navigation input next frame (Ctrl+G / Go To Cell).
    pub(crate) nav_focus_requested: bool,
    /// Confirm before reloading the file from disk and losing unsaved edits.
    pub(crate) show_reload_confirm: bool,
    /// Pending modal table picker (DB sources containing multiple tables).
    pub(crate) pending_table_picker: Option<ui::table_picker::TablePickerState>,
    /// Files queued for batch open (e.g. from a multi-select File→Open dialog
    /// or multiple paths on the command line). Drained one per frame so that
    /// any modal picker that surfaces during a load (e.g. multi-table DB)
    /// pauses the queue naturally until the user resolves it.
    pub(crate) pending_open_queue: std::collections::VecDeque<std::path::PathBuf>,
    /// Stack of recently-closed tabs for the `ReopenLastClosedTab` shortcut
    /// (default Ctrl+Shift+T). Most recent close is at the back; capped at
    /// `MAX_CLOSED_TAB_HISTORY`. Each snapshot carries enough state to
    /// reopen — path-backed tabs reload from disk, scratch tabs restore
    /// the full `TabState` clone verbatim.
    pub(crate) recently_closed_tabs: std::collections::VecDeque<ClosedTabSnapshot>,
    /// Tab indices the user marked via Ctrl-click on the tab bar — used to
    /// drive tab-vs-tab compare (right-click menu / `CompareSelectedTabs`
    /// shortcut). Cleared on any plain (non-Ctrl) tab click. Does not
    /// include the active tab; the active tab is always treated as one
    /// participant in compare.
    pub(crate) tab_multi_selection: std::collections::HashSet<usize>,
    /// Queue of columns whose date inference was ambiguous (US vs European)
    /// and need user confirmation. Each entry is shown as a modal one at a
    /// time; the head of the queue is the active dialog.
    pub(crate) pending_date_pickers: std::collections::VecDeque<DateAmbiguity>,
    /// One-shot prompt offered when a large CSV/TSV is opened: keep coloring
    /// and alignment on (slow but full-featured) or disable them just for
    /// this file. `None` while no prompt is pending. Resolved by the user via
    /// `raw_perf_prompt::render_raw_perf_prompt_dialog`.
    pub(crate) pending_raw_perf_prompt: Option<RawPerfPrompt>,
    /// Pending date-format-change banner to render above the central panel.
    /// Set by `run_date_inference_pass` whenever one or more columns are
    /// promoted with a non-ISO source layout. `None` once dismissed.
    pub(crate) pending_date_warning: Option<DateWarning>,
    /// Pending "Parse in new tab" modal. Set when the user picks a scope
    /// from the Edit menu or right-click; cleared when the modal is
    /// dismissed (Cancel) or the parse succeeds (Open).
    pub(crate) pending_parse_modal: Option<crate::app::dialogs::parse_in_new_tab::ParseModalState>,
    /// Active Schema Export dialog target + window size, or `None` when
    /// the dialog isn't open. Switching targets while the dialog is up
    /// mutates `target` in place; closing the dialog clears the field.
    pub(crate) schema_export: Option<SchemaExportState>,
    /// Currently opened directory tree sidebar (`None` = sidebar hidden).
    pub(crate) directory_tree: Option<ui::directory_tree::DirectoryTreeState>,
    /// How many key presses of the Konami sequence have been matched so far.
    pub(crate) konami_index: u8,
    /// Wall-clock deadline up to which the confetti overlay is animated.
    pub(crate) confetti_until: Option<std::time::Instant>,
    /// Click counter on the toolbar Octa logo. Reaching 7 clicks within
    /// `LOGO_CLICK_WINDOW` activates the hidden Rainbow theme.
    pub(crate) logo_click_count: u8,
    /// Most recent click timestamp on the Octa logo. Used to expire stale
    /// streaks from `logo_click_count`.
    pub(crate) logo_last_click: Option<std::time::Instant>,
    /// `true` while the hidden Rainbow theme is active. Decoupled from
    /// `theme_mode == Rainbow` so the surrounding code can keep using
    /// `theme_mode` without surprise.
    pub(crate) rainbow_active: bool,
    /// Click counter on the welcome-screen logo. Reaching 3 clicks within
    /// `WELCOME_LOGO_CLICK_WINDOW` triggers the snowfall easter egg. Reset
    /// once the snowfall starts or the window expires.
    pub(crate) welcome_logo_click_count: u8,
    /// Timestamp of the most recent welcome-logo click.
    pub(crate) welcome_logo_last_click: Option<std::time::Instant>,
    /// Wall-clock deadline up to which the snowfall overlay is animated.
    /// `None` when no snow is falling.
    pub(crate) snowfall_until: Option<std::time::Instant>,
    /// Session-only read-only mode. When `true`, every editing path
    /// (cell edits, structural changes, marks, undo/redo, cut/paste,
    /// raw-text editor, SQL DML) short-circuits. Toggled via the
    /// `ToggleReadOnly` shortcut (default F8). NOT persisted — every
    /// launch starts editable.
    pub(crate) readonly_mode: bool,
    /// Pending modal that announces the current read-only state
    /// (enabled / disabled). `None` while no notice is queued. Shown
    /// once per toggle; suppressible globally via Settings.
    pub(crate) pending_readonly_notice: Option<ReadOnlyNotice>,
    /// One-shot flag: cleared on the first frame after Octa enqueues its
    /// pinned-tab restore set. Without it the pin-load block would re-run
    /// every frame (since `initial_files` empties on first frame anyway).
    pub(crate) startup_pin_load_done: bool,
    /// Multi-search panel state — query, scope, results, background
    /// worker. Initialised hidden; opened via **Search → Multi-search…**
    /// or the `MultiSearch` keyboard shortcut.
    pub(crate) multi_search: super::multi_search::MultiSearchState,
}

/// Snapshot of a read-only-toggle event used by the notice modal. Captures
/// the post-toggle state so the dialog text reads correctly even if the
/// user re-toggles before dismissing.
pub(crate) struct ReadOnlyNotice {
    pub(crate) is_active: bool,
    /// Holds the live "Don't show this again" checkbox state across frames.
    /// Initialized when the notice is queued; on OK we copy this value
    /// back to `AppSettings.show_readonly_notice`. Without this field the
    /// checkbox would flicker because the dialog body re-derives its
    /// initial value from settings every frame.
    pub(crate) suppress_future: bool,
}
