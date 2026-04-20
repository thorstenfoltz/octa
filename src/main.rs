#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod view_modes;

use octa::data::search::RowMatcher;
use octa::data::{self, DataTable, ViewMode};
use octa::formats::{self, FormatRegistry};
use octa::ui;
use ui::settings::{AppSettings, SettingsDialog};
use ui::table_view::TableViewState;
use ui::theme::ThemeMode;

use eframe::egui;
use egui::{Color32, RichText};

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

fn render_icon(svg_source: &str) -> egui::IconData {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg_source, &opt).expect("Failed to parse SVG");
    let icon_size = 256u32;
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(icon_size, icon_size).expect("Failed to create pixmap");
    let size = tree.size();
    let sx = icon_size as f32 / size.width();
    let sy = icon_size as f32 / size.height();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(sx, sy),
        &mut pixmap.as_mut(),
    );
    egui::IconData {
        rgba: pixmap.data().to_vec(),
        width: icon_size,
        height: icon_size,
    }
}

const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

fn main() -> eframe::Result<()> {
    // Handle CLI flags before launching GUI
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("octa {}", VERSION);
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!(
                    "octa {} - A modular multi-format data viewer and editor",
                    VERSION
                );
                println!();
                println!("Usage: octa [OPTIONS] [FILE]");
                println!();
                println!("Arguments:");
                println!("  [FILE]  File to open on startup");
                println!();
                println!("Options:");
                println!("  -V, --version  Print version");
                println!("  -h, --help     Print help");
                println!();
                println!("Author:  {}", AUTHORS);
                println!("Repo:    {}", REPOSITORY);
                std::process::exit(0);
            }
            _ => {}
        }
    }

    // Parse CLI arguments: octa [file_path]
    let initial_file = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists());

    let title = match &initial_file {
        Some(p) => format!(
            "Octa - {}",
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        ),
        None => "Octa".to_string(),
    };

    let settings = AppSettings::load();
    let icon_svg = settings.icon_variant.svg_source();
    let icon = render_icon(icon_svg);
    let default_theme = settings.default_theme;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(settings.window_size.dimensions())
            .with_min_inner_size([800.0, 600.0])
            .with_maximized(true)
            .with_title(&title)
            .with_icon(Arc::new(icon)),
        ..Default::default()
    };

    eframe::run_native(
        "octa",
        options,
        Box::new(move |cc| {
            ui::theme::apply_theme(
                &cc.egui_ctx,
                default_theme,
                ui::theme::FontSettings {
                    size: settings.font_size,
                    body: settings.body_font,
                    custom_path: Some(settings.custom_font_path.as_str()),
                },
            );
            Ok(Box::new(OctaApp::new(initial_file, settings)))
        }),
    )
}

#[derive(Clone)]
enum UpdateState {
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
    /// Update completed successfully
    Updated(String),
    /// An error occurred
    Error(String),
}

struct TabState {
    table: DataTable,
    table_state: TableViewState,
    search_text: String,
    search_mode: data::SearchMode,
    show_replace_bar: bool,
    replace_text: String,
    filtered_rows: Vec<usize>,
    filter_dirty: bool,
    view_mode: ViewMode,
    raw_content: Option<String>,
    raw_content_modified: bool,
    pdf_page_images: Vec<egui::ColorImage>,
    pdf_textures: Vec<egui::TextureHandle>,
    pdf_page_texts: Vec<String>,
    raw_view_formatted: bool,
    csv_delimiter: u8,
    bg_row_buffer: Option<Arc<Mutex<Vec<Vec<data::CellValue>>>>>,
    bg_loading_done: Arc<std::sync::atomic::AtomicBool>,
    bg_can_load_more: bool,
    bg_file_exhausted: Arc<std::sync::atomic::AtomicBool>,
    commonmark_cache: egui_commonmark::CommonMarkCache,
    json_tree_expanded: std::collections::HashSet<String>,
    json_value: Option<serde_json::Value>,
    json_expand_depth: usize,
    json_expand_depth_str: String,
    json_edit_path: Option<String>,
    json_edit_buffer: String,
    show_add_column_dialog: bool,
    new_col_name: String,
    new_col_type: String,
    new_col_formula: String,
    insert_col_at: Option<usize>,
    show_delete_columns_dialog: bool,
    delete_col_selection: Vec<bool>,
    sql_query: String,
    sql_result: Option<DataTable>,
    sql_error: Option<String>,
    /// Whether the SQL panel is currently visible alongside the table view.
    sql_panel_open: bool,
    /// Autocomplete popup: currently highlighted suggestion index (clamped
    /// to the live suggestion list each frame).
    sql_ac_selected: usize,
    /// Autocomplete popup: set to `false` by Escape to hide the popup until
    /// the user types again. Reset to `true` on any text change.
    sql_ac_visible: bool,
}

impl TabState {
    fn new(search_mode: data::SearchMode) -> Self {
        Self {
            table: DataTable::empty(),
            table_state: TableViewState::default(),
            search_text: String::new(),
            search_mode,
            show_replace_bar: false,
            replace_text: String::new(),
            filtered_rows: Vec::new(),
            filter_dirty: true,
            view_mode: ViewMode::Table,
            raw_content: None,
            raw_content_modified: false,
            pdf_page_images: Vec::new(),
            pdf_textures: Vec::new(),
            pdf_page_texts: Vec::new(),
            raw_view_formatted: false,
            csv_delimiter: b',',
            bg_row_buffer: None,
            bg_loading_done: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            bg_can_load_more: false,
            bg_file_exhausted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            commonmark_cache: egui_commonmark::CommonMarkCache::default(),
            json_tree_expanded: std::collections::HashSet::new(),
            json_value: None,
            json_expand_depth: 1,
            json_expand_depth_str: "1".to_string(),
            json_edit_path: None,
            json_edit_buffer: String::new(),
            show_add_column_dialog: false,
            new_col_name: String::new(),
            new_col_type: "String".to_string(),
            new_col_formula: String::new(),
            insert_col_at: None,
            show_delete_columns_dialog: false,
            delete_col_selection: Vec::new(),
            sql_query: String::new(),
            sql_result: None,
            sql_error: None,
            sql_panel_open: false,
            sql_ac_selected: 0,
            sql_ac_visible: true,
        }
    }

    fn is_modified(&self) -> bool {
        self.table.is_modified() || self.raw_content_modified
    }

    fn title_display(&self) -> String {
        let name = if let Some(ref path) = self.table.source_path {
            std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string())
        } else {
            "Untitled".to_string()
        };
        if self.is_modified() {
            format!("{} *", name)
        } else {
            name
        }
    }
}

struct OctaApp {
    tabs: Vec<TabState>,
    active_tab: usize,
    pending_close_tab: Option<usize>,
    registry: FormatRegistry,
    theme_mode: ThemeMode,
    settings: AppSettings,
    settings_dialog: SettingsDialog,
    /// Whether the search text field should be focused next frame.
    search_focus_requested: bool,
    /// "Unsaved changes" dialog state
    show_close_confirm: bool,
    /// Whether we already decided to quit (skip further confirm)
    confirmed_close: bool,
    /// System clipboard handle (shared, lazily initialized)
    os_clipboard: Option<Arc<Mutex<arboard::Clipboard>>>,
    /// Logo texture for toolbar (small, native SVG size)
    logo_texture: Option<egui::TextureHandle>,
    /// Logo texture for welcome screen (large, rendered from SVG at high resolution)
    welcome_logo_texture: Option<egui::TextureHandle>,
    /// File path passed via command line (loaded on first frame)
    initial_file: Option<std::path::PathBuf>,
    /// Pending file to open after unsaved-changes dialog resolves
    pending_open_file: bool,
    /// Show unsaved-changes dialog before opening a new file
    show_open_confirm: bool,
    /// Show the About dialog
    show_about_dialog: bool,
    /// Show the Documentation dialog
    show_documentation_dialog: bool,
    /// Show the Update dialog
    show_update_dialog: bool,
    /// Confirm before reloading the raw CSV/TSV file when un-aligning columns.
    show_unalign_confirm: bool,
    /// Update check state shared with background thread
    update_state: Arc<Mutex<UpdateState>>,
    status_message: Option<(String, std::time::Instant)>,
    /// Recently opened file paths (most recent first).
    recent_files: Vec<String>,
    /// Zoom level in percent (100 = default, steps of 5).
    zoom_percent: u32,
    /// Status bar navigation input buffer.
    nav_input: String,
    /// Focus the status-bar navigation input next frame (Ctrl+G / Go To Cell).
    nav_focus_requested: bool,
    /// Confirm before reloading the file from disk and losing unsaved edits.
    show_reload_confirm: bool,
    /// Pending modal table picker (DB sources containing multiple tables).
    pending_table_picker: Option<ui::table_picker::TablePickerState>,
    /// Currently opened directory tree sidebar (`None` = sidebar hidden).
    directory_tree: Option<ui::directory_tree::DirectoryTreeState>,
}

