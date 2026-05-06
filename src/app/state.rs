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

/// Sort order for the Column Inspector dialog. View-only — does not mutate
/// the underlying column order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ColumnInspectorSort {
    #[default]
    Default,
    Asc,
    Desc,
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
    pub(crate) pdf_page_images: Vec<egui::ColorImage>,
    pub(crate) pdf_textures: Vec<egui::TextureHandle>,
    pub(crate) pdf_page_texts: Vec<String>,
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
    pub(crate) commonmark_cache: egui_commonmark::CommonMarkCache,
    /// Pending vertical scroll offset for the markdown view's ScrollArea —
    /// set when the user clicks a `#fragment` link, applied next frame.
    pub(crate) markdown_scroll_target: Option<f32>,
    pub(crate) json_tree_expanded: std::collections::HashSet<String>,
    pub(crate) json_value: Option<serde_json::Value>,
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
    pub(crate) show_add_column_dialog: bool,
    pub(crate) new_col_name: String,
    pub(crate) new_col_type: String,
    pub(crate) new_col_formula: String,
    pub(crate) insert_col_at: Option<usize>,
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
    /// Set to true when this tab represents an empty (0-byte) file. Renders
    /// the easter-egg ASCII art instead of the table view.
    pub(crate) empty_file_placeholder: bool,
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
}
