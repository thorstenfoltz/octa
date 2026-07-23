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

mod dialogs;
pub(crate) use dialogs::*;

/// Maximum number of recently-closed tabs Octa remembers for the
/// `ReopenLastClosedTab` shortcut. Matches the convention browsers use.
pub(crate) const MAX_CLOSED_TAB_HISTORY: usize = 10;

/// Where a tab was opened from in the cloud. Set when a file is downloaded
/// from a [`octa::cloud::CloudConnection`]; the tab's `source_path` points at
/// the downloaded temp copy. Save-back (gated by `cloud_writes_enabled`)
/// uploads to `key` on the connection identified by `conn_id`.
#[derive(Debug, Clone)]
pub(crate) struct CloudOrigin {
    pub(crate) conn_id: String,
    pub(crate) key: String,
}

/// Which live database table a tab was opened from (the sidebar Databases
/// tree). Editable only when the connection allows writes AND the table has
/// a primary key (`OctaApp::db_origin_writable`); otherwise
/// `OctaApp::is_readonly` treats the tab as locked, so no edit path has to
/// know about them individually. Saving an editable tab writes the diff back
/// to the server after a confirmation dialog.
#[derive(Debug, Clone)]
pub(crate) struct DbOrigin {
    pub(crate) conn_id: String,
    /// Catalog for three-level engines (Snowflake/Databricks/BigQuery), else None.
    pub(crate) catalog: Option<String>,
    pub(crate) schema: String,
    pub(crate) table: String,
    /// Primary-key column names in ordinal order; empty = no PK found
    /// (write-back has no row identity, so the tab stays read-only).
    pub(crate) pk_cols: Vec<String>,
}