/// Detect delimiter from a file by reading only the first few KB.
/// Shift cell references in a formula to target a specific row.
/// The formula is written as a template using row 1 (e.g. "A1+B1").
/// For `target_row=4` (0-indexed), references are shifted so row 1 -> row 5 (1-indexed).
/// References that already use a different row number are shifted by the same offset.
fn shift_formula_row(formula: &str, target_row: usize) -> String {
    let chars: Vec<char> = formula.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_alphabetic() {
            // Collect the column letters
            let col_start = i;
            while i < chars.len() && chars[i].is_ascii_alphabetic() {
                i += 1;
            }
            // Check if followed by digits (a cell reference)
            if i < chars.len() && chars[i].is_ascii_digit() {
                let col_part: String = chars[col_start..i].iter().collect();
                let num_start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let num_str: String = chars[num_start..i].iter().collect();
                if let Ok(orig_row) = num_str.parse::<usize>() {
                    // Compute offset from row 1 template, apply to target
                    let new_row = target_row + orig_row; // orig_row is 1-based, target_row is 0-based
                    result.push_str(&col_part);
                    result.push_str(&new_row.to_string());
                } else {
                    result.push_str(&col_part);
                    result.push_str(&num_str);
                }
            } else {
                // Not a cell ref, just letters
                let part: String = chars[col_start..i].iter().collect();
                result.push_str(&part);
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

fn detect_delimiter_from_file(path: &std::path::Path) -> u8 {
    use std::io::Read;
    let mut buf = vec![0u8; 1_048_576]; // 1 MB
    let content = match std::fs::File::open(path) {
        Ok(mut f) => match f.read(&mut buf) {
            Ok(n) => String::from_utf8_lossy(&buf[..n]).to_string(),
            Err(_) => return b',',
        },
        Err(_) => return b',',
    };
    detect_delimiter_from_content(&content)
}

/// Detect delimiter from file content (same logic as csv_reader but operates on a string).
fn detect_delimiter_from_content(content: &str) -> u8 {
    let lines: Vec<&str> = content.lines().take(20).collect();
    if lines.is_empty() {
        return b',';
    }
    let candidates: &[u8] = b",;|\t";
    let mut best: Option<(u8, usize)> = None;
    for &delim in candidates {
        let delim_char = delim as char;
        let counts: Vec<usize> = lines
            .iter()
            .map(|l| l.matches(delim_char).count())
            .collect();
        if counts[0] == 0 {
            continue;
        }
        let header_count = counts[0];
        let consistent = counts.iter().all(|&c| c == header_count || c == 0);
        if consistent && (best.is_none() || header_count > best.unwrap().1) {
            best = Some((delim, header_count));
        }
    }
    best.map(|(d, _)| d).unwrap_or(b',')
}

/// Format delimited text by aligning columns with spaces.
/// Background-load remaining Parquet rows after the initial batch.
/// Writes batches of rows into the shared buffer, which the UI thread drains.
fn load_remaining_parquet_rows(
    path: &std::path::Path,
    skip_rows: usize,
    max_rows: usize,
    buffer: Arc<Mutex<Vec<Vec<data::CellValue>>>>,
    done: Arc<std::sync::atomic::AtomicBool>,
    exhausted: Arc<std::sync::atomic::AtomicBool>,
) -> anyhow::Result<()> {
    use formats::parquet_reader::arrow_value_to_cell;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    let file = std::fs::File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.with_batch_size(8192).build()?;

    let mut skipped = 0usize;
    let mut loaded = 0usize;
    let flush_threshold = 50_000;

    let mut batch_buf = Vec::with_capacity(flush_threshold);

    'outer: for batch_result in reader {
        let batch = batch_result?;
        let num_rows = batch.num_rows();
        let num_cols = batch.num_columns();

        for row_idx in 0..num_rows {
            if skipped < skip_rows {
                skipped += 1;
                continue;
            }
            if loaded >= max_rows {
                break 'outer;
            }
            let mut row = Vec::with_capacity(num_cols);
            for col_idx in 0..num_cols {
                let array = batch.column(col_idx);
                row.push(arrow_value_to_cell(array, row_idx));
            }
            batch_buf.push(row);
            loaded += 1;

            if batch_buf.len() >= flush_threshold {
                if let Ok(mut buf) = buffer.lock() {
                    buf.append(&mut batch_buf);
                }
                batch_buf = Vec::with_capacity(flush_threshold);
            }
        }
    }

    // Flush remaining
    if !batch_buf.is_empty() {
        if let Ok(mut buf) = buffer.lock() {
            buf.append(&mut batch_buf);
        }
    }

    if loaded < max_rows {
        exhausted.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

const COLUMN_TYPES: &[&str] = &[
    "String",
    "Int64",
    "Float64",
    "Boolean",
    "Date32",
    "Timestamp(Microsecond, None)",
];

impl OctaApp {
    fn new(initial_file: Option<std::path::PathBuf>, settings: AppSettings) -> Self {
        let theme_mode = settings.default_theme;
        let search_mode = settings.default_search_mode;
        let recent_files = Self::load_recent_files();
        Self {
            tabs: vec![TabState::new(search_mode)],
            active_tab: 0,
            pending_close_tab: None,
            registry: FormatRegistry::new(),
            theme_mode,
            settings,
            settings_dialog: SettingsDialog::default(),
            search_focus_requested: false,
            show_close_confirm: false,
            confirmed_close: false,
            os_clipboard: arboard::Clipboard::new()
                .ok()
                .map(|c| Arc::new(Mutex::new(c))),
            logo_texture: None,
            welcome_logo_texture: None,
            initial_file,
            pending_open_file: false,
            show_open_confirm: false,
            show_about_dialog: false,
            show_documentation_dialog: false,
            show_update_dialog: false,
            show_unalign_confirm: false,
            update_state: Arc::new(Mutex::new(UpdateState::Idle)),
            status_message: None,
            recent_files,
            zoom_percent: 100,
            nav_input: String::new(),
            nav_focus_requested: false,
            show_reload_confirm: false,
            pending_table_picker: None,
            directory_tree: None,
        }
    }

    fn recent_files_path() -> Option<std::path::PathBuf> {
        AppSettings::config_dir().map(|d| d.join("recent.toml"))
    }

    fn load_recent_files() -> Vec<String> {
        #[derive(Deserialize)]
        struct RecentData {
            #[serde(default)]
            files: Vec<String>,
        }
        Self::recent_files_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str::<RecentData>(&s).ok())
            .map(|d| d.files)
            .unwrap_or_default()
    }

    fn save_recent_files(&self) {
        #[derive(Serialize)]
        struct RecentData<'a> {
            files: &'a [String],
        }
        if let Some(path) = Self::recent_files_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(contents) = toml::to_string_pretty(&RecentData {
                files: &self.recent_files,
            }) {
                let _ = std::fs::write(path, contents);
            }
        }
    }

    fn add_recent_file(&mut self, file_path: &str) {
        let canonical = std::fs::canonicalize(file_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| file_path.to_string());
        self.recent_files.retain(|p| p != &canonical);
        self.recent_files.insert(0, canonical);
        let max = self.settings.max_recent_files;
        self.recent_files.truncate(max);
        self.save_recent_files();
    }

    fn close_tab(&mut self, idx: usize) {
        self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.tabs
                .push(TabState::new(self.settings.default_search_mode));
        }
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    /// Build a tab-separated string from the current selection.
    /// Priority: selected_rows > selected_cols > selected_cell.
    fn copy_selection_to_string(&self) -> Option<String> {
        let tab = &self.tabs[self.active_tab];
        let state = &tab.table_state;

        if !state.selected_rows.is_empty() {
            // Copy selected rows (all columns)
            let mut rows: Vec<usize> = state.selected_rows.iter().copied().collect();
            rows.sort();
            let mut lines = Vec::new();
            for row in rows {
                let mut cells = Vec::new();
                for col in 0..tab.table.col_count() {
                    let text = tab
                        .table
                        .get(row, col)
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    cells.push(text);
                }
                lines.push(cells.join("\t"));
            }
            Some(lines.join("\n"))
        } else if !state.selected_cols.is_empty() {
            // Copy selected columns (all rows)
            let mut cols: Vec<usize> = state.selected_cols.iter().copied().collect();
            cols.sort();
            let mut lines = Vec::new();
            for row in 0..tab.table.row_count() {
                let mut cells = Vec::new();
                for &col in &cols {
                    let text = tab
                        .table
                        .get(row, col)
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    cells.push(text);
                }
                lines.push(cells.join("\t"));
            }
            Some(lines.join("\n"))
        } else if let Some((row, col)) = state.selected_cell {
            // Copy single cell
            let text = tab
                .table
                .get(row, col)
                .map(|v| v.to_string())
                .unwrap_or_default();
            Some(text)
        } else {
            None
        }
    }

    /// Paste tab-separated text into the table at the current selection.
    fn paste_text_into_table(&mut self, text: &str) {
        let parsed_rows: Vec<Vec<&str>> = text
            .lines()
            .map(|line| line.split('\t').collect())
            .collect();
        if parsed_rows.is_empty() {
            return;
        }

        let tab = &mut self.tabs[self.active_tab];
        let (start_row, start_col) = tab.table_state.selected_cell.unwrap_or((0, 0));

        for (ri, row_cells) in parsed_rows.iter().enumerate() {
            let target_row = start_row + ri;
            if target_row >= tab.table.row_count() {
                break;
            }
            for (ci, &cell_text) in row_cells.iter().enumerate() {
                let target_col = start_col + ci;
                if target_col >= tab.table.col_count() {
                    break;
                }
                if let Some(existing) = tab.table.get(target_row, target_col).cloned() {
                    let new_val = data::CellValue::parse_like(&existing, cell_text);
                    tab.table.set(target_row, target_col, new_val);
                }
            }
        }
        tab.filter_dirty = true;
    }

    /// Copy selection to both internal and OS clipboard.
    fn do_copy(&mut self) {
        if let Some(text) = self.copy_selection_to_string() {
            self.tabs[self.active_tab].table_state.clipboard = Some(text.clone());
            if let Some(ref cb) = self.os_clipboard {
                if let Ok(mut cb) = cb.lock() {
                    let _ = cb.set_text(&text);
                }
            }
        }
    }

    /// Paste from OS clipboard (preferred) or internal clipboard.
    fn do_paste(&mut self, paste_event_text: Option<String>) {
        // Priority: paste_event_text (from egui Paste event) > OS clipboard > internal clipboard
        let text = if let Some(t) = paste_event_text {
            Some(t)
        } else if let Some(ref cb) = self.os_clipboard {
            cb.lock().ok().and_then(|mut cb| cb.get_text().ok())
        } else {
            self.tabs[self.active_tab].table_state.clipboard.clone()
        };

        if let Some(text) = text {
            if !text.is_empty() {
                self.paste_text_into_table(&text);
            }
        }
    }

    /// Check if the OS clipboard has text content.
    fn os_clipboard_has_text(&self) -> bool {
        if let Some(ref cb) = self.os_clipboard {
            if let Ok(mut cb) = cb.lock() {
                return cb.get_text().map(|t| !t.is_empty()).unwrap_or(false);
            }
        }
        false
    }

    fn apply_zoom(&self, ctx: &egui::Context) {
        let base_font_size = self.settings.font_size;
        let effective_font_size = base_font_size * self.zoom_percent as f32 / 100.0;
        ui::theme::apply_theme(
            ctx,
            self.theme_mode,
            ui::theme::FontSettings {
                size: effective_font_size,
                body: self.settings.body_font,
                custom_path: Some(self.settings.custom_font_path.as_str()),
            },
        );
    }

    fn open_file(&mut self) {
        self.do_open_file_dialog();
    }

    fn do_open_file_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new();

        // Add "All Supported" filter first
        let all_exts = self.registry.all_extensions();
        let all_ext_refs: Vec<&str> = all_exts.iter().map(|s| s.as_str()).collect();
        dialog = dialog.add_filter("All Supported", &all_ext_refs);

        for (name, exts) in self.registry.format_descriptions() {
            let ext_refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter(&name, &ext_refs);
        }
        dialog = dialog.add_filter("All Files", &["*"]);

        if let Some(path) = dialog.pick_file() {
            self.load_file(path);
        }
    }

    fn load_file(&mut self, path: std::path::PathBuf) {
        let reader = match self.registry.reader_for_path(&path) {
            Some(r) => r,
            None => {
                self.status_message = Some((
                    format!(
                        "No reader available for: {}",
                        path.extension()
                            .map(|e| e.to_string_lossy().to_string())
                            .unwrap_or_default()
                    ),
                    std::time::Instant::now(),
                ));
                return;
            }
        };

        // Multi-table sources (DuckDB / SQLite): show picker if >1 table.
        match reader.list_tables(&path) {
            Ok(Some(tables)) if tables.len() > 1 => {
                self.pending_table_picker = Some(ui::table_picker::TablePickerState {
                    path,
                    format_name: reader.name().to_string(),
                    tables,
                    selected: 0,
                });
                return;
            }
            Ok(Some(tables)) if tables.len() == 1 => {
                let name = tables[0].name.clone();
                match reader.read_table(&path, &name) {
                    Ok(table) => self.apply_loaded_table(path, table),
                    Err(e) => {
                        self.status_message = Some((
                            format!("Error reading table: {e}"),
                            std::time::Instant::now(),
                        ));
                    }
                }
                return;
            }
            Ok(Some(_)) => {
                // Empty list — fall through to read_file which will yield a clear error.
            }
            Ok(None) => {}
            Err(e) => {
                self.status_message = Some((
                    format!("Error inspecting file: {e}"),
                    std::time::Instant::now(),
                ));
                return;
            }
        }

        match reader.read_file(&path) {
            Ok(table) => self.apply_loaded_table(path, table),
            Err(e) => {
                self.status_message = Some((
                    format!("Error reading file: {}", e),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    /// Load a specific named table from a DB-style multi-table source.
    fn load_table(&mut self, path: std::path::PathBuf, table_name: String) {
        let reader = match self.registry.reader_for_path(&path) {
            Some(r) => r,
            None => return,
        };
        match reader.read_table(&path, &table_name) {
            Ok(table) => self.apply_loaded_table(path, table),
            Err(e) => {
                self.status_message = Some((
                    format!("Error reading table '{table_name}': {e}"),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    /// Wire a freshly-loaded `DataTable` into a tab and run all the post-load
    /// setup (raw-content load, view-mode pick, recent-files update, etc.).
    fn apply_loaded_table(&mut self, path: std::path::PathBuf, table: DataTable) {
        // Decide whether to load into current tab or create a new one
        let current_empty = self.tabs[self.active_tab].table.col_count() == 0
            && !self.tabs[self.active_tab].is_modified();
        if !current_empty {
            let new_tab = TabState::new(self.settings.default_search_mode);
            self.tabs.push(new_tab);
            self.active_tab = self.tabs.len() - 1;
        }

        {
            let tab = &mut self.tabs[self.active_tab];
            tab.table = table;
            tab.table_state = TableViewState::default();
            tab.search_text.clear();
            tab.filter_dirty = true;
            // Set up on-demand loading state for truncated files
            if tab.table.total_rows.is_some() {
                let loaded = tab.table.row_count();
                self.status_message = Some((
                    format!(
                        "Loaded {} rows (scroll down to load more)",
                        ui::status_bar::format_number(loaded)
                    ),
                    std::time::Instant::now(),
                ));
                tab.bg_can_load_more = true;
                tab.bg_row_buffer = None;
                tab.bg_loading_done
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                tab.bg_file_exhausted
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            } else {
                self.status_message = None;
                tab.bg_row_buffer = None;
                tab.bg_loading_done
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                tab.bg_can_load_more = false;
                tab.bg_file_exhausted
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }
            tab.raw_view_formatted = false;

            // Detect and store CSV delimiter (read only first few KB)
            if tab.table.format_name.as_deref() == Some("CSV") {
                tab.csv_delimiter = detect_delimiter_from_file(&path);
            } else if tab.table.format_name.as_deref() == Some("TSV") {
                tab.csv_delimiter = b'\t';
            }

            // Load raw content for text-based formats (skip for large files)
            let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if file_size <= 500_000_000 {
                // 500 MB
                tab.raw_content = std::fs::read_to_string(&path).ok();
            } else {
                tab.raw_content = None;
            }
            tab.raw_content_modified = false;

            // For PDFs, render pages visually and default to Pdf view
            tab.pdf_page_images.clear();
            tab.pdf_textures.clear();
            tab.pdf_page_texts.clear();
            if tab.table.format_name.as_deref() == Some("PDF") {
                match formats::pdf_reader::render_pdf_pages(&path, 2.0) {
                    Ok((images, texts)) => {
                        tab.pdf_page_images = images;
                        tab.pdf_page_texts = texts;
                        tab.view_mode = ViewMode::Pdf;
                    }
                    Err(_) => {
                        tab.view_mode = ViewMode::Table;
                    }
                }
            } else if tab.table.format_name.as_deref() == Some("Markdown") {
                tab.view_mode = ViewMode::Markdown;
            } else if tab.table.format_name.as_deref() == Some("Jupyter Notebook") {
                tab.view_mode = ViewMode::Notebook;
            } else if tab.table.format_name.as_deref() == Some("Text") {
                tab.view_mode = ViewMode::Raw;
            } else {
                tab.view_mode = ViewMode::Table;
            }

            // Default SQL panel open; query starts empty so the placeholder
            // hint is shown. Panel is only meaningful in Table view.
            tab.sql_query.clear();
            tab.sql_result = None;
            tab.sql_error = None;
            tab.sql_panel_open =
                self.settings.sql_panel_default_open && tab.view_mode == ViewMode::Table;

            // Parse JSON for tree view (JSON and JSONL formats)
            tab.json_value = None;
            tab.json_tree_expanded.clear();
            if matches!(
                tab.table.format_name.as_deref(),
                Some("JSON") | Some("JSONL")
            ) {
                if let Some(ref content) = tab.raw_content {
                    tab.json_value = serde_json::from_str(content).ok();
                }
            }
            // Set expand depth to file's max depth
            tab.json_expand_depth = tab
                .json_value
                .as_ref()
                .map(octa::data::json_util::max_json_depth)
                .unwrap_or(0);
            tab.json_expand_depth_str = tab.json_expand_depth.to_string();

            // Track in recent files
            self.add_recent_file(&path.to_string_lossy());
        }
    }

    fn save_file(&mut self) {
        if let Some(ref path) = self.tabs[self.active_tab].table.source_path.clone() {
            let path = std::path::Path::new(path);
            self.do_save(path.to_path_buf());
        }
    }

    /// Apply a text transformation to every selected cell. The target cells are
    /// the intersection of `selected_rows` × `selected_cols` when both are set;
    /// otherwise every cell in the selected rows or columns; otherwise the
    /// single selected cell. Non-string cells are skipped.
    fn transform_selected_cells(&mut self, transform: fn(&str) -> String) {
        let tab = &mut self.tabs[self.active_tab];
        if tab.table.col_count() == 0 {
            return;
        }
        let state = &tab.table_state;
        let row_count = tab.table.row_count();
        let col_count = tab.table.col_count();

        let targets: Vec<(usize, usize)> = match (
            !state.selected_rows.is_empty(),
            !state.selected_cols.is_empty(),
            state.selected_cell,
        ) {
            (true, true, _) => state
                .selected_rows
                .iter()
                .flat_map(|&r| state.selected_cols.iter().map(move |&c| (r, c)))
                .collect(),
            (true, false, _) => state
                .selected_rows
                .iter()
                .flat_map(|&r| (0..col_count).map(move |c| (r, c)))
                .collect(),
            (false, true, _) => state
                .selected_cols
                .iter()
                .flat_map(|&c| (0..row_count).map(move |r| (r, c)))
                .collect(),
            (false, false, Some(rc)) => vec![rc],
            _ => Vec::new(),
        };

        for (r, c) in targets {
            if r >= row_count || c >= col_count {
                continue;
            }
            if let Some(cv) = tab.table.get(r, c).cloned() {
                if let data::CellValue::String(s) = cv {
                    let new_val = transform(&s);
                    if new_val != s {
                        tab.table.set(r, c, data::CellValue::String(new_val));
                    }
                }
            }
        }
    }

    fn duplicate_selected_rows(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        if tab.table.col_count() == 0 {
            return;
        }
        let mut rows: Vec<usize> = if !tab.table_state.selected_rows.is_empty() {
            tab.table_state.selected_rows.iter().copied().collect()
        } else if let Some((r, _)) = tab.table_state.selected_cell {
            vec![r]
        } else {
            return;
        };
        // Insert from highest index to lowest so earlier insertions don't shift later targets.
        rows.sort_unstable_by(|a, b| b.cmp(a));
        let col_count = tab.table.col_count();
        for r in rows {
            if r >= tab.table.row_count() {
                continue;
            }
            let values: Vec<data::CellValue> = (0..col_count)
                .map(|c| {
                    tab.table
                        .get(r, c)
                        .cloned()
                        .unwrap_or(data::CellValue::Null)
                })
                .collect();
            tab.table.insert_row(r + 1);
            for (c, v) in values.into_iter().enumerate() {
                tab.table.set(r + 1, c, v);
            }
        }
        tab.filter_dirty = true;
    }

    fn delete_selected_rows(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        if tab.table.col_count() == 0 || tab.table.row_count() == 0 {
            return;
        }
        let mut rows: Vec<usize> = if !tab.table_state.selected_rows.is_empty() {
            tab.table_state.selected_rows.iter().copied().collect()
        } else if let Some((r, _)) = tab.table_state.selected_cell {
            vec![r]
        } else {
            return;
        };
        rows.sort_unstable_by(|a, b| b.cmp(a));
        for r in rows {
            if r < tab.table.row_count() {
                tab.table.delete_row(r);
            }
        }
        tab.table_state.selected_rows.clear();
        tab.table_state.selected_cell = None;
        tab.filter_dirty = true;
    }

    fn reload_active_file(&mut self) {
        let Some(path) = self.tabs[self.active_tab].table.source_path.clone() else {
            return;
        };
        let tab = &mut self.tabs[self.active_tab];
        tab.table.discard_edits();
        tab.table.clear_modified();
        tab.raw_content_modified = false;
        self.load_file(std::path::PathBuf::from(path));
    }

    fn cycle_view_mode(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let has_raw = tab.raw_content.is_some();
        let has_markdown = tab.table.format_name.as_deref() == Some("Markdown");
        let has_notebook = tab.table.format_name.as_deref() == Some("Jupyter Notebook");
        let has_pdf = !tab.pdf_page_images.is_empty();
        let has_json = tab.json_value.is_some();
        let has_table = tab.table.col_count() > 0 && !has_notebook;

        let mut modes: Vec<ViewMode> = Vec::new();
        if has_table {
            modes.push(ViewMode::Table);
        }
        if has_raw {
            modes.push(ViewMode::Raw);
        }
        if has_markdown {
            modes.push(ViewMode::Markdown);
        }
        if has_notebook {
            modes.push(ViewMode::Notebook);
        }
        if has_pdf {
            modes.push(ViewMode::Pdf);
        }
        if has_json {
            modes.push(ViewMode::JsonTree);
        }
        if modes.len() < 2 {
            return;
        }
        let current_idx = modes.iter().position(|&m| m == tab.view_mode).unwrap_or(0);
        let next = modes[(current_idx + 1) % modes.len()];
        tab.view_mode = next;
    }

    fn save_file_as(&mut self) {
        let mut dialog = rfd::FileDialog::new();
        for (label, exts) in self.registry.save_format_descriptions() {
            let ext_refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter(&label, &ext_refs);
        }
        if let Some(ref source) = self.tabs[self.active_tab].table.source_path {
            if let Some(name) = std::path::Path::new(source).file_name() {
                dialog = dialog.set_file_name(name.to_string_lossy().to_string());
            }
        }

        if let Some(path) = dialog.save_file() {
            self.do_save(path);
        }
    }

    fn export_sql_result(&mut self) {
        let Some(result) = self.tabs[self.active_tab].sql_result.clone() else {
            return;
        };
        if result.col_count() == 0 {
            return;
        }

        let mut dialog = rfd::FileDialog::new();
        for (label, exts) in self.registry.save_format_descriptions() {
            let ext_refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter(&label, &ext_refs);
        }
        dialog = dialog.set_file_name("sql_result.csv");

        let Some(path) = dialog.save_file() else {
            return;
        };

        match self.registry.reader_for_path(&path) {
            Some(reader) if reader.supports_write() => match reader.write_file(&path, &result) {
                Ok(()) => {
                    self.status_message = Some((
                        format!("Exported to {}", path.display()),
                        std::time::Instant::now(),
                    ));
                }
                Err(e) => {
                    self.status_message =
                        Some((format!("Error exporting: {e}"), std::time::Instant::now()));
                }
            },
            Some(reader) => {
                self.status_message = Some((
                    format!("Writing is not supported for {} format", reader.name()),
                    std::time::Instant::now(),
                ));
            }
            None => {
                self.status_message = Some((
                    format!(
                        "No writer available for extension: {}",
                        path.extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("(none)")
                    ),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    fn save_tab(&mut self, tab_idx: usize) {
        if let Some(ref path) = self.tabs[tab_idx].table.source_path.clone() {
            let path = std::path::Path::new(path);
            self.do_save_tab(tab_idx, path.to_path_buf());
        }
    }

    fn do_save(&mut self, path: std::path::PathBuf) {
        self.do_save_tab(self.active_tab, path);
    }

    fn do_save_tab(&mut self, tab_idx: usize, path: std::path::PathBuf) {
        let tab = &mut self.tabs[tab_idx];
        // If raw content was modified, write it directly to the file
        if tab.raw_content_modified {
            if let Some(ref content) = tab.raw_content {
                match std::fs::write(&path, content) {
                    Ok(()) => {
                        tab.table.source_path = Some(path.to_string_lossy().to_string());
                        tab.raw_content_modified = false;
                        self.status_message = Some((
                            format!("Saved to {}", path.display()),
                            std::time::Instant::now(),
                        ));
                    }
                    Err(e) => {
                        self.status_message = Some((
                            format!("Error saving file: {}", e),
                            std::time::Instant::now(),
                        ));
                    }
                }
                return;
            }
        }

        // For CSV files with a custom delimiter, use write_delimited directly
        if tab.table.format_name.as_deref() == Some("CSV") && tab.csv_delimiter != b',' {
            tab.table.apply_edits();
            match formats::csv_reader::write_delimited(&path, tab.csv_delimiter, &tab.table) {
                Ok(()) => {
                    tab.table.source_path = Some(path.to_string_lossy().to_string());
                    tab.table.clear_modified();
                    self.status_message = Some((
                        format!("Saved to {}", path.display()),
                        std::time::Instant::now(),
                    ));
                }
                Err(e) => {
                    self.status_message = Some((
                        format!("Error saving file: {}", e),
                        std::time::Instant::now(),
                    ));
                }
            }
            return;
        }

        match self.registry.reader_for_path(&path) {
            Some(reader) => {
                if !reader.supports_write() {
                    self.status_message = Some((
                        format!("Writing is not supported for {} format", reader.name()),
                        std::time::Instant::now(),
                    ));
                    return;
                }
                let tab = &mut self.tabs[tab_idx];
                tab.table.apply_edits();
                match reader.write_file(&path, &tab.table) {
                    Ok(()) => {
                        tab.table.source_path = Some(path.to_string_lossy().to_string());
                        tab.table.clear_modified();
                        self.status_message = Some((
                            format!("Saved to {}", path.display()),
                            std::time::Instant::now(),
                        ));
                    }
                    Err(e) => {
                        self.status_message = Some((
                            format!("Error saving file: {}", e),
                            std::time::Instant::now(),
                        ));
                    }
                }
            }
            None => {
                self.status_message = Some((
                    format!(
                        "No writer available for extension: {}",
                        path.extension()
                            .map(|e| e.to_string_lossy().to_string())
                            .unwrap_or_default()
                    ),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    fn check_for_updates(&self, ctx: &egui::Context) {
        let state = Arc::clone(&self.update_state);
        let ctx = ctx.clone();
        *state.lock().unwrap() = UpdateState::Checking;
        std::thread::spawn(move || {
            let result = (|| -> Result<String, String> {
                let body =
                    ureq::get("https://api.github.com/repos/thorstenfoltz/octa/releases/latest")
                        .header("User-Agent", &format!("octa/{}", VERSION))
                        .header("Accept", "application/vnd.github.v3+json")
                        .call()
                        .map_err(|e| format!("Request failed: {}", e))?
                        .body_mut()
                        .read_to_string()
                        .map_err(|e| format!("Read failed: {}", e))?;

                let resp: serde_json::Value =
                    serde_json::from_str(&body).map_err(|e| format!("Invalid JSON: {}", e))?;

                resp["tag_name"]
                    .as_str()
                    .map(|s: &str| s.trim_start_matches('v').to_string())
                    .ok_or_else(|| "No tag_name in response".to_string())
            })();

            let mut s = state.lock().unwrap();
            match result {
                Ok(latest) if latest != VERSION => *s = UpdateState::Available(latest),
                Ok(_) => *s = UpdateState::UpToDate,
                Err(e) => *s = UpdateState::Error(e),
            }
            ctx.request_repaint();
        });
    }

    fn perform_update(&self, new_version: &str, ctx: &egui::Context) {
        let state = Arc::clone(&self.update_state);
        let ctx = ctx.clone();
        let version = new_version.to_string();
        *state.lock().unwrap() = UpdateState::Updating;

        std::thread::spawn(move || {
            let result = Self::download_and_replace(&version);
            let mut s = state.lock().unwrap();
            match result {
                Ok(()) => *s = UpdateState::Updated(version),
                Err(e) => *s = UpdateState::Error(e),
            }
            ctx.request_repaint();
        });
    }

    fn download_and_replace(new_version: &str) -> Result<(), String> {
        let current_exe =
            std::env::current_exe().map_err(|e| format!("Cannot find current exe: {}", e))?;

        #[cfg(target_os = "linux")]
        {
            let url = format!(
                "https://github.com/thorstenfoltz/octa/releases/download/{0}/octa-{0}-linux-x86_64.tar.gz",
                new_version
            );

            let bytes = ureq::get(&url)
                .header("User-Agent", &format!("octa/{}", VERSION))
                .call()
                .map_err(|e| format!("Download failed: {}", e))?
                .body_mut()
                .read_to_vec()
                .map_err(|e| format!("Read failed: {}", e))?;

            // Extract the binary from the tar.gz
            let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
            let mut archive = tar::Archive::new(decoder);
            let binary_name = format!("octa-{}-linux-x86_64/octa", new_version);

            let mut found = false;
            for entry in archive.entries().map_err(|e| format!("Tar error: {}", e))? {
                let mut entry = entry.map_err(|e| format!("Tar entry error: {}", e))?;
                let path = entry
                    .path()
                    .map_err(|e| format!("Path error: {}", e))?
                    .to_path_buf();
                if path.to_string_lossy() == binary_name {
                    // Write to a temp file next to the current exe
                    let tmp_path = current_exe.with_extension("update");
                    let mut tmp_file = std::fs::File::create(&tmp_path)
                        .map_err(|e| format!("Cannot create temp file: {}", e))?;
                    std::io::copy(&mut entry, &mut tmp_file)
                        .map_err(|e| format!("Extract failed: {}", e))?;

                    // Set executable permission
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))
                            .map_err(|e| format!("chmod failed: {}", e))?;
                    }

                    // Replace: rename current → .old, new → current
                    let old_path = current_exe.with_extension("old");
                    let _ = std::fs::remove_file(&old_path);
                    std::fs::rename(&current_exe, &old_path)
                        .map_err(|e| format!("Backup rename failed: {}", e))?;
                    std::fs::rename(&tmp_path, &current_exe)
                        .map_err(|e| format!("Install rename failed: {}", e))?;
                    let _ = std::fs::remove_file(&old_path);

                    found = true;
                    break;
                }
            }

            if !found {
                return Err(format!("Binary '{}' not found in archive", binary_name));
            }
        }

        #[cfg(target_os = "windows")]
        {
            let url = format!(
                "https://github.com/thorstenfoltz/octa/releases/download/{0}/octa-{0}-windows-x86_64.zip",
                new_version
            );

            let bytes = ureq::get(&url)
                .header("User-Agent", &format!("octa/{}", VERSION))
                .call()
                .map_err(|e| format!("Download failed: {}", e))?
                .body_mut()
                .read_to_vec()
                .map_err(|e| format!("Read failed: {}", e))?;

            let cursor = std::io::Cursor::new(bytes);
            let mut archive =
                zip::ZipArchive::new(cursor).map_err(|e| format!("Zip error: {}", e))?;

            let binary_name = "octa.exe";
            let mut found = false;
            for i in 0..archive.len() {
                let mut file = archive
                    .by_index(i)
                    .map_err(|e| format!("Zip entry error: {}", e))?;
                if file.name().ends_with(binary_name) && !file.name().ends_with('/') {
                    let tmp_path = current_exe.with_extension("update.exe");
                    let mut tmp_file = std::fs::File::create(&tmp_path)
                        .map_err(|e| format!("Cannot create temp file: {}", e))?;
                    std::io::copy(&mut file, &mut tmp_file)
                        .map_err(|e| format!("Extract failed: {}", e))?;

                    // On Windows the running exe can be renamed but not deleted
                    let old_path = current_exe.with_extension("old.exe");
                    let _ = std::fs::remove_file(&old_path);
                    std::fs::rename(&current_exe, &old_path)
                        .map_err(|e| format!("Backup rename failed: {}", e))?;
                    std::fs::rename(&tmp_path, &current_exe)
                        .map_err(|e| format!("Install rename failed: {}", e))?;

                    found = true;
                    break;
                }
            }

            if !found {
                return Err(format!("'{}' not found in archive", binary_name));
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = current_exe;
            let _ = new_version;
            return Err(
                "Auto-update is not supported on this platform. Please download the latest release from the repository.".to_string(),
            );
        }

        Ok(())
    }

    fn recompute_filter(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        if tab.search_text.is_empty() {
            tab.filtered_rows = (0..tab.table.row_count()).collect();
        } else {
            let matcher = RowMatcher::new(&tab.search_text, tab.search_mode);
            tab.filtered_rows = (0..tab.table.row_count())
                .filter(|&row_idx| {
                    (0..tab.table.col_count()).any(|col_idx| {
                        tab.table
                            .get(row_idx, col_idx)
                            .map(|v| matcher.matches(&v.to_string()))
                            .unwrap_or(false)
                    })
                })
                .collect();
        }
        tab.filter_dirty = false;
        tab.table_state.invalidate_row_heights();
    }

    /// Replace the next matching cell value (starting after the current selection).
    fn replace_next_match(&mut self) {
        let tab = &self.tabs[self.active_tab];
        if tab.search_text.is_empty() {
            return;
        }
        let matcher = RowMatcher::new(&tab.search_text, tab.search_mode);
        let row_count = tab.table.row_count();
        let col_count = tab.table.col_count();
        if row_count == 0 || col_count == 0 {
            return;
        }

        // Start searching from the cell after the current selection
        let (start_row, start_col) = match tab.table_state.selected_cell {
            Some((r, c)) => {
                if c + 1 < col_count {
                    (r, c + 1)
                } else if r + 1 < row_count {
                    (r + 1, 0)
                } else {
                    (0, 0) // wrap around
                }
            }
            None => (0, 0),
        };

        let replace_text = tab.replace_text.clone();

        // Scan all cells starting from (start_row, start_col), wrapping around
        let total_cells = row_count * col_count;
        let start_idx = start_row * col_count + start_col;
        for offset in 0..total_cells {
            let idx = (start_idx + offset) % total_cells;
            let row = idx / col_count;
            let col = idx % col_count;
            if let Some(val) = self.tabs[self.active_tab].table.get(row, col) {
                let text = val.to_string();
                if matcher.matches(&text) {
                    let new_text = matcher.replace(&text, &replace_text);
                    let new_val = data::CellValue::parse_like(val, &new_text);
                    if new_val != *val {
                        self.tabs[self.active_tab].table.set(row, col, new_val);
                    }
                    self.tabs[self.active_tab].table_state.selected_cell = Some((row, col));
                    self.tabs[self.active_tab].table_state.selected_rows.clear();
                    self.tabs[self.active_tab].table_state.selected_cols.clear();
                    self.tabs[self.active_tab].filter_dirty = true;
                    self.status_message = Some((
                        format!("Replaced at row {}, col {}", row + 1, col + 1),
                        std::time::Instant::now(),
                    ));
                    return;
                }
            }
        }
        self.status_message = Some(("No match found".to_string(), std::time::Instant::now()));
    }

    /// Replace all matching cell values.
    fn replace_all_matches(&mut self) {
        let tab = &self.tabs[self.active_tab];
        if tab.search_text.is_empty() {
            return;
        }
        let matcher = RowMatcher::new(&tab.search_text, tab.search_mode);
        let replace_text = tab.replace_text.clone();
        let row_count = tab.table.row_count();
        let col_count = tab.table.col_count();
        let mut count = 0usize;
        for row in 0..row_count {
            for col in 0..col_count {
                if let Some(val) = self.tabs[self.active_tab].table.get(row, col).cloned() {
                    let text = val.to_string();
                    if matcher.matches(&text) {
                        let new_text = matcher.replace(&text, &replace_text);
                        let new_val = data::CellValue::parse_like(&val, &new_text);
                        if new_val != val {
                            self.tabs[self.active_tab].table.set(row, col, new_val);
                            count += 1;
                        }
                    }
                }
            }
        }
        self.tabs[self.active_tab].filter_dirty = true;
        self.status_message = Some((
            format!("Replaced {} cell(s)", count),
            std::time::Instant::now(),
        ));
    }

    /// Open the "Delete Columns" dialog, initializing checkboxes.
    fn open_delete_columns_dialog(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        tab.delete_col_selection = vec![false; tab.table.col_count()];
        // Pre-select the currently selected column if any
        if let Some((_, col)) = tab.table_state.selected_cell {
            if col < tab.delete_col_selection.len() {
                tab.delete_col_selection[col] = true;
            }
        }
        tab.show_delete_columns_dialog = true;
    }

    /// Sort columns alphabetically by name, ascending or descending.
    #[allow(dead_code)]
    fn sort_columns_alphabetically(&mut self, ascending: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let col_count = tab.table.col_count();
        if col_count <= 1 {
            return;
        }

        // order[new_pos] = old_pos
        let mut order: Vec<usize> = (0..col_count).collect();
        order.sort_by(|&a, &b| {
            let cmp = tab.table.columns[a]
                .name
                .to_lowercase()
                .cmp(&tab.table.columns[b].name.to_lowercase());
            if ascending { cmp } else { cmp.reverse() }
        });

        // Reorder column widths to match
        let old_widths = tab.table_state.col_widths.clone();
        tab.table_state.col_widths = order
            .iter()
            .map(|&orig| old_widths.get(orig).copied().unwrap_or(120.0))
            .collect();

        // Update selected cell column: build reverse map
        if let Some((row, col)) = tab.table_state.selected_cell {
            if let Some(new_col) = order.iter().position(|&orig| orig == col) {
                tab.table_state.selected_cell = Some((row, new_col));
            }
        }

        // Apply the reorder atomically
        tab.table.reorder_columns(&order);
        tab.filter_dirty = true;
    }
}

impl eframe::App for OctaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Load file from CLI on first frame ---
        if let Some(path) = self.initial_file.take() {
            self.load_file(path);
        }

        // --- Global keyboard shortcuts ---
        // All bindings are read from `self.settings.shortcuts`, which the user
        // can customize via Settings → Shortcuts. Fixed key handling (Ctrl+1..9
        // tab jumps, Esc closing the replace bar) stays hard-coded because it
        // isn't user-configurable.
        use octa::ui::shortcuts::ShortcutAction as SA;
        let shortcuts = self.settings.shortcuts.clone();
        let action_fired = |a: SA| ctx.input(|i| shortcuts.triggered(a, i));

        if action_fired(SA::NewFile) {
            let mut new_tab = TabState::new(self.settings.default_search_mode);
            new_tab.view_mode = ViewMode::Raw;
            new_tab.raw_content = Some(String::new());
            self.tabs.push(new_tab);
            self.active_tab = self.tabs.len() - 1;
        }
        if action_fired(SA::OpenFile) {
            self.open_file();
        }
        if action_fired(SA::SaveFile) {
            if self.tabs[self.active_tab].table.source_path.is_some() {
                self.save_file();
            } else if self.tabs[self.active_tab].table.col_count() > 0
                || self.tabs[self.active_tab].raw_content_modified
            {
                self.save_file_as();
            }
        }
        if action_fired(SA::FocusSearch) {
            self.search_focus_requested = true;
        }
        if action_fired(SA::ToggleFindReplace) {
            self.tabs[self.active_tab].show_replace_bar =
                !self.tabs[self.active_tab].show_replace_bar;
            self.search_focus_requested = true;
        }
        if self.tabs[self.active_tab].show_replace_bar
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            self.tabs[self.active_tab].show_replace_bar = false;
        }
        if action_fired(SA::QuitApp) {
            if self.tabs[self.active_tab].is_modified() && !self.confirmed_close {
                self.show_close_confirm = true;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if action_fired(SA::CloseTab) {
            if self.tabs[self.active_tab].is_modified() {
                self.pending_close_tab = Some(self.active_tab);
                self.show_close_confirm = true;
            } else {
                self.close_tab(self.active_tab);
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                    self.tabs[self.active_tab].title_display(),
                ));
            }
        }
        if action_fired(SA::NextTab) {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.tabs[self.active_tab].title_display(),
            ));
        }
        if action_fired(SA::PrevTab) {
            if self.active_tab == 0 {
                self.active_tab = self.tabs.len() - 1;
            } else {
                self.active_tab -= 1;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.tabs[self.active_tab].title_display(),
            ));
        }
        // Ctrl+1..9: jump to tab by number (not user-configurable)
        let ctrl_held = ctx.input(|i| i.modifiers.command);
        for n in 1..=9u8 {
            let key = match n {
                1 => egui::Key::Num1,
                2 => egui::Key::Num2,
                3 => egui::Key::Num3,
                4 => egui::Key::Num4,
                5 => egui::Key::Num5,
                6 => egui::Key::Num6,
                7 => egui::Key::Num7,
                8 => egui::Key::Num8,
                9 => egui::Key::Num9,
                _ => unreachable!(),
            };
            if ctrl_held && ctx.input(|i| i.key_pressed(key)) {
                let idx = (n as usize) - 1;
                if idx < self.tabs.len() {
                    self.active_tab = idx;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                        self.tabs[self.active_tab].title_display(),
                    ));
                }
            }
        }
        // Only select all table rows when no TextEdit has focus — otherwise
        // Ctrl+A should scope to the text editor (SQL, raw, search bars, etc.)
        // and leave the table alone.
        let text_edit_focused = ctx
            .memory(|m| m.focused())
            .and_then(|id| egui::TextEdit::load_state(ctx, id).map(|_| ()))
            .is_some();
        if action_fired(SA::SelectAllRows)
            && !text_edit_focused
            && self.tabs[self.active_tab].table.col_count() > 0
            && self.tabs[self.active_tab].table.row_count() > 0
        {
            self.tabs[self.active_tab].table_state.selected_rows.clear();
            self.tabs[self.active_tab].table_state.selected_cols.clear();
            for r in 0..self.tabs[self.active_tab].table.row_count() {
                self.tabs[self.active_tab]
                    .table_state
                    .selected_rows
                    .insert(r);
            }
        }
        if action_fired(SA::ExportSqlResult)
            && self.tabs[self.active_tab]
                .sql_result
                .as_ref()
                .is_some_and(|t| t.col_count() > 0)
        {
            self.export_sql_result();
        }

        // ZoomIn also accepts Ctrl+Equals in addition to the user's binding —
        // on US layouts Ctrl++ is typed as Ctrl+= by the keyboard driver.
        let zoom_equals_fallback = shortcuts.combo(SA::ZoomIn).key == Some(egui::Key::Plus)
            && ctx.input(|i| {
                i.modifiers.command
                    && !i.modifiers.alt
                    && !i.modifiers.shift
                    && i.key_pressed(egui::Key::Equals)
            });
        if action_fired(SA::ZoomIn) || zoom_equals_fallback {
            self.zoom_percent = (self.zoom_percent + 5).min(500);
            self.apply_zoom(ctx);
            self.tabs[self.active_tab].table_state.invalidate_row_heights();
        }
        if action_fired(SA::ZoomOut) {
            self.zoom_percent = self.zoom_percent.saturating_sub(5).max(25);
            self.apply_zoom(ctx);
            self.tabs[self.active_tab].table_state.invalidate_row_heights();
        }
        if action_fired(SA::ZoomReset) {
            self.zoom_percent = 100;
            self.apply_zoom(ctx);
            self.tabs[self.active_tab].table_state.invalidate_row_heights();
        }

        let lower_fired = action_fired(SA::LowercaseSelection);
        let upper_fired = action_fired(SA::UppercaseSelection);
        if lower_fired || upper_fired {
            let op = if upper_fired {
                view_modes::text_ops::CaseOp::Upper
            } else {
                view_modes::text_ops::CaseOp::Lower
            };
            // Consume the key press so built-in TextEdit bindings (e.g. egui's
            // Ctrl+U = delete-to-start-of-line, which ignores Alt) don't also
            // fire on the same event.
            let combo = self.settings.shortcuts.combo(if upper_fired {
                SA::UppercaseSelection
            } else {
                SA::LowercaseSelection
            });
            if let Some(key) = combo.key {
                let modifiers = egui::Modifiers {
                    alt: combo.alt,
                    ctrl: combo.ctrl,
                    shift: combo.shift,
                    mac_cmd: false,
                    command: combo.ctrl,
                };
                ctx.input_mut(|i| i.consume_key(modifiers, key));
            }
            let sql_id = view_modes::sql_editor_id();
            let raw_id = egui::Id::new("raw_text_editor");
            let focused = ctx.memory(|m| m.focused());
            if focused == Some(sql_id) {
                let tab = &mut self.tabs[self.active_tab];
                view_modes::text_ops::apply_case_to_selection(ctx, sql_id, &mut tab.sql_query, op);
            } else if focused == Some(raw_id) {
                let tab = &mut self.tabs[self.active_tab];
                if let Some(ref mut content) = tab.raw_content {
                    if view_modes::text_ops::apply_case_to_selection(ctx, raw_id, content, op) {
                        tab.raw_content_modified = true;
                    }
                }
            } else if lower_fired {
                self.transform_selected_cells(str::to_lowercase);
            } else {
                self.transform_selected_cells(str::to_uppercase);
            }
        }
        if action_fired(SA::SaveFileAs)
            && (self.tabs[self.active_tab].table.col_count() > 0
                || self.tabs[self.active_tab].raw_content_modified)
        {
            self.save_file_as();
        }
        if action_fired(SA::ReloadFile) && self.tabs[self.active_tab].table.source_path.is_some() {
            if self.tabs[self.active_tab].is_modified() {
                self.show_reload_confirm = true;
            } else {
                self.reload_active_file();
            }
        }
        if action_fired(SA::GoToCell) {
            self.nav_focus_requested = true;
        }
        if action_fired(SA::EditCell) && self.tabs[self.active_tab].table.col_count() > 0 {
            let tab = &mut self.tabs[self.active_tab];
            let binary_mode = self.settings.binary_display_mode;
            if let Some((r, c)) = tab.table_state.selected_cell {
                let text = tab
                    .table
                    .get(r, c)
                    .map(|v| v.display_with_binary_mode(binary_mode))
                    .unwrap_or_default();
                tab.table_state.begin_edit(r, c, text);
            }
        }
        if action_fired(SA::DuplicateRow) {
            self.duplicate_selected_rows();
        }
        if action_fired(SA::DeleteRow) {
            self.delete_selected_rows();
        }
        if action_fired(SA::InsertRowBelow) && self.tabs[self.active_tab].table.col_count() > 0 {
            let tab = &mut self.tabs[self.active_tab];
            let insert_at = tab
                .table_state
                .selected_cell
                .map(|(r, _)| r + 1)
                .unwrap_or(tab.table.row_count());
            tab.table.insert_row(insert_at);
            tab.filter_dirty = true;
        }
        if action_fired(SA::ToggleSqlPanel)
            && self.tabs[self.active_tab].view_mode == ViewMode::Table
        {
            self.tabs[self.active_tab].sql_panel_open = !self.tabs[self.active_tab].sql_panel_open;
        }
        if action_fired(SA::CycleViewMode) {
            self.cycle_view_mode();
        }

        // --- Handle close request ---
        if ctx.input(|i| i.viewport().close_requested())
            && self.tabs[self.active_tab].is_modified()
            && !self.confirmed_close
        {
            // Block the close and show our dialog
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.show_close_confirm = true;
        }
        // If confirmed_close is true, we just let it close

        // Drain background-loaded rows into the table
        if let Some(ref buffer) = self.tabs[self.active_tab].bg_row_buffer.clone() {
            let mut drained = false;
            if let Ok(mut buf) = buffer.try_lock() {
                if !buf.is_empty() {
                    self.tabs[self.active_tab].table.rows.append(&mut *buf);
                    drained = true;
                }
            }
            let loading_done = self.tabs[self.active_tab]
                .bg_loading_done
                .load(std::sync::atomic::Ordering::Relaxed);
            if drained {
                self.tabs[self.active_tab].filter_dirty = true;
                let file_exhausted = self.tabs[self.active_tab]
                    .bg_file_exhausted
                    .load(std::sync::atomic::Ordering::Relaxed);
                if self.tabs[self.active_tab].table.total_rows.is_some() {
                    let total_loaded = self.tabs[self.active_tab].table.row_offset
                        + self.tabs[self.active_tab].table.row_count();
                    let total_fmt = ui::status_bar::format_number(total_loaded);
                    if loading_done && file_exhausted {
                        self.status_message = Some((
                            format!("Loaded all {} rows", total_fmt),
                            std::time::Instant::now(),
                        ));
                        self.tabs[self.active_tab].table.total_rows = None;
                        self.tabs[self.active_tab].bg_can_load_more = false;
                    } else if loading_done {
                        self.status_message = Some((
                            format!("Loaded {} rows (scroll down to load more)", total_fmt),
                            std::time::Instant::now(),
                        ));
                        self.tabs[self.active_tab].bg_can_load_more = true;
                    } else {
                        self.status_message = Some((
                            format!("Loading... {} rows so far", total_fmt),
                            std::time::Instant::now(),
                        ));
                    }
                }
                // Evict front rows if we have too many in memory
                if self.tabs[self.active_tab].table.rows.len() > 3_000_000 {
                    let evict_count = self.tabs[self.active_tab].table.rows.len() - 2_000_000;
                    self.tabs[self.active_tab]
                        .table
                        .evict_front_rows(evict_count);
                    self.tabs[self.active_tab].filter_dirty = true;
                }
            }
            if loading_done {
                self.tabs[self.active_tab].bg_row_buffer = None;
            }
            // Request repaint to keep draining
            if !loading_done {
                ctx.request_repaint();
            }
        }

        // Recompute filter if needed
        if self.tabs[self.active_tab].filter_dirty {
            self.recompute_filter();
        }

        let search_active = !self.tabs[self.active_tab].search_text.is_empty();
        let filtered_count = self.tabs[self.active_tab].filtered_rows.len();

        // Top toolbar — framed in bg_header to avoid the flat egui grey.
        let header_colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        let toolbar_frame = egui::Frame::new()
            .fill(header_colors.bg_header)
            .inner_margin(egui::Margin::symmetric(4, 4))
            .stroke(egui::Stroke::new(1.0, header_colors.border_subtle));
        egui::TopBottomPanel::top("toolbar")
            .exact_height(40.0)
            .frame(toolbar_frame)
            .show(ctx, |ui| {
                // Lazily create logo texture
                if self.logo_texture.is_none() || self.welcome_logo_texture.is_none() {
                    let opt = resvg::usvg::Options::default();
                    let svg_src = self.settings.icon_variant.svg_source();
                    if let Ok(tree) = resvg::usvg::Tree::from_str(svg_src, &opt) {
                        if self.logo_texture.is_none() {
                            let size = tree.size();
                            let (w, h) = (size.width() as u32, size.height() as u32);
                            if let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(w, h) {
                                resvg::render(
                                    &tree,
                                    resvg::tiny_skia::Transform::default(),
                                    &mut pixmap.as_mut(),
                                );
                                let image = egui::ColorImage::from_rgba_unmultiplied(
                                    [w as usize, h as usize],
                                    pixmap.data(),
                                );
                                self.logo_texture = Some(ctx.load_texture(
                                    "octa_logo",
                                    image,
                                    egui::TextureOptions::LINEAR,
                                ));
                            }
                        }
                        if self.welcome_logo_texture.is_none() {
                            let render_size = 512u32;
                            let size = tree.size();
                            let sx = render_size as f32 / size.width();
                            let sy = render_size as f32 / size.height();
                            if let Some(mut pixmap) =
                                resvg::tiny_skia::Pixmap::new(render_size, render_size)
                            {
                                resvg::render(
                                    &tree,
                                    resvg::tiny_skia::Transform::from_scale(sx, sy),
                                    &mut pixmap.as_mut(),
                                );
                                let image = egui::ColorImage::from_rgba_unmultiplied(
                                    [render_size as usize, render_size as usize],
                                    pixmap.data(),
                                );
                                self.welcome_logo_texture = Some(ctx.load_texture(
                                    "octa_welcome_logo",
                                    image,
                                    egui::TextureOptions::LINEAR,
                                ));
                            }
                        }
                    }
                }

                let tab = &mut self.tabs[self.active_tab];
                let action = ui::toolbar::draw_toolbar(
                    ui,
                    self.theme_mode,
                    &mut tab.search_text,
                    &mut tab.search_mode,
                    self.search_focus_requested,
                    tab.show_replace_bar,
                    &mut tab.replace_text,
                    tab.table.col_count() > 0,
                    tab.table.is_modified(),
                    tab.table.source_path.is_some(),
                    tab.table_state.selected_cell,
                    tab.table.row_count(),
                    tab.table.col_count(),
                    tab.view_mode,
                    tab.raw_content.is_some(),
                    !tab.pdf_page_images.is_empty(),
                    tab.table.format_name.as_deref() == Some("Markdown"),
                    tab.table.format_name.as_deref() == Some("Jupyter Notebook"),
                    tab.json_value.is_some(),
                    tab.sql_panel_open,
                    self.zoom_percent,
                    self.logo_texture.as_ref(),
                    &self.recent_files,
                    self.directory_tree.is_some(),
                );
                // Clear focus request after this frame
                self.search_focus_requested = false;

                if action.new_file {
                    let mut new_tab = TabState::new(self.settings.default_search_mode);
                    new_tab.view_mode = ViewMode::Raw;
                    new_tab.raw_content = Some(String::new());
                    self.tabs.push(new_tab);
                    self.active_tab = self.tabs.len() - 1;
                }
                if action.open_file {
                    self.open_file();
                }
                if action.open_directory {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.directory_tree =
                            Some(ui::directory_tree::DirectoryTreeState::new(path));
                    }
                }
                if action.close_directory {
                    self.directory_tree = None;
                }
                if let Some(ref path) = action.open_recent {
                    let path_buf = std::path::PathBuf::from(path);
                    if path_buf.exists() {
                        self.load_file(path_buf);
                    } else {
                        self.recent_files.retain(|p| p != path);
                        self.save_recent_files();
                        self.status_message =
                            Some((format!("File not found: {path}"), std::time::Instant::now()));
                    }
                }
                if action.save_file {
                    self.save_file();
                }
                if action.save_file_as {
                    self.save_file_as();
                }
                if action.exit {
                    if self.tabs[self.active_tab].is_modified() && !self.confirmed_close {
                        self.show_close_confirm = true;
                    } else {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                if action.toggle_theme {
                    self.theme_mode = self.theme_mode.toggle();
                    self.apply_zoom(ctx);
                }
                if action.zoom_in {
                    self.zoom_percent = (self.zoom_percent + 5).min(500);
                    self.apply_zoom(ctx);
                    self.tabs[self.active_tab].table_state.invalidate_row_heights();
                }
                if action.zoom_out {
                    self.zoom_percent = self.zoom_percent.saturating_sub(5).max(25);
                    self.apply_zoom(ctx);
                    self.tabs[self.active_tab].table_state.invalidate_row_heights();
                }
                if action.zoom_reset {
                    self.zoom_percent = 100;
                    self.apply_zoom(ctx);
                    self.tabs[self.active_tab].table_state.invalidate_row_heights();
                }
                if action.search_changed {
                    self.tabs[self.active_tab].filter_dirty = true;
                }
                if action.toggle_replace_bar {
                    self.tabs[self.active_tab].show_replace_bar =
                        !self.tabs[self.active_tab].show_replace_bar;
                }
                if action.replace_next {
                    self.replace_next_match();
                }
                if action.replace_all {
                    self.replace_all_matches();
                }

                // --- View mode change ---
                if let Some(new_mode) = action.view_mode_changed {
                    self.tabs[self.active_tab].view_mode = new_mode;
                }

                // --- SQL panel toggle ---
                if action.toggle_sql_panel {
                    let tab = &mut self.tabs[self.active_tab];
                    tab.sql_panel_open = !tab.sql_panel_open;
                }

                // --- Search actions ---
                if action.search_focus {
                    self.search_focus_requested = true;
                }

                // --- Help actions ---
                if action.show_documentation {
                    self.show_documentation_dialog = true;
                }
                if action.show_settings {
                    self.settings_dialog.open(&self.settings);
                }
                if action.show_about {
                    self.show_about_dialog = true;
                }
                if action.check_for_updates {
                    self.show_update_dialog = true;
                    self.check_for_updates(ctx);
                }

                // --- Row operations ---
                if action.add_row {
                    let insert_at = match self.tabs[self.active_tab].table_state.selected_cell {
                        Some((row, _)) => row + 1,
                        None => self.tabs[self.active_tab].table.row_count(),
                    };
                    self.tabs[self.active_tab].table.insert_row(insert_at);
                    let sel_col = self.tabs[self.active_tab]
                        .table_state
                        .selected_cell
                        .map(|(_, c)| c)
                        .unwrap_or(0);
                    self.tabs[self.active_tab].table_state.selected_cell =
                        Some((insert_at, sel_col));
                    self.tabs[self.active_tab].table_state.editing_cell = None;
                    self.tabs[self.active_tab].filter_dirty = true;
                }
                if action.delete_row {
                    if let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell {
                        self.tabs[self.active_tab].table.delete_row(row);
                        self.tabs[self.active_tab].table_state.editing_cell = None;
                        if self.tabs[self.active_tab].table.row_count() == 0 {
                            self.tabs[self.active_tab].table_state.selected_cell = None;
                        } else {
                            let new_row = row.min(self.tabs[self.active_tab].table.row_count() - 1);
                            self.tabs[self.active_tab].table_state.selected_cell =
                                Some((new_row, col));
                        }
                        self.tabs[self.active_tab].filter_dirty = true;
                    }
                }
                if action.move_row_up {
                    if let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell {
                        if row > 0 {
                            self.tabs[self.active_tab].table.move_row(row, row - 1);
                            self.tabs[self.active_tab].table_state.selected_cell =
                                Some((row - 1, col));
                            self.tabs[self.active_tab].filter_dirty = true;
                        }
                    }
                }
                if action.move_row_down {
                    if let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell {
                        if row + 1 < self.tabs[self.active_tab].table.row_count() {
                            self.tabs[self.active_tab].table.move_row(row, row + 1);
                            self.tabs[self.active_tab].table_state.selected_cell =
                                Some((row + 1, col));
                            self.tabs[self.active_tab].filter_dirty = true;
                        }
                    }
                }

                // --- Column operations ---
                if action.add_column {
                    self.tabs[self.active_tab].show_add_column_dialog = true;
                    self.tabs[self.active_tab].new_col_name.clear();
                    self.tabs[self.active_tab].new_col_type = "String".to_string();
                    self.tabs[self.active_tab].new_col_formula.clear();
                    // Insert after selected column, or at end
                    self.tabs[self.active_tab].insert_col_at = self.tabs[self.active_tab]
                        .table_state
                        .selected_cell
                        .map(|(_, c)| c + 1);
                }
                if action.delete_column && self.tabs[self.active_tab].table.col_count() > 0 {
                    self.open_delete_columns_dialog();
                }
                if action.move_col_left {
                    if let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell {
                        if col > 0 {
                            self.tabs[self.active_tab].table.move_column(col, col - 1);
                            self.tabs[self.active_tab].table_state.selected_cell =
                                Some((row, col - 1));
                            self.tabs[self.active_tab].table_state.widths_initialized = false;
                        }
                    }
                }
                if action.move_col_right {
                    if let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell {
                        if col + 1 < self.tabs[self.active_tab].table.col_count() {
                            self.tabs[self.active_tab].table.move_column(col, col + 1);
                            self.tabs[self.active_tab].table_state.selected_cell =
                                Some((row, col + 1));
                            self.tabs[self.active_tab].table_state.widths_initialized = false;
                        }
                    }
                }
                if let Some(col_idx) = action.sort_rows_asc_by {
                    self.tabs[self.active_tab]
                        .table
                        .sort_rows_by_column(col_idx, true);
                    self.tabs[self.active_tab].filter_dirty = true;
                }
                if let Some(col_idx) = action.sort_rows_desc_by {
                    self.tabs[self.active_tab]
                        .table
                        .sort_rows_by_column(col_idx, false);
                    self.tabs[self.active_tab].filter_dirty = true;
                }

                if action.discard_edits {
                    self.tabs[self.active_tab].table.discard_edits();
                }
            });

        // --- Tab bar ---
        // Render whenever any file/buffer is open (including after "+ New
        // File"). Hidden in the pristine startup state so the welcome screen
        // isn't capped by an empty tab strip.
        let has_open_file = self.tabs.iter().any(|t| {
            t.table.source_path.is_some() || t.raw_content.is_some() || t.table.col_count() > 0
        });
        if has_open_file {
            let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
            let tab_frame = egui::Frame::new()
                .fill(colors.bg_secondary)
                .inner_margin(egui::Margin::symmetric(4, 2))
                .stroke(egui::Stroke::new(1.0, colors.border_subtle));
            egui::TopBottomPanel::top("tab_bar")
                .exact_height(28.0)
                .frame(tab_frame)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let mut tab_to_close: Option<usize> = None;
                        let mut tab_to_activate: Option<usize> = None;

                        for (idx, tab) in self.tabs.iter().enumerate() {
                            let is_active = idx == self.active_tab;
                            let label = tab.title_display();
                            let hover_path = tab
                                .table
                                .source_path
                                .clone()
                                .unwrap_or_else(|| "Untitled".to_string());

                            let bg = if is_active {
                                colors.accent.gamma_multiply(0.3)
                            } else {
                                Color32::TRANSPARENT
                            };

                            let frame = egui::Frame::new()
                                .fill(bg)
                                .inner_margin(egui::Margin::symmetric(8, 4))
                                .corner_radius(4.0);

                            frame.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let text = if is_active {
                                        egui::RichText::new(&label)
                                            .strong()
                                            .color(colors.text_primary)
                                    } else {
                                        egui::RichText::new(&label).color(colors.text_secondary)
                                    };
                                    let tab_label_resp = ui
                                        .add(egui::Label::new(text).sense(egui::Sense::click()))
                                        .on_hover_text(&hover_path);
                                    if tab_label_resp.hovered() {
                                        ctx.set_cursor_icon(egui::CursorIcon::Default);
                                    }
                                    if tab_label_resp.clicked() {
                                        tab_to_activate = Some(idx);
                                    }
                                    // Close button
                                    let close_resp = ui.add(
                                        egui::Label::new(
                                            egui::RichText::new("  \u{00D7}")
                                                .size(14.0)
                                                .color(colors.text_muted),
                                        )
                                        .sense(egui::Sense::click() | egui::Sense::hover()),
                                    );
                                    if close_resp.hovered() {
                                        ctx.set_cursor_icon(egui::CursorIcon::Default);
                                        let r = close_resp.rect.shrink2(egui::vec2(2.0, 1.0));
                                        ui.painter().rect_filled(
                                            r,
                                            3.0,
                                            colors.accent.gamma_multiply(0.25),
                                        );
                                        ui.painter().text(
                                            r.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "\u{00D7}",
                                            egui::FontId::proportional(14.0),
                                            colors.error,
                                        );
                                    }
                                    if close_resp.clicked() {
                                        tab_to_close = Some(idx);
                                    }
                                });
                            });
                        }

                        // "+" button to add new empty tab (opens editor)
                        if ui
                            .add(egui::Button::new(
                                egui::RichText::new("+").size(14.0).color(colors.text_muted),
                            ))
                            .clicked()
                        {
                            let mut new_tab = TabState::new(self.settings.default_search_mode);
                            new_tab.view_mode = ViewMode::Raw;
                            new_tab.raw_content = Some(String::new());
                            self.tabs.push(new_tab);
                            tab_to_activate = Some(self.tabs.len() - 1);
                        }

                        // Process tab actions
                        if let Some(idx) = tab_to_activate {
                            self.active_tab = idx;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                                self.tabs[self.active_tab].title_display(),
                            ));
                        }
                        if let Some(idx) = tab_to_close {
                            if self.tabs[idx].is_modified() {
                                self.pending_close_tab = Some(idx);
                                self.show_close_confirm = true;
                            } else {
                                self.close_tab(idx);
                                ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                                    self.tabs[self.active_tab].title_display(),
                                ));
                            }
                        }
                    });
                });
        }

        // --- Directory tree sidebar ---
        if self.directory_tree.is_some() {
            let tree_action = {
                let position = self.settings.directory_tree_position;
                let state = self.directory_tree.as_mut().unwrap();
                let mut action = ui::directory_tree::TreeAction::default();
                // Default to a 50/50 split the first time the sidebar is
                // shown; subsequent frames honor whatever width the user
                // has dragged the separator to.
                let screen_w = ctx.screen_rect().width();
                let default_w = (screen_w * 0.5).clamp(160.0, screen_w - 160.0);
                let max_w = (screen_w - 80.0).max(160.0);
                match position {
                    ui::settings::DirectoryTreePosition::Left => {
                        egui::SidePanel::left("directory_tree_panel")
                            .resizable(true)
                            .default_width(default_w)
                            .width_range(80.0..=max_w)
                            .show(ctx, |ui| {
                                action = ui::directory_tree::render_directory_tree(ui, state);
                            });
                    }
                    ui::settings::DirectoryTreePosition::Right => {
                        egui::SidePanel::right("directory_tree_panel")
                            .resizable(true)
                            .default_width(default_w)
                            .width_range(80.0..=max_w)
                            .show(ctx, |ui| {
                                action = ui::directory_tree::render_directory_tree(ui, state);
                            });
                    }
                }
                action
            };
            if tree_action.close {
                self.directory_tree = None;
            } else if let Some(path) = tree_action.open_file {
                self.load_file(path);
            }
        }

        // --- Add Column dialog ---
        if self.tabs[self.active_tab].show_add_column_dialog {
            let mut open = true;
            let mut should_add = false;
            egui::Window::new("Insert Column")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.tabs[self.active_tab].new_col_name);
                    });
                    // Autofill: show existing column names that match what the
                    // user has typed so far. Clicking one fills the Name field.
                    let typed = self.tabs[self.active_tab].new_col_name.clone();
                    if !typed.is_empty() {
                        let lower = typed.to_lowercase();
                        let matches: Vec<String> = self.tabs[self.active_tab]
                            .table
                            .columns
                            .iter()
                            .filter(|c| {
                                let n = c.name.to_lowercase();
                                n != lower && n.contains(&lower)
                            })
                            .take(8)
                            .map(|c| c.name.clone())
                            .collect();
                        if !matches.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("Autofill:")
                                        .size(10.0)
                                        .color(ui.visuals().weak_text_color()),
                                );
                                for name in matches {
                                    if ui.small_button(&name).clicked() {
                                        self.tabs[self.active_tab].new_col_name = name;
                                    }
                                }
                            });
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.label("Type:");
                        egui::ComboBox::from_id_salt("col_type_combo")
                            .selected_text(self.tabs[self.active_tab].new_col_type.as_str())
                            .show_ui(ui, |ui| {
                                for t in COLUMN_TYPES {
                                    ui.selectable_value(
                                        &mut self.tabs[self.active_tab].new_col_type,
                                        t.to_string(),
                                        *t,
                                    );
                                }
                            });
                    });
                    ui.add_space(4.0);
                    // Show/edit insert position
                    ui.horizontal(|ui| {
                        ui.label("Insert at position:");
                        let col_count = self.tabs[self.active_tab].table.col_count();
                        let mut pos_val = self.tabs[self.active_tab]
                            .insert_col_at
                            .unwrap_or(col_count)
                            + 1;
                        let drag = egui::DragValue::new(&mut pos_val)
                            .range(1..=(col_count + 1))
                            .speed(1.0);
                        if ui.add(drag).changed() {
                            self.tabs[self.active_tab].insert_col_at =
                                Some((pos_val - 1).min(col_count));
                        }
                        ui.label(format!("/ {}", col_count + 1));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Formula:");
                        ui.add(
                            egui::TextEdit::singleline(
                                &mut self.tabs[self.active_tab].new_col_formula,
                            )
                            .hint_text("e.g. =A1+B1 or =A1*2"),
                        );
                    });
                    ui.label(
                        RichText::new(
                            "Tip: click a column header to set insert position. \
                             Formula uses Excel-style references (A1, B2, ...) with +, -, *, /.",
                        )
                        .size(10.0)
                        .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Add").clicked()
                            && !self.tabs[self.active_tab].new_col_name.is_empty()
                        {
                            should_add = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.tabs[self.active_tab].show_add_column_dialog = false;
                        }
                    });
                });
            if should_add {
                let idx = self.tabs[self.active_tab]
                    .insert_col_at
                    .unwrap_or(self.tabs[self.active_tab].table.col_count());
                let formula_text = self.tabs[self.active_tab]
                    .new_col_formula
                    .trim()
                    .to_string();
                let col_name = self.tabs[self.active_tab].new_col_name.clone();
                let col_type = self.tabs[self.active_tab].new_col_type.clone();
                self.tabs[self.active_tab]
                    .table
                    .insert_column(idx, col_name, col_type);
                // If a formula was provided, evaluate it for every row
                if formula_text.starts_with('=') {
                    let formula_body = &formula_text[1..];
                    let row_count = self.tabs[self.active_tab].table.row_count();
                    for row in 0..row_count {
                        let shifted = shift_formula_row(formula_body, row);
                        if let Some(result) =
                            data::evaluate_formula(&shifted, &self.tabs[self.active_tab].table)
                        {
                            let val = if result.fract() == 0.0 && result.abs() < i64::MAX as f64 {
                                data::CellValue::Int(result as i64)
                            } else {
                                data::CellValue::Float(result)
                            };
                            self.tabs[self.active_tab].table.set(row, idx, val);
                        }
                    }
                }
                // Select the new column
                if let Some((row, _)) = self.tabs[self.active_tab].table_state.selected_cell {
                    self.tabs[self.active_tab].table_state.selected_cell = Some((row, idx));
                }
                self.tabs[self.active_tab].table_state.widths_initialized = false;
                self.tabs[self.active_tab].filter_dirty = true;
                self.tabs[self.active_tab].show_add_column_dialog = false;
            }
            if !open {
                self.tabs[self.active_tab].show_add_column_dialog = false;
            }
        }

        // --- Delete Columns dialog ---
        if self.tabs[self.active_tab].show_delete_columns_dialog {
            let mut open = true;
            let mut should_delete = false;
            // Make sure selection vec is in sync (table may have changed)
            let tab = &mut self.tabs[self.active_tab];
            if tab.delete_col_selection.len() != tab.table.col_count() {
                tab.delete_col_selection = vec![false; tab.table.col_count()];
            }
            egui::Window::new("Delete Columns")
                .open(&mut open)
                .resizable(true)
                .collapsible(false)
                .min_width(280.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Select columns to delete:");
                    ui.add_space(6.0);

                    let tab = &mut self.tabs[self.active_tab];
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for (idx, col) in tab.table.columns.iter().enumerate() {
                                let mut checked = tab.delete_col_selection[idx];
                                let label = format!("{} [{}]", col.name, col.data_type);
                                if ui.checkbox(&mut checked, label).changed() {
                                    tab.delete_col_selection[idx] = checked;
                                }
                            }
                        });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.small_button("All").clicked() {
                            for v in &mut tab.delete_col_selection {
                                *v = true;
                            }
                        }
                        if ui.small_button("None").clicked() {
                            for v in &mut tab.delete_col_selection {
                                *v = false;
                            }
                        }
                    });

                    let selected_count = tab.delete_col_selection.iter().filter(|&&v| v).count();
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let delete_btn = ui.add_enabled(
                            selected_count > 0,
                            egui::Button::new(format!("Delete ({} selected)", selected_count)),
                        );
                        if delete_btn.clicked() {
                            should_delete = true;
                        }
                        if ui.button("Cancel").clicked() {
                            tab.show_delete_columns_dialog = false;
                        }
                    });
                });

            if should_delete {
                let tab = &mut self.tabs[self.active_tab];
                // Delete in reverse order to keep indices valid
                let to_delete: Vec<usize> = tab
                    .delete_col_selection
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &sel)| if sel { Some(i) } else { None })
                    .rev()
                    .collect();

                for col_idx in to_delete {
                    tab.table.delete_column(col_idx);
                }

                tab.table_state.editing_cell = None;
                if tab.table.col_count() == 0 {
                    tab.table_state.selected_cell = None;
                } else if let Some((row, col)) = tab.table_state.selected_cell {
                    let new_col = col.min(tab.table.col_count() - 1);
                    tab.table_state.selected_cell = Some((row, new_col));
                }
                tab.table_state.widths_initialized = false;
                tab.filter_dirty = true;
                tab.show_delete_columns_dialog = false;
            }

            if !open {
                self.tabs[self.active_tab].show_delete_columns_dialog = false;
            }
        }

        // --- Unsaved changes confirmation dialog ---
        if self.show_close_confirm {
            egui::Window::new("Unsaved Changes")
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("You have unsaved changes. What would you like to do?");
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            self.show_close_confirm = false;
                            if let Some(tab_idx) = self.pending_close_tab {
                                // Closing a specific tab
                                self.save_tab(tab_idx);
                                self.close_tab(tab_idx);
                                self.pending_close_tab = None;
                            } else {
                                // Closing the entire app
                                if self.tabs[self.active_tab].table.source_path.is_some() {
                                    self.save_file();
                                } else {
                                    self.save_file_as();
                                }
                                self.confirmed_close = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        if ui.button("Don't Save").clicked() {
                            self.show_close_confirm = false;
                            if let Some(tab_idx) = self.pending_close_tab {
                                self.close_tab(tab_idx);
                                self.pending_close_tab = None;
                            } else {
                                self.confirmed_close = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_close_confirm = false;
                            self.pending_close_tab = None;
                        }
                    });
                });
        }

        // --- Unsaved changes before opening new file ---
        if self.show_open_confirm {
            egui::Window::new("Unsaved Changes")
                .id(egui::Id::new("open_confirm"))
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("You have unsaved changes. What would you like to do?");
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            self.show_open_confirm = false;
                            if self.tabs[self.active_tab].table.source_path.is_some() {
                                self.save_file();
                            } else {
                                self.save_file_as();
                            }
                            self.do_open_file_dialog();
                        }
                        if ui.button("Don't Save").clicked() {
                            self.show_open_confirm = false;
                            self.tabs[self.active_tab].table.clear_modified();
                            self.tabs[self.active_tab].raw_content_modified = false;
                            self.do_open_file_dialog();
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_open_confirm = false;
                            self.pending_open_file = false;
                        }
                    });
                });
        }

        // --- Table picker (multi-table DB sources) ---
        if let Some(state) = self.pending_table_picker.as_mut() {
            let action = ui::table_picker::render_table_picker(ctx, state);
            match action {
                ui::table_picker::TablePickerAction::None => {}
                ui::table_picker::TablePickerAction::Cancel => {
                    self.pending_table_picker = None;
                }
                ui::table_picker::TablePickerAction::Open(path, table_name) => {
                    self.pending_table_picker = None;
                    self.load_table(path, table_name);
                }
            }
        }

        // --- Settings dialog ---
        if let Some(new_settings) = self.settings_dialog.show(ctx, self.logo_texture.as_ref()) {
            let icon_changed = self.settings_dialog.icon_changed;
            let font_changed = self.settings_dialog.font_changed;
            let theme_changed = self.settings_dialog.theme_changed;

            self.settings = new_settings;
            self.settings.save();

            if theme_changed {
                self.theme_mode = self.settings.default_theme;
            }
            if font_changed || theme_changed {
                self.apply_zoom(ctx);
            }
            if icon_changed {
                // Refresh the toolbar logo texture
                let svg_src = self.settings.icon_variant.svg_source();
                let opt = resvg::usvg::Options::default();
                if let Ok(tree) = resvg::usvg::Tree::from_str(svg_src, &opt) {
                    let size = tree.size();
                    let (w, h) = (size.width() as u32, size.height() as u32);
                    if let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(w, h) {
                        resvg::render(
                            &tree,
                            resvg::tiny_skia::Transform::default(),
                            &mut pixmap.as_mut(),
                        );
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [w as usize, h as usize],
                            pixmap.data(),
                        );
                        self.logo_texture = Some(ctx.load_texture(
                            "octa_logo",
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
                // Re-render welcome logo at high resolution
                self.welcome_logo_texture = None;

                // Update the window icon
                let icon = render_icon(svg_src);
                ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(Arc::new(icon))));

                // Update desktop icon on Linux
                #[cfg(target_os = "linux")]
                {
                    let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);

                    // Always write to user-local icon path (create dirs if needed)
                    if let Some(ref h) = home {
                        let local_icon_path =
                            h.join(".local/share/icons/hicolor/scalable/apps/octa.svg");
                        if let Some(parent) = local_icon_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let _ = std::fs::write(&local_icon_path, svg_src);
                    }

                    // Also try system paths if they already exist
                    for path in &[
                        "/usr/share/icons/hicolor/scalable/apps/octa.svg",
                        "/usr/local/share/icons/hicolor/scalable/apps/octa.svg",
                    ] {
                        let p = std::path::Path::new(path);
                        if p.exists() {
                            let _ = std::fs::write(p, svg_src);
                        }
                    }

                    // Refresh icon caches (GTK, XDG, KDE)
                    if let Some(ref h) = home {
                        let local_hicolor = h.join(".local/share/icons/hicolor");
                        let _ = std::process::Command::new("gtk-update-icon-cache")
                            .args(["-f", "-t"])
                            .arg(&local_hicolor)
                            .spawn();
                    }
                    let _ = std::process::Command::new("xdg-icon-resource")
                        .arg("forceupdate")
                        .spawn();
                    if let Some(ref h) = home {
                        let local_apps = h.join(".local/share/applications");
                        if local_apps.exists() {
                            let _ = std::process::Command::new("update-desktop-database")
                                .arg(&local_apps)
                                .spawn();
                        }
                    }
                    // KDE Plasma: rebuild sycoca cache so taskbar picks up the new icon
                    for cmd in &["kbuildsycoca6", "kbuildsycoca5"] {
                        if std::process::Command::new(cmd)
                            .arg("--noincremental")
                            .spawn()
                            .is_ok()
                        {
                            break;
                        }
                    }
                }
            }
        }

        // --- Documentation dialog ---
        if self.show_documentation_dialog {
            let mut open = self.show_documentation_dialog;
            egui::Window::new("Documentation")
                .open(&mut open)
                .resizable(true)
                .collapsible(true)
                .default_size([800.0, 600.0])
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let docs = r#"# Octa Documentation

## Getting Started

Open a file using **File > Open** (or **Ctrl+O**), or pass a file path as a command-line argument:

```
octa path/to/file.parquet
```

**Supported formats:** Parquet, CSV, TSV, JSON, JSONL, Excel (.xlsx), Avro, Arrow IPC, ORC, HDF5, XML, TOML, YAML, PDF, Markdown, Plain Text.

All formats support both reading and writing. When saving, the original format and settings (e.g. CSV delimiter) are preserved.

## Navigation

- **Arrow keys** move the selected cell up, down, left, or right
- **Scroll wheel** scrolls the table vertically
- **Shift + Scroll wheel** scrolls horizontally
- Click a **row number** to select the entire row (Ctrl+click to add to selection, Shift+click for range)
- Click a **column header** to select the entire column
- **Ctrl+A** selects all rows

## Editing

- **Double-click** a cell to start editing — the current text is selected so you can type to replace it, or click to position the cursor
- Click outside the cell or press **Tab** to confirm the edit
- **Escape** cancels the current edit
- **Ctrl+Z** to undo, **Ctrl+Y** to redo — works for cell edits, row/column operations, and color marks
- **Edit > Insert Row** adds a new empty row below the selected cell
- **Edit > Insert Column** opens a dialog to add a column (choose name and type)
- **Edit > Delete Row / Delete Column** removes the selected row or column
- **Edit > Move Row Up/Down** and **Move Column Left/Right** reorder data
- **Edit > Discard All Edits** reverts all unsaved changes
- **Drag a column header** to reorder columns via drag-and-drop
- **Double-click a column header** to rename it inline
- **Right-click a column header** to change the column data type

## Formulas

Cells support simple Excel-like formulas starting with **=**. Supported features:

- **Cell references**: A1, B2, AA1, etc. (column letter + row number, 1-based — column letters are shown in each column header)
- **Operators**: `+`, `-`, `*`, `/`
- **Parentheses**: `(A1 + B1) * 2`
- **Numeric literals**: `=A1 * 1.5`

Examples: `=A1+B1`, `=C1*2`, `=(A1+B1)/C1`

When inserting a new column via **Edit > Insert Column**, you can specify a formula in the **Formula** field. The formula acts as a template using row 1 references — it is automatically applied to every row (e.g. `=A1+B1` fills row 3 with `=A3+B3`).

Division by zero returns no result (the cell stays empty).

## Search & Filter

Use the search box in the toolbar to filter rows in real-time. Only rows containing a match are displayed.

Three search modes are available (selectable via the dropdown next to the search box):

- **Plain**: case-insensitive substring match
- **Wildcard**: `*` matches any sequence of characters, `?` matches one character
- **Regex**: full regular expression syntax

Use **Ctrl+F** to focus the search box from anywhere.

## Find & Replace

Open the replace bar with **Ctrl+H** or via **Search > Find & Replace**.

Type a search term and a replacement value, then:

- **Next** replaces the first matching cell value found in the table
- **All** replaces every matching cell value across all visible rows

Press **Escape** to close the replace bar.

## Color Marking

Right-click a **cell**, **row number**, or **column header** to open the context menu, then use the **Mark** submenu. Available colors: Red, Orange, Yellow, Green, Blue, Purple.

Mark precedence: cell marks take priority over row marks, which take priority over column marks.

To clear a mark, right-click and select **Clear Mark** from the context menu.

## View Modes

Switch between views using the **View** menu:

- **Table View** (default): structured tabular display with sorting, filtering, and editing
- **Raw Text**: shows the raw file content as plain text (available for text-based formats)
- **PDF View**: rendered page view (available for PDF files)
- **Markdown View**: rendered markdown (available for .md files)

## Tabs and Folder Sidebar

Every opened file is shown as a tab, even when only one file is open. Hovering a tab reveals the full file path — handy when several tabs share a file name.

**File > Open Directory…** opens a folder browser docked as a sidebar (left by default — switch to the right via **Settings > Directory Tree**). Clicking any file in the tree opens it in a new tab. **File > Close Directory** hides the sidebar without touching the open tabs.

## SQL Autocomplete and Case Conversion

In the SQL editor:

- As you type, a strip of suggestion chips appears below the editor with matching column names and SQL keywords. Click a chip to complete the current token. Toggle this off under **Settings > SQL > Autocomplete** (on by default).
- The **UPPER** / **lower** buttons (and the right-click context menu) convert the current selection, or the whole query when nothing is selected.

The same upper / lower case context menu is available in the Raw Text editor.

## Column Insertion Autofill

When typing a name in **Insert Column**, matching existing column names are shown as clickable chips — click to fill the name field.

## Saving

- **File > Save** writes changes back to the original file (preserves format and settings)
- **File > Save As** lets you save to a new file, optionally in a different format
- If you have unsaved changes and try to open a new file or close the application, a confirmation dialog appears

## Settings

Open **Help > Settings** to configure:

- **Font size**: adjusts text size across the entire application including table content
- **Default theme**: Light or Dark mode
- **Icon color**: choose from 12 color variants for the application icon
- **Default search mode**: which search mode is active by default (Plain, Wildcard, or Regex)
- **Show row numbers**: toggle the row number column on the left
- **Alternating row colors**: toggle zebra-stripe row backgrounds for easier reading
- **Negative numbers in red**: display negative numeric values in red
- **Highlight edited cells**: show a yellow background on cells that have been modified (off by default)

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+N | New file |
| Ctrl+O | Open file |
| Ctrl+S | Save file |
| Ctrl+F | Focus search box |
| Ctrl+H | Toggle Find & Replace bar |
| Ctrl+Z | Undo |
| Ctrl+Y | Redo |
| Ctrl+C | Copy selection |
| Ctrl+V | Paste |
| Ctrl+A | Select all rows |
| Arrow keys | Navigate between cells |
| Escape | Close replace bar or cancel cell edit |
| Double-click cell | Edit cell value |
| Double-click column header | Rename column |
| Right-click | Open context menu |
"#;
                        egui_commonmark::CommonMarkViewer::new()
                            .show(ui, &mut self.tabs[self.active_tab].commonmark_cache, docs);
                    });
                });
            self.show_documentation_dialog = open;
        }

        // --- Un-align confirmation ---
        if self.show_unalign_confirm {
            let mut confirm = false;
            let mut cancel = false;
            egui::Window::new("Discard aligned edits?")
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(
                        "Turning off 'Align Columns' reloads the file from disk.\n\
                         Unsaved changes in the raw view will be lost.",
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Reload and discard").clicked() {
                            confirm = true;
                        }
                        if ui.button("Keep aligned").clicked() {
                            cancel = true;
                        }
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(
                                "(You can disable this warning in Settings → File-Specific.)",
                            )
                            .weak()
                            .size(11.0),
                        );
                    });
                });
            if confirm {
                let tab = &mut self.tabs[self.active_tab];
                if let (Some(content), Some(path)) =
                    (tab.raw_content.as_mut(), tab.table.source_path.clone())
                {
                    if let Ok(original) = std::fs::read_to_string(&path) {
                        *content = original;
                        tab.raw_content_modified = false;
                        tab.raw_view_formatted = false;
                    }
                }
                self.show_unalign_confirm = false;
            } else if cancel {
                self.show_unalign_confirm = false;
            }
        }

        // --- Reload confirm dialog ---
        if self.show_reload_confirm {
            let mut confirm = false;
            let mut cancel = false;
            egui::Window::new("Discard unsaved changes?")
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(
                        "Reloading will replace your current edits with the contents on disk.",
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Reload and discard").clicked() {
                            confirm = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            if confirm {
                self.show_reload_confirm = false;
                self.reload_active_file();
            } else if cancel {
                self.show_reload_confirm = false;
            }
        }

        // --- About dialog ---
        if self.show_about_dialog {
            egui::Window::new("About Octa")
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Octa").strong().size(20.0));
                        ui.add_space(4.0);
                        ui.label(format!("Version {}", VERSION));
                        ui.add_space(8.0);
                        ui.label(format!("Author: {}", AUTHORS));
                        ui.add_space(4.0);
                        if ui.hyperlink_to("GitHub Repository", REPOSITORY).clicked() {
                            // egui opens the link automatically
                        }
                        ui.add_space(12.0);
                        if ui.button("Close").clicked() {
                            self.show_about_dialog = false;
                        }
                    });
                });
        }

        // --- Update dialog ---
        if self.show_update_dialog {
            egui::Window::new("Check for Updates")
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let state = self.update_state.lock().unwrap().clone();
                    match state {
                        UpdateState::Idle | UpdateState::Checking => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Checking for updates...");
                            });
                        }
                        UpdateState::UpToDate => {
                            ui.label(format!(
                                "You are running the latest version ({}).",
                                VERSION
                            ));
                            ui.add_space(8.0);
                            if ui.button("Close").clicked() {
                                self.show_update_dialog = false;
                                *self.update_state.lock().unwrap() = UpdateState::Idle;
                            }
                        }
                        UpdateState::Available(ref new_version) => {
                            ui.label(format!(
                                "A new version is available: {} (current: {})",
                                new_version, VERSION
                            ));
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                let version = new_version.clone();
                                if ui.button("Update Now").clicked() {
                                    self.perform_update(&version, ctx);
                                }
                                if ui.button("Cancel").clicked() {
                                    self.show_update_dialog = false;
                                    *self.update_state.lock().unwrap() = UpdateState::Idle;
                                }
                            });
                        }
                        UpdateState::Updating => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Downloading and installing update...");
                            });
                        }
                        UpdateState::Updated(ref version) => {
                            ui.label(format!(
                                "Updated to version {}. Please restart Octa to use the new version.",
                                version
                            ));
                            ui.add_space(8.0);
                            if ui.button("Close").clicked() {
                                self.show_update_dialog = false;
                                *self.update_state.lock().unwrap() = UpdateState::Idle;
                            }
                        }
                        UpdateState::Error(ref msg) => {
                            ui.label(format!("Update check failed: {}", msg));
                            ui.add_space(8.0);
                            if ui.button("Close").clicked() {
                                self.show_update_dialog = false;
                                *self.update_state.lock().unwrap() = UpdateState::Idle;
                            }
                        }
                    }
                });
        }

        // Bottom status bar — share the toolbar's framed look.
        let status_colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        let status_frame = egui::Frame::new()
            .fill(status_colors.bg_header)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .stroke(egui::Stroke::new(1.0, status_colors.border_subtle));
        let status_action = egui::TopBottomPanel::bottom("status_bar")
            .exact_height(28.0)
            .frame(status_frame)
            .show(ctx, |ui| {
                ui::status_bar::draw_status_bar(
                    ui,
                    &self.tabs[self.active_tab].table,
                    &self.tabs[self.active_tab].table_state,
                    self.theme_mode,
                    filtered_count,
                    search_active,
                    &mut self.nav_input,
                    std::mem::take(&mut self.nav_focus_requested),
                    self.zoom_percent,
                )
            })
            .inner;

        // Handle navigation from status bar
        if let Some((row, col)) = status_action.navigate_to {
            let tab = &mut self.tabs[self.active_tab];
            tab.table_state.selected_cell = Some((row, col));
            tab.table_state.selected_rows.clear();
            tab.table_state.selected_cols.clear();
            // Auto-scroll to the target cell
            let row_height =
                (self.settings.font_size * self.zoom_percent as f32 / 100.0 * 2.0).max(26.0);
            tab.table_state.set_scroll_y(row as f32 * row_height);
            let col_left: f32 = tab.table_state.col_widths[..col].iter().sum();
            tab.table_state.set_scroll_x(col_left);
        }

        // SQL editor + result panel (rendered before CentralPanel so the
        // table fills the remaining space). Only meaningful while viewing
        // the table itself — collapse for non-tabular view modes.
        let sql_panel_visible = {
            let tab = &self.tabs[self.active_tab];
            tab.sql_panel_open && tab.table.col_count() > 0 && tab.view_mode == ViewMode::Table
        };
        if sql_panel_visible {
            let position = self.settings.sql_panel_position;
            let mut sql_action = view_modes::SqlAction::default();
            let tab = &mut self.tabs[self.active_tab];
            let partial_rows = tab.table.total_rows.and_then(|total| {
                let loaded = tab.table.row_count();
                if loaded < total {
                    Some((loaded, total))
                } else {
                    None
                }
            });
            let render = |ui: &mut egui::Ui,
                          tab: &mut TabState,
                          autocomplete: bool,
                          row_limit: usize|
             -> view_modes::SqlAction {
                view_modes::render_sql_view(
                    ui,
                    tab,
                    autocomplete,
                    row_limit,
                    position,
                    partial_rows,
                )
            };
            let autocomplete = self.settings.sql_autocomplete;
            let row_limit = self.settings.sql_default_row_limit;
            match position {
                ui::settings::SqlPanelPosition::Bottom => {
                    egui::TopBottomPanel::bottom("sql_panel")
                        .resizable(true)
                        .default_height(280.0)
                        .min_height(140.0)
                        .show(ctx, |ui| {
                            sql_action = render(ui, tab, autocomplete, row_limit);
                        });
                }
                ui::settings::SqlPanelPosition::Top => {
                    egui::TopBottomPanel::top("sql_panel")
                        .resizable(true)
                        .default_height(280.0)
                        .min_height(140.0)
                        .show(ctx, |ui| {
                            sql_action = render(ui, tab, autocomplete, row_limit);
                        });
                }
                ui::settings::SqlPanelPosition::Left => {
                    egui::SidePanel::left("sql_panel")
                        .resizable(true)
                        .default_width(440.0)
                        .min_width(280.0)
                        .show(ctx, |ui| {
                            sql_action = render(ui, tab, autocomplete, row_limit);
                        });
                }
                ui::settings::SqlPanelPosition::Right => {
                    egui::SidePanel::right("sql_panel")
                        .resizable(true)
                        .default_width(440.0)
                        .min_width(280.0)
                        .show(ctx, |ui| {
                            sql_action = render(ui, tab, autocomplete, row_limit);
                        });
                }
            }
            if sql_action.clear {
                let tab = &mut self.tabs[self.active_tab];
                tab.sql_result = None;
                tab.sql_error = None;
            }
            if sql_action.run {
                let tab = &mut self.tabs[self.active_tab];
                let query = tab.sql_query.clone();
                let mut snapshot = tab.table.clone();
                snapshot.apply_edits();
                match octa::sql::run_query(&snapshot, &query) {
                    Ok(outcome) => match outcome.kind {
                        octa::sql::QueryKind::Select => {
                            tab.sql_result = Some(outcome.table);
                            tab.sql_error = None;
                        }
                        octa::sql::QueryKind::Mutation => {
                            // Apply the mutation to the base table directly so
                            // INSERT / UPDATE / DELETE affect the data, not just
                            // a result set. Selection / widths / per-tab UI state
                            // are reset because row/column identity may have changed.
                            tab.table = outcome.table;
                            tab.table_state = TableViewState::default();
                            tab.filter_dirty = true;
                            tab.sql_result = None;
                            tab.sql_error = None;
                            let rows = tab.table.row_count();
                            let affected = outcome.affected.unwrap_or(0);
                            self.status_message = Some((
                                format!(
                                    "SQL applied: {} row(s) affected — table now {} row(s)",
                                    affected, rows
                                ),
                                std::time::Instant::now(),
                            ));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                                self.tabs[self.active_tab].title_display(),
                            ));
                        }
                    },
                    Err(e) => {
                        tab.sql_error = Some(e.to_string());
                    }
                }
            }
            if sql_action.export {
                self.export_sql_result();
            }
        }

        // Central panel: table view or raw text view
        egui::CentralPanel::default().show(ctx, |ui| {
            // Show status message if any
            if let Some((ref msg, instant)) = self.status_message {
                if instant.elapsed().as_secs() < 10 {
                    let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
                    let color = if msg.starts_with("Saved") {
                        colors.success
                    } else {
                        colors.error
                    };
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(msg).color(color).size(12.0));
                    });
                    ui.add_space(4.0);
                }
            }

            // Recompute filter before drawing (in case it was dirtied by toolbar actions)
            if self.tabs[self.active_tab].filter_dirty {
                self.recompute_filter();
            }

            // --- PDF rendered view ---
            if self.tabs[self.active_tab].view_mode == ViewMode::Pdf {
                view_modes::render_pdf_view(
                    ctx,
                    ui,
                    &mut self.tabs[self.active_tab],
                    self.theme_mode,
                );
                return;
            }

            // --- Jupyter Notebook rendered view ---
            if self.tabs[self.active_tab].view_mode == ViewMode::Notebook {
                view_modes::render_notebook_view(
                    ctx,
                    ui,
                    &self.tabs[self.active_tab],
                    self.theme_mode,
                    self.settings.notebook_output_layout,
                );
                return;
            }

            // --- Markdown rendered view ---
            if self.tabs[self.active_tab].view_mode == ViewMode::Markdown {
                view_modes::render_markdown_view(ui, &mut self.tabs[self.active_tab]);
                return;
            }

            // --- Raw text view ---
            if self.tabs[self.active_tab].view_mode == ViewMode::Raw {
                let raw_action = view_modes::render_raw_view(
                    ui,
                    &mut self.tabs[self.active_tab],
                    self.theme_mode,
                    self.settings.color_aligned_columns,
                    self.settings.tab_size,
                    self.settings.warn_raw_align_reload,
                );
                if raw_action.confirm_unalign {
                    self.show_unalign_confirm = true;
                }
                return;
            }

            // --- JSON tree view ---
            if self.tabs[self.active_tab].view_mode == ViewMode::JsonTree {
                view_modes::render_json_tree_view(
                    ui,
                    &mut self.tabs[self.active_tab],
                    self.theme_mode,
                );
                return;
            }

            // --- Table view ---
            let os_has_clipboard = self.os_clipboard_has_text();
            let tab = &mut self.tabs[self.active_tab];
            let filtered = tab.filtered_rows.clone();
            let os_has_clip = tab.table_state.clipboard.is_some() || os_has_clipboard;
            let interaction = ui::table_view::draw_table(
                ui,
                &mut tab.table,
                &mut tab.table_state,
                self.theme_mode,
                &filtered,
                os_has_clip,
                self.settings.show_row_numbers,
                self.settings.alternating_row_colors,
                self.settings.negative_numbers_red,
                self.settings.highlight_edits,
                self.settings.font_size * self.zoom_percent as f32 / 100.0,
                self.settings.cell_line_breaks,
                self.settings.binary_display_mode,
                self.welcome_logo_texture.as_ref(),
                &self.settings.shortcuts,
            );

            // Handle column header click: update insert position for "Add Column" dialog
            let tab = &mut self.tabs[self.active_tab];
            if let Some(col_idx) = interaction.header_col_clicked {
                tab.insert_col_at = Some(col_idx + 1);
                if let Some((row, _)) = tab.table_state.selected_cell {
                    tab.table_state.selected_cell = Some((row, col_idx));
                }
            }

            // Handle drag-and-drop column move
            if let Some((from, to)) = interaction.col_drag_move {
                tab.table.move_column(from, to);
                if let Some((row, col)) = tab.table_state.selected_cell {
                    let new_col = if col == from {
                        to
                    } else if from < to {
                        if col > from && col <= to {
                            col - 1
                        } else {
                            col
                        }
                    } else {
                        if col >= to && col < from {
                            col + 1
                        } else {
                            col
                        }
                    };
                    tab.table_state.selected_cell = Some((row, new_col));
                }
                if from < tab.table_state.col_widths.len() && to < tab.table_state.col_widths.len()
                {
                    let w = tab.table_state.col_widths.remove(from);
                    tab.table_state.col_widths.insert(to, w);
                }
                tab.filter_dirty = true;
            }

            // Handle column rename
            let tab = &mut self.tabs[self.active_tab];
            if let Some((col_idx, new_name)) = interaction.rename_column {
                if col_idx < tab.table.columns.len() && !new_name.is_empty() {
                    tab.table.columns[col_idx].name = new_name;
                    tab.table.structural_changes = true;
                    tab.table_state.widths_initialized = false;
                }
            }

            // Handle column type change (convert actual cell values)
            if let Some((col_idx, new_type)) = interaction.change_col_type {
                if !tab.table.convert_column(col_idx, &new_type) {
                    self.status_message = Some((
                        format!(
                            "Cannot convert column to {new_type}: some values are incompatible"
                        ),
                        std::time::Instant::now(),
                    ));
                }
            }

            // Sort rows by column (from table header arrows or context menu)
            let tab = &mut self.tabs[self.active_tab];
            if let Some(col_idx) = interaction.sort_rows_asc_by {
                tab.table.sort_rows_by_column(col_idx, true);
                tab.filter_dirty = true;
            }
            if let Some(col_idx) = interaction.sort_rows_desc_by {
                tab.table.sort_rows_by_column(col_idx, false);
                tab.filter_dirty = true;
            }

            // --- Context menu: row operations ---
            if interaction.ctx_insert_row {
                let insert_at = match tab.table_state.selected_cell {
                    Some((row, _)) => row + 1,
                    None => tab.table.row_count(),
                };
                tab.table.insert_row(insert_at);
                let sel_col = tab.table_state.selected_cell.map(|(_, c)| c).unwrap_or(0);
                tab.table_state.selected_cell = Some((insert_at, sel_col));
                tab.table_state.editing_cell = None;
                tab.filter_dirty = true;
            }
            if interaction.ctx_delete_row {
                if let Some((row, col)) = tab.table_state.selected_cell {
                    tab.table.delete_row(row);
                    tab.table_state.editing_cell = None;
                    if tab.table.row_count() == 0 {
                        tab.table_state.selected_cell = None;
                    } else {
                        let new_row = row.min(tab.table.row_count() - 1);
                        tab.table_state.selected_cell = Some((new_row, col));
                    }
                    tab.filter_dirty = true;
                }
            }
            if interaction.ctx_move_row_up {
                if let Some((row, col)) = tab.table_state.selected_cell {
                    if row > 0 {
                        tab.table.move_row(row, row - 1);
                        tab.table_state.selected_cell = Some((row - 1, col));
                        tab.filter_dirty = true;
                    }
                }
            }
            if interaction.ctx_move_row_down {
                if let Some((row, col)) = tab.table_state.selected_cell {
                    if row + 1 < tab.table.row_count() {
                        tab.table.move_row(row, row + 1);
                        tab.table_state.selected_cell = Some((row + 1, col));
                        tab.filter_dirty = true;
                    }
                }
            }

            // --- Context menu: column operations ---
            if interaction.ctx_insert_column {
                tab.show_add_column_dialog = true;
                tab.new_col_name.clear();
                tab.new_col_type = "String".to_string();
                tab.new_col_formula.clear();
                tab.insert_col_at = tab.table_state.selected_cell.map(|(_, c)| c + 1);
            }
            if interaction.ctx_delete_column && tab.table.col_count() > 0 {
                self.open_delete_columns_dialog();
            }
            if interaction.ctx_move_col_left {
                let tab = &mut self.tabs[self.active_tab];
                if let Some((row, col)) = tab.table_state.selected_cell {
                    if col > 0 {
                        tab.table.move_column(col, col - 1);
                        tab.table_state.selected_cell = Some((row, col - 1));
                        tab.table_state.widths_initialized = false;
                    }
                }
            }
            if interaction.ctx_move_col_right {
                let tab = &mut self.tabs[self.active_tab];
                if let Some((row, col)) = tab.table_state.selected_cell {
                    if col + 1 < tab.table.col_count() {
                        tab.table.move_column(col, col + 1);
                        tab.table_state.selected_cell = Some((row, col + 1));
                        tab.table_state.widths_initialized = false;
                    }
                }
            }

            // --- Copy / Paste ---
            let tab = &mut self.tabs[self.active_tab];
            if interaction.ctx_copy_cell {
                if let Some((row, col)) = tab.table_state.selected_cell {
                    let text = tab
                        .table
                        .get(row, col)
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    tab.table_state.clipboard = Some(text.clone());
                    if let Some(ref cb) = self.os_clipboard {
                        if let Ok(mut cb) = cb.lock() {
                            let _ = cb.set_text(&text);
                        }
                    }
                }
            }
            if interaction.ctx_copy {
                self.do_copy();
            }
            if interaction.ctx_paste {
                self.do_paste(interaction.paste_text);
            }

            // --- Undo / Redo ---
            let tab = &mut self.tabs[self.active_tab];
            if interaction.undo {
                tab.table.undo();
                tab.filter_dirty = true;
                tab.table_state.widths_initialized = false;
            }
            if interaction.redo {
                tab.table.redo();
                tab.filter_dirty = true;
                tab.table_state.widths_initialized = false;
            }

            // --- Color marks ---
            if let Some((key, color)) = interaction.set_mark {
                tab.table.set_mark(key, color);
            }
            if let Some(key) = interaction.clear_mark {
                tab.table.clear_mark(key);
            }

            // --- Lazy loading: load more rows on demand ---
            if interaction.needs_more_rows
                && tab.bg_can_load_more
                && tab.bg_row_buffer.is_none()
                && tab.table.total_rows.is_some()
            {
                tab.bg_can_load_more = false;
                let buffer = Arc::new(Mutex::new(Vec::<Vec<data::CellValue>>::new()));
                let done_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let exhausted_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
                tab.bg_row_buffer = Some(buffer.clone());
                tab.bg_loading_done = done_flag.clone();
                tab.bg_file_exhausted = exhausted_flag.clone();

                let skip_rows = tab.table.row_offset + tab.table.row_count();
                let max_chunk = 1_000_000usize;

                if let Some(ref source_path) = tab.table.source_path.clone() {
                    let path = std::path::PathBuf::from(source_path);
                    let format_name = tab.table.format_name.clone().unwrap_or_default();
                    let num_cols = tab.table.col_count();
                    let csv_delimiter = tab.csv_delimiter;

                    if format_name == "Parquet" {
                        std::thread::spawn(move || {
                            if let Err(e) = load_remaining_parquet_rows(
                                &path,
                                skip_rows,
                                max_chunk,
                                buffer.clone(),
                                done_flag,
                                exhausted_flag,
                            ) {
                                eprintln!("Background loading error: {}", e);
                            }
                        });
                    } else if format_name == "CSV" || format_name == "TSV" {
                        let delimiter = if format_name == "TSV" {
                            b'\t'
                        } else {
                            csv_delimiter
                        };
                        std::thread::spawn(move || {
                            if let Err(e) = formats::csv_reader::load_csv_rows_chunk(
                                &path,
                                delimiter,
                                skip_rows,
                                max_chunk,
                                num_cols,
                                buffer,
                                done_flag,
                                exhausted_flag,
                            ) {
                                eprintln!("Background CSV loading error: {}", e);
                            }
                        });
                    }
                }
            }
        });
    }
}
