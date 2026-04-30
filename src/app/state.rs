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
    pub(crate) pdf_page_images: Vec<egui::ColorImage>,
    pub(crate) pdf_textures: Vec<egui::TextureHandle>,
    pub(crate) pdf_page_texts: Vec<String>,
    pub(crate) raw_view_formatted: bool,
    pub(crate) csv_delimiter: u8,
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
    /// File path passed via command line (loaded on first frame)
    pub(crate) initial_file: Option<std::path::PathBuf>,
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
    /// Currently opened directory tree sidebar (`None` = sidebar hidden).
    pub(crate) directory_tree: Option<ui::directory_tree::DirectoryTreeState>,
}