pub(crate) struct TabState {
    pub(crate) table: DataTable,
    /// Set when the chat assistant changed this tab's table in place
    /// (`edit_open_tab`). Consumed by the next manual save to decide whether to
    /// back up the original file first ("Back up before modifying"), so the
    /// user's own edits never trigger a backup but the assistant's do.
    /// Session-only.
    pub(crate) assistant_modified: bool,
    pub(crate) table_state: TableViewState,
    pub(crate) search_text: String,
    pub(crate) search_mode: data::SearchMode,
    /// Case-sensitive search toggle (`Aa` button in the search bar). When off,
    /// matching is case-insensitive across all modes. Session-only, per tab.
    pub(crate) search_case_sensitive: bool,
    /// Whole-word search toggle. When on, matches are bounded by word
    /// boundaries (`\b`). Session-only, per tab.
    pub(crate) search_whole_word: bool,
    /// Which columns the search applies to. `None` = whole table; `Some(col)`
    /// restricts matching to one column. Session-only, per tab.
    pub(crate) search_scope_col: Option<usize>,
    pub(crate) show_replace_bar: bool,
    pub(crate) replace_text: String,
    pub(crate) filtered_rows: Vec<usize>,
    pub(crate) filter_dirty: bool,
    /// Highlight-search navigation state (count, current index, pending jump).
    pub(crate) search_nav: SearchNavState,
    /// In Highlight mode, the (data-row, col) cells whose value matches the
    /// query, in display order. Empty in Filter mode. Recomputed by
    /// `recompute_filter`.
    pub(crate) search_cell_matches: Vec<(usize, usize)>,
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
    /// large CSV/TSV. Not persisted - only governs this tab.
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
    /// Pending vertical scroll offset for the markdown view's ScrollArea -
    /// set when the user clicks a `#fragment` link, applied next frame.
    pub(crate) markdown_scroll_target: Option<f32>,
    /// Layout mode for the Markdown view (Preview / Split / Edit). Default
    /// is `Preview` so opening a markdown file shows the rendered document.
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
    /// Active "Date/Time calculation" dialog state, or `None` when closed.
    pub(crate) time_calc: Option<TimeCalcDialog>,
    pub(crate) sql_query: String,
    pub(crate) sql_result: Option<DataTable>,
    pub(crate) sql_error: Option<String>,
    /// Clicked cell in the SQL result grid (row, col), highlighted and used as
    /// the Ctrl+C copy target - mirrors the main table's click-to-select-cell
    /// behaviour so copy is predictable from the result view.
    pub(crate) sql_result_selected: Option<(usize, usize)>,
    /// Whether the SQL panel is currently visible alongside the table view.
    pub(crate) sql_panel_open: bool,
    /// SQL panel target for a live-database tab: `true` runs the query on
    /// the server (native dialect), `false` on the local DuckDB snapshot.
    /// Meaningless while `db_origin` is None.
    pub(crate) sql_run_on_server: bool,
    /// Set to `true` when the SQL panel is opened so the editor grabs keyboard
    /// focus on the next frame (the user can start typing immediately without
    /// clicking). Consumed (cleared) by `draw_sql_editor`.
    pub(crate) sql_editor_focus_pending: bool,
    /// Autocomplete popup: currently highlighted suggestion index (clamped
    /// to the live suggestion list each frame).
    pub(crate) sql_ac_selected: usize,
    /// Autocomplete popup: set to `false` by Escape to hide the popup until
    /// the user types again. Reset to `true` on any text change.
    pub(crate) sql_ac_visible: bool,
    /// Per-tab multi-table SQL workspace. Lazily constructed on the first
    /// SQL action (panel open or query run). Carries the tab's `data`
    /// table plus any extras the user has added and any ATTACH-ed DBs.
    /// `None` until then so opening a tab doesn't pay the DuckDB
    /// connection cost up front.
    pub(crate) sql_workspace: Option<octa::sql::SqlWorkspace>,
    /// Last successfully executed SELECT, kept verbatim so the write-back
    /// dialog has a source query to compose `CREATE TABLE AS ...` from.
    pub(crate) sql_last_query: String,
    /// Recent executed queries for this tab (most-recent first), session-only.
    /// Surfaced in the SQL panel's History dropdown. Capped in `run_workspace_query`.
    pub(crate) sql_history: Vec<String>,
    /// Cells/rows temporarily marked to show what the last SQL mutation
    /// changed; cleared once `sql_diff_highlight_until` passes.
    pub(crate) sql_diff_marks: Vec<data::MarkKey>,
    /// When the post-mutation row-diff highlight expires. `None` when no
    /// highlight is active.
    pub(crate) sql_diff_highlight_until: Option<std::time::Instant>,
    /// Toggle for the collapsible Workspace section at the top of the SQL
    /// panel. Off by default to keep the panel compact for users who only
    /// query `data`.
    pub(crate) sql_workspace_open: bool,
    /// Currently selected entry in the workspace tree; drives the inspector
    /// pane on the right side of the workspace section. `None` shows the
    /// inspector's empty-state hint.
    pub(crate) sql_inspector_selection: Option<crate::app::sql_panel::InspectorTarget>,
    /// Cache of [`SqlWorkspace`] introspection results keyed by the same
    /// `InspectorTarget` shape. Populated on demand when the user selects an
    /// entry; reset by `clear_inspector_cache` on workspace mutations
    /// (refresh, add, remove, attach, detach).
    pub(crate) sql_inspector_cache:
        std::collections::HashMap<crate::app::sql_panel::InspectorTarget, InspectorCacheEntry>,
    /// Per-attachment expansion state for the workspace tree (alias ->
    /// expanded?). Schemas inside an attachment use the keys `(alias,
    /// schema)`; we use a single map and synthesise the key.
    pub(crate) sql_workspace_tree_expanded: std::collections::HashSet<String>,
    /// State for the SQL write-back dialog. `None` when the dialog is
    /// closed. Lives on the tab so write-back state survives toggling
    /// between tabs.
    pub(crate) sql_write_back: Option<super::dialogs::sql_write_back::SqlWriteBackState>,
    /// Whether the first data row in the file is being treated as column
    /// headers (the default for most readers). When toggled off, the headers
    /// are pushed back into row 0 and column names become `column_1..N`.
    pub(crate) first_row_is_header: bool,
    /// Column index whose value-frequency dialog is currently open for this
    /// tab. `None` = dialog closed. Set by Ctrl+Shift+I, column-header
    /// right-click -> "Value frequency...", or the Edit menu.
    pub(crate) value_frequency_col: Option<usize>,
    /// Top-N cap shown in the value-frequency dialog. `None` means "all
    /// distinct values". Defaults to `Some(50)` per the F3 plan.
    pub(crate) value_frequency_top_n: Option<usize>,
    /// Whether numeric columns are auto-binned (Sturges) in the value-
    /// frequency dialog. Ignored for non-numeric columns.
    pub(crate) value_frequency_bin_numeric: bool,
    /// Custom bin count for numeric value-frequency binning. `None` =
    /// Sturges (the default). `Some(n)` overrides with exactly `n` bins.
    pub(crate) value_frequency_bins: Option<usize>,
    /// Text buffer backing the "Bins:" input in the value-frequency dialog.
    pub(crate) value_frequency_bins_buf: String,
    /// Window-size mode for the Value Frequency dialog.
    pub(crate) value_frequency_size: ui::settings::DialogSize,
    /// When `true`, the value-frequency *column picker* is open - used when
    /// the feature is launched with no column context (Analyse menu, or the
    /// shortcut with no cell selected). On confirm it sets
    /// `value_frequency_col`.
    pub(crate) value_frequency_pick: bool,
    /// Per-column number-display format (decimals + rounding). Keys are
    /// column indices into `table.columns`, same index-keyed precedent as
    /// `column_filters` / `hidden_columns`. Display-only: Save asks the user
    /// before applying rounding to the written values.
    pub(crate) column_number_formats:
        std::collections::HashMap<usize, octa::data::num_format::NumberFormat>,
    /// Conditional-formatting rules colouring cells whose value matches a
    /// predicate. Evaluated against every visible cell; the first matching
    /// rule wins (see `octa::data::conditional_format`). Session-only, like
    /// `column_number_formats` and manual marks.
    pub(crate) conditional_format_rules: Vec<octa::data::conditional_format::CondRule>,
    /// Whether the "Conditional formatting..." dialog is open on this tab.
    pub(crate) show_conditional_format: bool,
    /// Conditional-formatting dialog window sizing (Normal/Maximized/Minimized).
    pub(crate) conditional_format_size: ui::settings::DialogSize,
    /// Data-validation rules for this tab. Cells failing any rule are painted
    /// red by the renderer (see `octa::data::validation`). Session-only.
    pub(crate) validation_rules: Vec<octa::data::validation::ValidationRule>,
    /// Cached set of `(row, col)` cells failing a validation rule, recomputed in
    /// `recompute_filter` (like `search_cell_matches`) so the renderer stays cheap.
    pub(crate) validation_violations: std::collections::HashSet<(usize, usize)>,
    /// Cells flagged as numeric outliers by the Detect-outliers dialog. Painted
    /// orange by the renderer (see `octa::data::outliers`). Session-only; a
    /// snapshot stamped on Apply (not recomputed as rows change).
    pub(crate) outlier_cells: std::collections::HashSet<(usize, usize)>,
    /// Whether the "Data validation..." dialog is open on this tab.
    pub(crate) show_validation: bool,
    /// Data-validation dialog window sizing (Normal/Maximized/Minimized).
    pub(crate) validation_size: ui::settings::DialogSize,
    /// Column index whose Number-format dialog is open. `None` = closed.
    /// This is the "primary" column (drives the dialog title + preview); the
    /// chosen format applies to every column in `column_format_cols`.
    pub(crate) column_format_col: Option<usize>,
    /// All columns the Number-format dialog currently applies to. Seeded from
    /// the selection when the dialog opens and editable in-dialog via a column
    /// picker, so one configuration can round several columns at once.
    pub(crate) column_format_cols: Vec<usize>,
    /// Text buffer backing the decimals input in the Number-format dialog.
    /// Seeded when the dialog opens; parsed live into `column_number_formats`.
    pub(crate) column_format_decimals_buf: String,
    /// Whether the "Find duplicates..." dialog is open on this tab.
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
    /// disappear from view. Transient - not persisted across sessions, same
    /// precedent as `column_filters`.
    pub(crate) hidden_columns: std::collections::HashSet<usize>,
    /// Whether "Filter to marked" is active on this tab. While active,
    /// `recompute_filter` keeps only marked rows and unmarked columns are added
    /// to `hidden_columns`. Session-only.
    pub(crate) mark_filter_active: bool,
    /// Snapshot of `hidden_columns` taken when "Filter to marked" was engaged,
    /// so toggling it off restores exactly the columns the user had hidden
    /// manually (rather than un-hiding everything). `None` when inactive.
    pub(crate) mark_filter_hidden_snapshot: Option<std::collections::HashSet<usize>>,
    /// Named jump targets within this tab (session-only, fixed position: they
    /// do not track later row inserts/deletes). See [`Bookmark`].
    pub(crate) bookmarks: Vec<Bookmark>,
    /// Whether this tab is pinned. Pinned tabs render with a 📌 prefix,
    /// hide their × close button, and refuse to close via Ctrl+W or the
    /// unsaved-changes path. File-backed pinned tabs survive across
    /// restarts via `AppSettings.pinned_tabs` (scratch tabs cannot be
    /// pinned).
    pub(crate) pinned: bool,
    /// Whether this tab is a *chart tab* - created via the **Analyse ->
    /// Chart** toolbar button rather than loaded from a file. Chart tabs
    /// hold a snapshot of the source table, render only the Chart view,
    /// don't appear in the file-save / pin paths, and their title is
    /// derived from the source filename.
    pub(crate) is_chart_tab: bool,
    /// Display label for a chart tab. Set when the tab is opened so the
    /// tab strip can show e.g. "Chart - sales.parquet". Ignored on
    /// non-chart tabs.
    pub(crate) chart_tab_label: Option<String>,
    /// Optional fixed label for derived non-chart tabs (e.g.
    /// "Summary - sales.parquet"). When set it overrides the
    /// source-path-based title; `None` keeps the normal behaviour.
    pub(crate) custom_tab_label: Option<String>,
    /// User-chosen display name for this tab (via tab right-click ->
    /// "Rename tab..."). Overrides the auto-generated title in the tab strip
    /// only; the file path and on-disk name are unchanged. `None` = show the
    /// file name. Session-only.
    pub(crate) user_tab_name: Option<String>,
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
    /// is selectable (table has zero columns) - the dialog won't open in that
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
    /// picked a comparison target yet - the menu entry "View -> Compare
    /// with..." sets this and the active `view_mode` to `Compare`.
    pub(crate) compare_right_path: Option<std::path::PathBuf>,
    /// Right-side raw text content for the Compare view's TextDiff mode.
    /// Loaded eagerly when "Compare with..." is invoked.
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
    /// For non-GeoJSON tables plotted on the map: the (lat, lon) column
    /// indices currently driving `geojson_features`. `None` for GeoJSON tabs
    /// (whose geometry comes from the file) or before the Map view is opened.
    /// The Map view's column dropdown writes here and rebuilds the points.
    pub(crate) map_coord_cols: Option<(usize, usize)>,
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
    /// (not persisted), so the chart doesn't reappear on the wrong file next
    /// session. Seeded on first entry to the Chart view by
    /// `render_chart_view::seed_defaults`.
    pub(crate) chart_config: octa::data::chart::ChartConfig,
    /// Staging buffers for the Customise numeric inputs. egui's `DragValue`
    /// always flashes the horizontal-resize cursor on hover, which reads as
    /// "drag to adjust" - confusing here. We render each input as a plain
    /// `TextEdit` instead and parse the string back into `chart_config` on
    /// every change. Each buffer is empty when the corresponding `Option`
    /// is `None`, otherwise holds the f64 / usize formatted for display.
    pub(crate) chart_buffers: ChartInputBuffers,
    /// Set when this tab was opened from cloud storage. Carries the connection
    /// id + object key so a later save can write back (gated by
    /// `cloud_writes_enabled`). `None` for local files.
    pub(crate) cloud_origin: Option<CloudOrigin>,
    /// Set when the tab shows a live database table (read-only).
    pub(crate) db_origin: Option<DbOrigin>,
    /// Set when this tab was opened from a compressed file (`.gz` / `.zst`).
    /// The tab's own `source_path` points at the decompressed temp file;
    /// Save re-compresses that temp back onto the original path. Save As to
    /// any other path leaves the original compressed file untouched.
    pub(crate) compressed_origin: Option<CompressedOrigin>,
}

/// Provenance of a transparently decompressed tab: the compressed file the
/// user opened, its codec, and the decompressed temp file the tab actually
/// reads/writes. `temp` gates save-back: only a save landing on exactly this
/// temp path is re-compressed onto `original`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompressedOrigin {
    pub(crate) original: std::path::PathBuf,
    pub(crate) codec: octa::formats::compression::Codec,
    pub(crate) temp: std::path::PathBuf,
}

/// Text-input staging buffers for the Chart Customise section. Kept on
/// `TabState` (not on `ChartConfig`) because they're UI scratch state that
/// shouldn't end up in any serialisation of the chart config.
#[derive(Default, Debug, Clone)]
pub(crate) struct ChartInputBuffers {
    pub hist_bins: String,
    pub x_min: String,
    pub x_max: String,
    pub x_step: String,
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
    /// Session search behaviour (Filter vs Highlight). Seeded from
    /// `settings.search_result_mode`; the search-bar toggle edits this without
    /// persisting. Governs the table view only; text/tree views always
    /// highlight.
    pub(crate) search_result_mode: data::SearchResultMode,
    /// Recent search queries (most-recent first), persisted across sessions to
    /// `<config_dir>/search_history.json`. Surfaced in the search-box history
    /// dropdown; capped at `settings.search_history_limit`.
    pub(crate) search_history: Vec<String>,
    /// Saved SQL snippets (named query library), persisted to
    /// `<config_dir>/sql_snippets.json`. Offered in the SQL panel's Snippets
    /// dropdown.
    pub(crate) sql_snippets: Vec<super::sql_snippets::SqlSnippet>,
    /// Active "Save SQL snippet" dialog (name + description buffers + the query
    /// being saved), or `None` when closed.
    pub(crate) sql_snippet_save: Option<SqlSnippetDraft>,
    /// Whether the SQL snippets manager window is open (app-level; the snippet
    /// library is shared across tabs).
    pub(crate) sql_snippets_window_open: bool,
    /// Window-size mode for the SQL snippets manager window.
    pub(crate) sql_snippets_window_size: ui::settings::DialogSize,
    /// Saved chat prompts (named prompt library), persisted to
    /// `<config_dir>/chat_prompts.json`. Offered in the chat panel's Prompts
    /// manager window.
    pub(crate) chat_prompts: Vec<super::chat_prompts::ChatPrompt>,
    /// Active "Save chat prompt" dialog (name + description buffers + the prompt
    /// being saved), or `None` when closed.
    pub(crate) chat_prompt_save: Option<ChatPromptDraft>,
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
    /// Live filter text for the Documentation dialog's section search box.
    /// Empty shows every section; otherwise the sidebar narrows to sections
    /// whose title or body contains the query (case-insensitive).
    pub(crate) docs_search_query: String,
    /// Show the Update dialog
    pub(crate) show_update_dialog: bool,
    /// Confirm before reloading the raw CSV/TSV file when un-aligning columns.
    pub(crate) show_unalign_confirm: bool,
    /// Update check state shared with background thread
    pub(crate) update_state: Arc<Mutex<UpdateState>>,
    pub(crate) status_message: Option<(String, std::time::Instant)>,
    /// Last time the auto-save timer ran a pass (transient, set at startup and
    /// after each pass / Settings apply). Drives `drive_auto_save`.
    pub(crate) last_auto_save: std::time::Instant,
    /// Set at startup when the previous run ended uncleanly or a crash file is
    /// waiting; drives a one-shot "export a debug report?" dialog.
    pub(crate) pending_crash_offer: bool,
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
    /// Pending multi-select sheet picker, shown when an Excel workbook has
    /// more sheets than `excel_max_auto_sheets`. The user ticks which sheets
    /// to open (each in its own tab).
    pub(crate) pending_sheet_picker: Option<SheetPickerState>,
    /// A single-table file read running on a background thread (size-gated at
    /// `BACKGROUND_LOAD_MIN_BYTES`). Polled each frame by task C2; `None` when
    /// no background load is in flight.
    /// One-shot handoff from `load_file`'s decompression hook to
    /// `apply_loaded_table`: (original compressed path, codec, decompressed
    /// temp path). The load itself runs on the temp path; `apply_loaded_table`
    /// claims the origin only when the path it applies matches the recorded
    /// temp, so a failed load cannot mislabel the next unrelated open.
    pub(crate) pending_compressed_origin: Option<(
        std::path::PathBuf,
        octa::formats::compression::Codec,
        std::path::PathBuf,
    )>,
    pub(crate) pending_load: Option<PendingLoad>,
    /// Files queued for batch open (e.g. from a multi-select File->Open dialog
    /// or multiple paths on the command line). Drained one per frame so that
    /// any modal picker that surfaces during a load (e.g. multi-table DB)
    /// pauses the queue naturally until the user resolves it.
    pub(crate) pending_open_queue: std::collections::VecDeque<std::path::PathBuf>,
    /// Stack of recently-closed tabs for the `ReopenLastClosedTab` shortcut
    /// (default Ctrl+Shift+T). Most recent close is at the back; capped at
    /// `MAX_CLOSED_TAB_HISTORY`. Each snapshot carries enough state to
    /// reopen - path-backed tabs reload from disk, scratch tabs restore
    /// the full `TabState` clone verbatim.
    pub(crate) recently_closed_tabs: std::collections::VecDeque<ClosedTabSnapshot>,
    /// Tab indices the user marked via Ctrl-click on the tab bar - used to
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
    /// Pending near-miss date banner: columns that looked date-shaped but had
    /// unparseable values, so they were left as text. `None` once dismissed.
    pub(crate) pending_date_parse_warning: Option<DateParseWarning>,
    /// Pending whitespace-trim banner: the columns that had leading/trailing
    /// whitespace stripped on load. `None` once dismissed.
    pub(crate) pending_trim_warning: Option<TrimWarning>,
    /// Pending malformed-file repair prompt (opt-in via
    /// `offer_repair_on_malformed`). Set by `load_file` when a CSV/TSV looks
    /// malformed; resolved by `dialogs::repair_file::render_repair_file_dialog`.
    pub(crate) pending_file_repair: Option<FileRepair>,
    /// Pending "round on save?" prompt. Set when a save is requested on a tab
    /// that has per-column rounding formats; resolved by
    /// `round_save_prompt::render_round_save_prompt_dialog`.
    pub(crate) pending_round_save: Option<RoundSavePrompt>,
    /// Pending "apply schema changes?" prompt. Set when saving a DB tab whose
    /// columns differ from the on-disk schema; resolved by
    /// `schema_change_save::render_schema_change_save_dialog`.
    pub(crate) pending_schema_change_save: Option<SchemaChangeSavePrompt>,
    /// Pending live-DB write-back confirmation: raised by saving a modified
    /// db-origin tab; resolved by
    /// `db_write_back::render_db_write_back_dialog` (Confirm spawns the
    /// worker, Cancel keeps the edits).
    pub(crate) pending_db_write_back: Option<crate::app::dialogs::db_write_back::DbWriteBackPrompt>,
    /// In-flight live-DB write-back worker, if any (one at a time app-wide).
    pub(crate) db_write_back_job: Option<crate::app::dialogs::db_write_back::DbWriteBackJob>,
    /// Active "Copy table to another connection" dialog, or `None` when
    /// closed (see `src/app/dialogs/db_copy.rs`).
    pub(crate) db_copy_dialog: Option<crate::app::dialogs::db_copy::DbCopyState>,
    /// Pending "Parse in new tab" modal. Set when the user picks a scope
    /// from the Edit menu or right-click; cleared when the modal is
    /// dismissed (Cancel) or the parse succeeds (Open).
    pub(crate) pending_parse_modal: Option<crate::app::dialogs::parse_in_new_tab::ParseModalState>,
    /// Active Schema Export dialog target + window size, or `None` when
    /// the dialog isn't open. Switching targets while the dialog is up
    /// mutates `target` in place; closing the dialog clears the field.
    pub(crate) schema_export: Option<SchemaExportState>,
    /// Active Pivot / Unpivot dialog state, or `None` when closed. Operates on
    /// the active tab; running it builds a DuckDB PIVOT/UNPIVOT query and lands
    /// the result in a new detached tab (see `src/app/dialogs/pivot.rs`).
    pub(crate) pivot_dialog: Option<PivotState>,
    /// Active multi-column sort dialog state, or `None` when closed. Sorts the
    /// active tab in place (see `src/app/dialogs/multi_sort.rs`).
    pub(crate) multi_sort_dialog: Option<MultiSortState>,
    /// Active git revision-picker, or `None` when closed. Compares the active
    /// tab against a committed version (see `src/app/dialogs/git_compare.rs`).
    pub(crate) git_compare_dialog: Option<GitCompareState>,
    /// Active correlation-matrix dialog, or `None` when closed. Computes a
    /// correlation matrix into a detached tab (see `src/app/dialogs/correlation.rs`).
    pub(crate) correlation_dialog: Option<CorrelationState>,
    /// Active Transform-column dialog state, or `None` when closed. Reshapes
    /// the active tab in place (see `src/app/dialogs/transform.rs`).
    pub(crate) transform_dialog: Option<TransformState>,
    /// Active Conditional-column (CASE) dialog state, or `None` when closed.
    /// Builds a new column from an if / else-if / else rule chain over the
    /// active tab (see `src/app/dialogs/conditional_column.rs`).
    pub(crate) conditional_column_dialog: Option<ConditionalColumnState>,
    /// Active "Rename columns" dialog state, or `None` when closed.
    /// Bulk-renames columns of the active tab (see
    /// `src/app/dialogs/rename_columns.rs`).
    pub(crate) rename_columns_state: Option<RenameColumnsState>,
    /// Active "Random sample" dialog state, or `None` when closed
    /// (`src/app/dialogs/random_sample.rs`).
    pub(crate) random_sample_dialog: Option<RandomSampleState>,
    /// Active "Tidy up" dialog state, or `None` when closed
    /// (`src/app/dialogs/tidy_up.rs`).
    pub(crate) tidy_up_dialog: Option<TidyUpState>,
    /// Pending "name this bookmark" dialog, or `None` when closed. On save,
    /// pushes a session bookmark onto the active tab.
    pub(crate) bookmark_draft: Option<BookmarkDraft>,
    /// Pending "rename tab" dialog, or `None` when closed. On save, sets the
    /// target tab's `user_tab_name`.
    pub(crate) tab_rename_draft: Option<TabRenameDraft>,
    /// Active Anonymise-columns dialog state, or `None` when closed. Masks or
    /// scrambles chosen columns of the active tab (see
    /// `src/app/dialogs/anonymize.rs`).
    pub(crate) anonymize_dialog: Option<AnonymizeState>,
    /// Active Fill-missing-values (impute) dialog state, or `None` when
    /// closed. Fills null / empty cells in one column using the chosen strategy
    /// (see `src/app/dialogs/impute.rs`).
    pub(crate) impute_dialog: Option<ImputeState>,
    /// Active Drop-duplicate-rows dialog state, or `None` when closed. Removes
    /// duplicate rows from the active tab in place (see
    /// `src/app/dialogs/dedupe.rs`).
    pub(crate) dedupe_dialog: Option<DedupeState>,
    /// Active Detect-outliers dialog state, or `None` when closed. Flags
    /// numeric outlier cells in the active tab (see `src/app/dialogs/outliers.rs`).
    pub(crate) outlier_dialog: Option<OutlierState>,
    /// Active Detect-PII dialog state, or `None` when closed. Reports likely
    /// personal-data columns (see `src/app/dialogs/pii.rs`).
    pub(crate) pii_dialog: Option<PiiState>,
    /// Active Find-near-duplicates dialog state, or `None` when closed. Runs a
    /// fuzzy-duplicate scan on a worker thread (see
    /// `src/app/dialogs/find_fuzzy_duplicates.rs`).
    pub(crate) fuzzy_duplicates_dialog: Option<FuzzyDuplicatesState>,
    /// Active Partition-by-column dialog state, or `None` when closed. Writes
    /// one file per distinct column value into an output directory (see
    /// `src/app/dialogs/partition.rs`).
    pub(crate) partition_dialog: Option<PartitionState>,
    /// Active Union-tables dialog state, or `None` when closed. Stacks
    /// multiple open tabs row-by-row into a new reconciled tab (see
    /// `src/app/dialogs/union.rs`).
    pub(crate) union_dialog: Option<UnionState>,
    /// Active Join-tables dialog state, or `None` when closed. Joins
    /// multiple open tabs column-by-column on shared key columns, opening
    /// the result in a new tab (see `src/app/dialogs/join.rs`).
    pub(crate) join_dialog: Option<JoinState>,
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
    /// `ToggleReadOnly` shortcut (default F8). NOT persisted - every
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
    /// Multi-search panel state - query, scope, results, background
    /// worker. Initialised hidden; opened via **Search -> Multi-search...**
    /// or the `MultiSearch` keyboard shortcut.
    pub(crate) multi_search: super::multi_search::MultiSearchState,
    /// In-GUI chat assistant panel state (conversation, input, provider
    /// switching). Initialised hidden; opened via the toolbar Assistant
    /// button or the `ToggleChatPanel` shortcut.
    pub(crate) chat: super::chat_panel::ChatPanelState,
    /// Live-tab edits queued by the chat `edit_open_tab` tool, drained per frame.
    pub(crate) pending_tab_edits:
        std::sync::Arc<std::sync::Mutex<Vec<crate::mcp::tools::PendingTabEdit>>>,
    /// Sidebar cloud-storage browser: per-connection lazy listings + in-flight
    /// downloads + sign-in status, all driven by background workers. Hidden
    /// until toggled via **File -> Cloud connections**.
    pub(crate) cloud_browser: super::cloud_browser::CloudBrowserState,
    pub(crate) db_browser: super::db_browser::DbBrowserState,
    /// Live DB connectors reused across sidebar listings, table opens and
    /// server queries (cleared on Settings apply).
    pub(crate) db_conn_cache: super::db_conn_cache::DbConnCache,
    /// In-flight "Run on server" SQL query, if any (one at a time).
    pub(crate) sql_server_job: Option<super::sql_panel::SqlServerJob>,
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
