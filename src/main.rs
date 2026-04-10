#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use octa::data::{self, DataTable, SearchMode, ViewMode};
use octa::formats::{self, FormatRegistry};
use octa::ui;
use ui::settings::{AppSettings, SettingsDialog};
use ui::table_view::TableViewState;
use ui::theme::ThemeMode;

use eframe::egui;
use egui::{Align, Color32, Layout, RichText, Stroke};

use std::sync::{Arc, Mutex};

/// Precompiled matcher for the current search query and mode.
enum RowMatcher {
    Plain(String),
    Regex(regex::Regex),
    Invalid,
}

impl RowMatcher {
    fn new(query: &str, mode: SearchMode) -> Self {
        match mode {
            SearchMode::Plain => RowMatcher::Plain(query.to_lowercase()),
            SearchMode::Wildcard => {
                let pattern = data::wildcard_to_regex(query);
                match regex::Regex::new(&pattern) {
                    Ok(re) => RowMatcher::Regex(re),
                    Err(_) => RowMatcher::Invalid,
                }
            }
            SearchMode::Regex => match regex::Regex::new(query) {
                Ok(re) => RowMatcher::Regex(re),
                Err(_) => RowMatcher::Invalid,
            },
        }
    }

    fn matches(&self, text: &str) -> bool {
        match self {
            RowMatcher::Plain(q) => text.to_lowercase().contains(q),
            RowMatcher::Regex(re) => re.is_match(text),
            RowMatcher::Invalid => false,
        }
    }

    /// Replace matching portion(s) in `text` with `replacement`.
    fn replace(&self, text: &str, replacement: &str) -> String {
        match self {
            RowMatcher::Plain(q) => {
                // Case-insensitive replacement: find the match position and replace preserving structure
                let lower = text.to_lowercase();
                if let Some(pos) = lower.find(q.as_str()) {
                    let mut result = String::with_capacity(text.len());
                    result.push_str(&text[..pos]);
                    result.push_str(replacement);
                    result.push_str(&text[pos + q.len()..]);
                    result
                } else {
                    text.to_string()
                }
            }
            RowMatcher::Regex(re) => re.replace(text, replacement).to_string(),
            RowMatcher::Invalid => text.to_string(),
        }
    }
}

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
            .with_inner_size([3840.0, 2160.0])
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
            ui::theme::apply_theme(&cc.egui_ctx, default_theme, settings.font_size);
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

struct OctaApp {
    table: DataTable,
    registry: FormatRegistry,
    theme_mode: ThemeMode,
    settings: AppSettings,
    settings_dialog: SettingsDialog,
    table_state: TableViewState,
    search_text: String,
    search_mode: data::SearchMode,
    /// Whether the search text field should be focused next frame.
    search_focus_requested: bool,
    /// Whether the search & replace bar is visible.
    show_replace_bar: bool,
    /// Replacement text for search & replace.
    replace_text: String,
    filtered_rows: Vec<usize>,
    filter_dirty: bool,
    status_message: Option<(String, std::time::Instant)>,
    /// "Add Column" dialog state
    show_add_column_dialog: bool,
    new_col_name: String,
    new_col_type: String,
    /// Formula for populating a new column (e.g. "=A1+B1")
    new_col_formula: String,
    /// Column index to insert at (None = append at end)
    insert_col_at: Option<usize>,
    /// "Delete Columns" dialog state
    show_delete_columns_dialog: bool,
    /// Checkbox state per column (true = marked for deletion)
    delete_col_selection: Vec<bool>,
    /// "Unsaved changes" dialog state
    show_close_confirm: bool,
    /// Whether we already decided to quit (skip further confirm)
    confirmed_close: bool,
    /// System clipboard handle (shared, lazily initialized)
    os_clipboard: Option<Arc<Mutex<arboard::Clipboard>>>,
    /// Current view mode (Table, Raw, or Pdf)
    view_mode: ViewMode,
    /// Raw file content for text-based formats
    raw_content: Option<String>,
    /// Whether raw content has been modified
    raw_content_modified: bool,
    /// Rendered PDF page images (loaded on file open, textures created lazily)
    pdf_page_images: Vec<egui::ColorImage>,
    /// Texture handles for rendered PDF pages
    pdf_textures: Vec<egui::TextureHandle>,
    /// Extracted text per PDF page (for copy support)
    pdf_page_texts: Vec<String>,
    /// Logo texture for toolbar
    logo_texture: Option<egui::TextureHandle>,
    /// Whether raw view shows aligned/formatted columns
    raw_view_formatted: bool,
    /// CSV delimiter used for current file
    csv_delimiter: u8,
    /// Background loading: shared buffer for incoming rows
    bg_row_buffer: Option<Arc<Mutex<Vec<Vec<data::CellValue>>>>>,
    /// Background loading: flag indicating loading is complete
    bg_loading_done: Arc<std::sync::atomic::AtomicBool>,
    /// Whether more rows can be loaded on demand (file has more rows than currently loaded)
    bg_can_load_more: bool,
    /// Set by background loader when file has no more rows
    bg_file_exhausted: Arc<std::sync::atomic::AtomicBool>,
    /// File path passed via command line (loaded on first frame)
    initial_file: Option<std::path::PathBuf>,
    /// Pending file to open after unsaved-changes dialog resolves
    pending_open_file: bool,
    /// Show unsaved-changes dialog before opening a new file
    show_open_confirm: bool,
    /// Cache for commonmark rendering
    commonmark_cache: egui_commonmark::CommonMarkCache,
    /// Show the About dialog
    show_about_dialog: bool,
    /// Show the Documentation dialog
    show_documentation_dialog: bool,
    /// Show the Update dialog
    show_update_dialog: bool,
    /// Update check state shared with background thread
    update_state: Arc<Mutex<UpdateState>>,
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

fn format_delimited_text(content: &str, delimiter: char) -> String {
    let lines: Vec<Vec<&str>> = content
        .lines()
        .map(|line| line.split(delimiter).collect())
        .collect();
    if lines.is_empty() {
        return content.to_string();
    }
    let max_cols = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; max_cols];
    for line in &lines {
        for (i, cell) in line.iter().enumerate() {
            widths[i] = widths[i].max(cell.trim().len());
        }
    }
    lines
        .iter()
        .map(|line| {
            line.iter()
                .enumerate()
                .map(|(i, cell)| {
                    let trimmed = cell.trim();
                    if i < line.len() - 1 {
                        format!("{:<width$}", trimmed, width = widths[i])
                    } else {
                        trimmed.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(&format!("{} ", delimiter))
        })
        .collect::<Vec<_>>()
        .join("\n")
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
        Self {
            table: DataTable::empty(),
            registry: FormatRegistry::new(),
            theme_mode,
            settings,
            settings_dialog: SettingsDialog::default(),
            table_state: TableViewState::default(),
            search_text: String::new(),
            search_mode,
            search_focus_requested: false,
            show_replace_bar: false,
            replace_text: String::new(),
            filtered_rows: Vec::new(),
            filter_dirty: true,
            status_message: None,
            show_add_column_dialog: false,
            new_col_name: String::new(),
            new_col_type: "String".to_string(),
            new_col_formula: String::new(),
            insert_col_at: None,
            show_delete_columns_dialog: false,
            delete_col_selection: Vec::new(),
            show_close_confirm: false,
            confirmed_close: false,
            os_clipboard: arboard::Clipboard::new()
                .ok()
                .map(|c| Arc::new(Mutex::new(c))),
            view_mode: ViewMode::Table,
            raw_content: None,
            raw_content_modified: false,
            pdf_page_images: Vec::new(),
            pdf_textures: Vec::new(),
            pdf_page_texts: Vec::new(),
            logo_texture: None,
            raw_view_formatted: false,
            csv_delimiter: b',',
            bg_row_buffer: None,
            bg_loading_done: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            bg_can_load_more: false,
            bg_file_exhausted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            initial_file,
            pending_open_file: false,
            show_open_confirm: false,
            commonmark_cache: egui_commonmark::CommonMarkCache::default(),
            show_about_dialog: false,
            show_documentation_dialog: false,
            show_update_dialog: false,
            update_state: Arc::new(Mutex::new(UpdateState::Idle)),
        }
    }

    /// Build a tab-separated string from the current selection.
    /// Priority: selected_rows > selected_cols > selected_cell.
    fn copy_selection_to_string(&self) -> Option<String> {
        let state = &self.table_state;

        if !state.selected_rows.is_empty() {
            // Copy selected rows (all columns)
            let mut rows: Vec<usize> = state.selected_rows.iter().copied().collect();
            rows.sort();
            let mut lines = Vec::new();
            for row in rows {
                let mut cells = Vec::new();
                for col in 0..self.table.col_count() {
                    let text = self
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
            for row in 0..self.table.row_count() {
                let mut cells = Vec::new();
                for &col in &cols {
                    let text = self
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
            let text = self
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

        let (start_row, start_col) = self.table_state.selected_cell.unwrap_or((0, 0));

        for (ri, row_cells) in parsed_rows.iter().enumerate() {
            let target_row = start_row + ri;
            if target_row >= self.table.row_count() {
                break;
            }
            for (ci, &cell_text) in row_cells.iter().enumerate() {
                let target_col = start_col + ci;
                if target_col >= self.table.col_count() {
                    break;
                }
                if let Some(existing) = self.table.get(target_row, target_col).cloned() {
                    let new_val = data::CellValue::parse_like(&existing, cell_text);
                    self.table.set(target_row, target_col, new_val);
                }
            }
        }
        self.filter_dirty = true;
    }

    /// Copy selection to both internal and OS clipboard.
    fn do_copy(&mut self) {
        if let Some(text) = self.copy_selection_to_string() {
            self.table_state.clipboard = Some(text.clone());
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
            self.table_state.clipboard.clone()
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

    fn open_file(&mut self) {
        // If current file has unsaved changes, prompt before opening
        if self.table.is_modified() || self.raw_content_modified {
            self.pending_open_file = true;
            self.show_open_confirm = true;
            return;
        }
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
        match self.registry.reader_for_path(&path) {
            Some(reader) => match reader.read_file(&path) {
                Ok(table) => {
                    self.table = table;
                    self.table_state = TableViewState::default();
                    self.search_text.clear();
                    self.filter_dirty = true;
                    // Set up on-demand loading state for truncated files
                    if self.table.total_rows.is_some() {
                        let loaded = self.table.row_count();
                        self.status_message = Some((
                            format!(
                                "Loaded {} rows (scroll down to load more)",
                                ui::status_bar::format_number(loaded)
                            ),
                            std::time::Instant::now(),
                        ));
                        self.bg_can_load_more = true;
                        self.bg_row_buffer = None;
                        self.bg_loading_done
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        self.bg_file_exhausted
                            .store(false, std::sync::atomic::Ordering::Relaxed);
                    } else {
                        self.status_message = None;
                        self.bg_row_buffer = None;
                        self.bg_loading_done
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        self.bg_can_load_more = false;
                        self.bg_file_exhausted
                            .store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                    self.raw_view_formatted = false;

                    // Detect and store CSV delimiter (read only first few KB)
                    if self.table.format_name.as_deref() == Some("CSV") {
                        self.csv_delimiter = detect_delimiter_from_file(&path);
                    } else if self.table.format_name.as_deref() == Some("TSV") {
                        self.csv_delimiter = b'\t';
                    }

                    // Load raw content for text-based formats (skip for large files)
                    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    if file_size <= 500_000_000 {
                        // 500 MB
                        self.raw_content = std::fs::read_to_string(&path).ok();
                    } else {
                        self.raw_content = None;
                    }
                    self.raw_content_modified = false;

                    // For PDFs, render pages visually and default to Pdf view
                    self.pdf_page_images.clear();
                    self.pdf_textures.clear();
                    self.pdf_page_texts.clear();
                    if self.table.format_name.as_deref() == Some("PDF") {
                        match formats::pdf_reader::render_pdf_pages(&path, 2.0) {
                            Ok((images, texts)) => {
                                self.pdf_page_images = images;
                                self.pdf_page_texts = texts;
                                self.view_mode = ViewMode::Pdf;
                            }
                            Err(_) => {
                                self.view_mode = ViewMode::Table;
                            }
                        }
                    } else if self.table.format_name.as_deref() == Some("Markdown") {
                        self.view_mode = ViewMode::Markdown;
                    } else if self.table.format_name.as_deref() == Some("Jupyter Notebook") {
                        self.view_mode = ViewMode::Notebook;
                    } else if self.table.format_name.as_deref() == Some("Text") {
                        self.view_mode = ViewMode::Raw;
                    } else {
                        self.view_mode = ViewMode::Table;
                    }
                }
                Err(e) => {
                    self.status_message = Some((
                        format!("Error reading file: {}", e),
                        std::time::Instant::now(),
                    ));
                }
            },
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
            }
        }
    }

    fn save_file(&mut self) {
        if let Some(ref path) = self.table.source_path.clone() {
            let path = std::path::Path::new(path);
            self.do_save(path.to_path_buf());
        }
    }

    fn save_file_as(&mut self) {
        let mut dialog = rfd::FileDialog::new();
        for (label, exts) in self.registry.save_format_descriptions() {
            let ext_refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter(&label, &ext_refs);
        }
        if let Some(ref source) = self.table.source_path {
            if let Some(name) = std::path::Path::new(source).file_name() {
                dialog = dialog.set_file_name(name.to_string_lossy().to_string());
            }
        }

        if let Some(path) = dialog.save_file() {
            self.do_save(path);
        }
    }

    fn do_save(&mut self, path: std::path::PathBuf) {
        // If raw content was modified, write it directly to the file
        if self.raw_content_modified {
            if let Some(ref content) = self.raw_content {
                match std::fs::write(&path, content) {
                    Ok(()) => {
                        self.table.source_path = Some(path.to_string_lossy().to_string());
                        self.raw_content_modified = false;
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
        if self.table.format_name.as_deref() == Some("CSV") && self.csv_delimiter != b',' {
            self.table.apply_edits();
            match formats::csv_reader::write_delimited(&path, self.csv_delimiter, &self.table) {
                Ok(()) => {
                    self.table.source_path = Some(path.to_string_lossy().to_string());
                    self.table.clear_modified();
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
                self.table.apply_edits();
                match reader.write_file(&path, &self.table) {
                    Ok(()) => {
                        self.table.source_path = Some(path.to_string_lossy().to_string());
                        self.table.clear_modified();
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
        if self.search_text.is_empty() {
            self.filtered_rows = (0..self.table.row_count()).collect();
        } else {
            let matcher = RowMatcher::new(&self.search_text, self.search_mode);
            self.filtered_rows = (0..self.table.row_count())
                .filter(|&row_idx| {
                    (0..self.table.col_count()).any(|col_idx| {
                        self.table
                            .get(row_idx, col_idx)
                            .map(|v| matcher.matches(&v.to_string()))
                            .unwrap_or(false)
                    })
                })
                .collect();
        }
        self.filter_dirty = false;
    }

    /// Replace the next matching cell value (starting after the current selection).
    fn replace_next_match(&mut self) {
        if self.search_text.is_empty() {
            return;
        }
        let matcher = RowMatcher::new(&self.search_text, self.search_mode);
        let row_count = self.table.row_count();
        let col_count = self.table.col_count();
        if row_count == 0 || col_count == 0 {
            return;
        }

        // Start searching from the cell after the current selection
        let (start_row, start_col) = match self.table_state.selected_cell {
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

        // Scan all cells starting from (start_row, start_col), wrapping around
        let total_cells = row_count * col_count;
        let start_idx = start_row * col_count + start_col;
        for offset in 0..total_cells {
            let idx = (start_idx + offset) % total_cells;
            let row = idx / col_count;
            let col = idx % col_count;
            if let Some(val) = self.table.get(row, col) {
                let text = val.to_string();
                if matcher.matches(&text) {
                    let new_text = matcher.replace(&text, &self.replace_text);
                    let new_val = data::CellValue::parse_like(val, &new_text);
                    if new_val != *val {
                        self.table.set(row, col, new_val);
                    }
                    self.table_state.selected_cell = Some((row, col));
                    self.table_state.selected_rows.clear();
                    self.table_state.selected_cols.clear();
                    self.filter_dirty = true;
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
        if self.search_text.is_empty() {
            return;
        }
        let matcher = RowMatcher::new(&self.search_text, self.search_mode);
        let mut count = 0usize;
        for row in 0..self.table.row_count() {
            for col in 0..self.table.col_count() {
                if let Some(val) = self.table.get(row, col).cloned() {
                    let text = val.to_string();
                    if matcher.matches(&text) {
                        let new_text = matcher.replace(&text, &self.replace_text);
                        let new_val = data::CellValue::parse_like(&val, &new_text);
                        if new_val != val {
                            self.table.set(row, col, new_val);
                            count += 1;
                        }
                    }
                }
            }
        }
        self.filter_dirty = true;
        self.status_message = Some((
            format!("Replaced {} cell(s)", count),
            std::time::Instant::now(),
        ));
    }

    /// Open the "Delete Columns" dialog, initializing checkboxes.
    fn open_delete_columns_dialog(&mut self) {
        self.delete_col_selection = vec![false; self.table.col_count()];
        // Pre-select the currently selected column if any
        if let Some((_, col)) = self.table_state.selected_cell {
            if col < self.delete_col_selection.len() {
                self.delete_col_selection[col] = true;
            }
        }
        self.show_delete_columns_dialog = true;
    }

    /// Sort columns alphabetically by name, ascending or descending.
    #[allow(dead_code)]
    fn sort_columns_alphabetically(&mut self, ascending: bool) {
        let col_count = self.table.col_count();
        if col_count <= 1 {
            return;
        }

        // order[new_pos] = old_pos
        let mut order: Vec<usize> = (0..col_count).collect();
        order.sort_by(|&a, &b| {
            let cmp = self.table.columns[a]
                .name
                .to_lowercase()
                .cmp(&self.table.columns[b].name.to_lowercase());
            if ascending { cmp } else { cmp.reverse() }
        });

        // Reorder column widths to match
        let old_widths = self.table_state.col_widths.clone();
        self.table_state.col_widths = order
            .iter()
            .map(|&orig| old_widths.get(orig).copied().unwrap_or(120.0))
            .collect();

        // Update selected cell column: build reverse map
        if let Some((row, col)) = self.table_state.selected_cell {
            if let Some(new_col) = order.iter().position(|&orig| orig == col) {
                self.table_state.selected_cell = Some((row, new_col));
            }
        }

        // Apply the reorder atomically
        self.table.reorder_columns(&order);
        self.filter_dirty = true;
    }
}

impl eframe::App for OctaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Load file from CLI on first frame ---
        if let Some(path) = self.initial_file.take() {
            self.load_file(path);
        }

        // --- Global keyboard shortcuts ---
        let ctrl_held = ctx.input(|i| i.modifiers.command);
        if ctrl_held && ctx.input(|i| i.key_pressed(egui::Key::O)) {
            self.open_file();
        }
        if ctrl_held && ctx.input(|i| i.key_pressed(egui::Key::S)) {
            if self.table.source_path.is_some() {
                self.save_file();
            } else if self.table.col_count() > 0 {
                self.save_file_as();
            }
        }
        if ctrl_held && ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.search_focus_requested = true;
        }
        if ctrl_held && ctx.input(|i| i.key_pressed(egui::Key::H)) {
            self.show_replace_bar = !self.show_replace_bar;
            self.search_focus_requested = true;
        }
        if self.show_replace_bar && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_replace_bar = false;
        }
        if ctrl_held
            && ctx.input(|i| i.key_pressed(egui::Key::A))
            && self.table.col_count() > 0
            && self.table.row_count() > 0
        {
            self.table_state.selected_rows.clear();
            self.table_state.selected_cols.clear();
            for r in 0..self.table.row_count() {
                self.table_state.selected_rows.insert(r);
            }
        }

        // --- Handle close request ---
        if ctx.input(|i| i.viewport().close_requested())
            && (self.table.is_modified() || self.raw_content_modified)
            && !self.confirmed_close
        {
            // Block the close and show our dialog
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.show_close_confirm = true;
        }
        // If confirmed_close is true, we just let it close

        // Drain background-loaded rows into the table
        if let Some(ref buffer) = self.bg_row_buffer {
            let mut drained = false;
            if let Ok(mut buf) = buffer.try_lock() {
                if !buf.is_empty() {
                    self.table.rows.append(&mut *buf);
                    drained = true;
                }
            }
            let loading_done = self
                .bg_loading_done
                .load(std::sync::atomic::Ordering::Relaxed);
            if drained {
                self.filter_dirty = true;
                let file_exhausted = self
                    .bg_file_exhausted
                    .load(std::sync::atomic::Ordering::Relaxed);
                if self.table.total_rows.is_some() {
                    let total_loaded = self.table.row_offset + self.table.row_count();
                    let total_fmt = ui::status_bar::format_number(total_loaded);
                    if loading_done && file_exhausted {
                        self.status_message = Some((
                            format!("Loaded all {} rows", total_fmt),
                            std::time::Instant::now(),
                        ));
                        self.table.total_rows = None;
                        self.bg_can_load_more = false;
                    } else if loading_done {
                        self.status_message = Some((
                            format!("Loaded {} rows (scroll down to load more)", total_fmt),
                            std::time::Instant::now(),
                        ));
                        self.bg_can_load_more = true;
                    } else {
                        self.status_message = Some((
                            format!("Loading... {} rows so far", total_fmt),
                            std::time::Instant::now(),
                        ));
                    }
                }
                // Evict front rows if we have too many in memory
                if self.table.rows.len() > 3_000_000 {
                    let evict_count = self.table.rows.len() - 2_000_000;
                    self.table.evict_front_rows(evict_count);
                    self.filter_dirty = true;
                }
            }
            if loading_done {
                self.bg_row_buffer = None;
            }
            // Request repaint to keep draining
            if !loading_done {
                ctx.request_repaint();
            }
        }

        // Recompute filter if needed
        if self.filter_dirty {
            self.recompute_filter();
        }

        let search_active = !self.search_text.is_empty();
        let filtered_count = self.filtered_rows.len();

        // Top toolbar
        egui::TopBottomPanel::top("toolbar")
            .exact_height(40.0)
            .show(ctx, |ui| {
                // Lazily create logo texture
                if self.logo_texture.is_none() {
                    let opt = resvg::usvg::Options::default();
                    let svg_src = self.settings.icon_variant.svg_source();
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
                }

                let action = ui::toolbar::draw_toolbar(
                    ui,
                    self.theme_mode,
                    &mut self.search_text,
                    &mut self.search_mode,
                    self.search_focus_requested,
                    self.show_replace_bar,
                    &mut self.replace_text,
                    self.table.col_count() > 0,
                    self.table.is_modified(),
                    self.table.source_path.is_some(),
                    self.table_state.selected_cell,
                    self.table.row_count(),
                    self.table.col_count(),
                    self.view_mode,
                    self.raw_content.is_some(),
                    !self.pdf_page_images.is_empty(),
                    self.table.format_name.as_deref() == Some("Markdown"),
                    self.table.format_name.as_deref() == Some("Jupyter Notebook"),
                    self.logo_texture.as_ref(),
                );
                // Clear focus request after this frame
                self.search_focus_requested = false;

                if action.open_file {
                    self.open_file();
                }
                if action.save_file {
                    self.save_file();
                }
                if action.save_file_as {
                    self.save_file_as();
                }
                if action.toggle_theme {
                    self.theme_mode = self.theme_mode.toggle();
                    ui::theme::apply_theme(ctx, self.theme_mode, self.settings.font_size);
                }
                if action.search_changed {
                    self.filter_dirty = true;
                }
                if action.toggle_replace_bar {
                    self.show_replace_bar = !self.show_replace_bar;
                }
                if action.replace_next {
                    self.replace_next_match();
                }
                if action.replace_all {
                    self.replace_all_matches();
                }

                // --- View mode change ---
                if let Some(new_mode) = action.view_mode_changed {
                    self.view_mode = new_mode;
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
                    let insert_at = match self.table_state.selected_cell {
                        Some((row, _)) => row + 1,
                        None => self.table.row_count(),
                    };
                    self.table.insert_row(insert_at);
                    let sel_col = self.table_state.selected_cell.map(|(_, c)| c).unwrap_or(0);
                    self.table_state.selected_cell = Some((insert_at, sel_col));
                    self.table_state.editing_cell = None;
                    self.filter_dirty = true;
                }
                if action.delete_row {
                    if let Some((row, col)) = self.table_state.selected_cell {
                        self.table.delete_row(row);
                        self.table_state.editing_cell = None;
                        if self.table.row_count() == 0 {
                            self.table_state.selected_cell = None;
                        } else {
                            let new_row = row.min(self.table.row_count() - 1);
                            self.table_state.selected_cell = Some((new_row, col));
                        }
                        self.filter_dirty = true;
                    }
                }
                if action.move_row_up {
                    if let Some((row, col)) = self.table_state.selected_cell {
                        if row > 0 {
                            self.table.move_row(row, row - 1);
                            self.table_state.selected_cell = Some((row - 1, col));
                            self.filter_dirty = true;
                        }
                    }
                }
                if action.move_row_down {
                    if let Some((row, col)) = self.table_state.selected_cell {
                        if row + 1 < self.table.row_count() {
                            self.table.move_row(row, row + 1);
                            self.table_state.selected_cell = Some((row + 1, col));
                            self.filter_dirty = true;
                        }
                    }
                }

                // --- Column operations ---
                if action.add_column {
                    self.show_add_column_dialog = true;
                    self.new_col_name.clear();
                    self.new_col_type = "String".to_string();
                    self.new_col_formula.clear();
                    // Insert after selected column, or at end
                    self.insert_col_at = self.table_state.selected_cell.map(|(_, c)| c + 1);
                }
                if action.delete_column && self.table.col_count() > 0 {
                    self.open_delete_columns_dialog();
                }
                if action.move_col_left {
                    if let Some((row, col)) = self.table_state.selected_cell {
                        if col > 0 {
                            self.table.move_column(col, col - 1);
                            self.table_state.selected_cell = Some((row, col - 1));
                            self.table_state.widths_initialized = false;
                        }
                    }
                }
                if action.move_col_right {
                    if let Some((row, col)) = self.table_state.selected_cell {
                        if col + 1 < self.table.col_count() {
                            self.table.move_column(col, col + 1);
                            self.table_state.selected_cell = Some((row, col + 1));
                            self.table_state.widths_initialized = false;
                        }
                    }
                }
                if let Some(col_idx) = action.sort_rows_asc_by {
                    self.table.sort_rows_by_column(col_idx, true);
                    self.filter_dirty = true;
                }
                if let Some(col_idx) = action.sort_rows_desc_by {
                    self.table.sort_rows_by_column(col_idx, false);
                    self.filter_dirty = true;
                }

                if action.discard_edits {
                    self.table.discard_edits();
                }
            });

        // --- Add Column dialog ---
        if self.show_add_column_dialog {
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
                        ui.text_edit_singleline(&mut self.new_col_name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Type:");
                        egui::ComboBox::from_id_salt("col_type_combo")
                            .selected_text(self.new_col_type.as_str())
                            .show_ui(ui, |ui| {
                                for t in COLUMN_TYPES {
                                    ui.selectable_value(&mut self.new_col_type, t.to_string(), *t);
                                }
                            });
                    });
                    ui.add_space(4.0);
                    // Show/edit insert position
                    ui.horizontal(|ui| {
                        ui.label("Insert at position:");
                        let col_count = self.table.col_count();
                        let mut pos_val = self.insert_col_at.unwrap_or(col_count) + 1;
                        let drag = egui::DragValue::new(&mut pos_val)
                            .range(1..=(col_count + 1))
                            .speed(1.0);
                        if ui.add(drag).changed() {
                            self.insert_col_at = Some((pos_val - 1).min(col_count));
                        }
                        ui.label(format!("/ {}", col_count + 1));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Formula:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_col_formula)
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
                        if ui.button("Add").clicked() && !self.new_col_name.is_empty() {
                            should_add = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_add_column_dialog = false;
                        }
                    });
                });
            if should_add {
                let idx = self.insert_col_at.unwrap_or(self.table.col_count());
                let formula_text = self.new_col_formula.trim().to_string();
                self.table
                    .insert_column(idx, self.new_col_name.clone(), self.new_col_type.clone());
                // If a formula was provided, evaluate it for every row
                if formula_text.starts_with('=') {
                    let formula_body = &formula_text[1..];
                    let row_count = self.table.row_count();
                    for row in 0..row_count {
                        // Rewrite the formula for this row: replace row numbers
                        // The formula uses row 1 as template — shift references for each row
                        let shifted = shift_formula_row(formula_body, row);
                        if let Some(result) =
                            data::evaluate_formula(&shifted, &self.table)
                        {
                            let val = if result.fract() == 0.0
                                && result.abs() < i64::MAX as f64
                            {
                                data::CellValue::Int(result as i64)
                            } else {
                                data::CellValue::Float(result)
                            };
                            self.table.set(row, idx, val);
                        }
                    }
                }
                // Select the new column
                if let Some((row, _)) = self.table_state.selected_cell {
                    self.table_state.selected_cell = Some((row, idx));
                }
                self.table_state.widths_initialized = false;
                self.filter_dirty = true;
                self.show_add_column_dialog = false;
            }
            if !open {
                self.show_add_column_dialog = false;
            }
        }

        // --- Delete Columns dialog ---
        if self.show_delete_columns_dialog {
            let mut open = true;
            let mut should_delete = false;
            // Make sure selection vec is in sync (table may have changed)
            if self.delete_col_selection.len() != self.table.col_count() {
                self.delete_col_selection = vec![false; self.table.col_count()];
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

                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for (idx, col) in self.table.columns.iter().enumerate() {
                                let mut checked = self.delete_col_selection[idx];
                                let label = format!("{} [{}]", col.name, col.data_type);
                                if ui.checkbox(&mut checked, label).changed() {
                                    self.delete_col_selection[idx] = checked;
                                }
                            }
                        });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.small_button("All").clicked() {
                            for v in &mut self.delete_col_selection {
                                *v = true;
                            }
                        }
                        if ui.small_button("None").clicked() {
                            for v in &mut self.delete_col_selection {
                                *v = false;
                            }
                        }
                    });

                    let selected_count = self.delete_col_selection.iter().filter(|&&v| v).count();
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
                            self.show_delete_columns_dialog = false;
                        }
                    });
                });

            if should_delete {
                // Delete in reverse order to keep indices valid
                let to_delete: Vec<usize> = self
                    .delete_col_selection
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &sel)| if sel { Some(i) } else { None })
                    .rev()
                    .collect();

                for col_idx in to_delete {
                    self.table.delete_column(col_idx);
                }

                self.table_state.editing_cell = None;
                if self.table.col_count() == 0 {
                    self.table_state.selected_cell = None;
                } else if let Some((row, col)) = self.table_state.selected_cell {
                    let new_col = col.min(self.table.col_count() - 1);
                    self.table_state.selected_cell = Some((row, new_col));
                }
                self.table_state.widths_initialized = false;
                self.filter_dirty = true;
                self.show_delete_columns_dialog = false;
            }

            if !open {
                self.show_delete_columns_dialog = false;
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
                            if self.table.source_path.is_some() {
                                self.save_file();
                            } else {
                                self.save_file_as();
                            }
                            // After save, close
                            self.confirmed_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Don't Save").clicked() {
                            self.show_close_confirm = false;
                            self.confirmed_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_close_confirm = false;
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
                            if self.table.source_path.is_some() {
                                self.save_file();
                            } else {
                                self.save_file_as();
                            }
                            self.do_open_file_dialog();
                        }
                        if ui.button("Don't Save").clicked() {
                            self.show_open_confirm = false;
                            self.table.clear_modified();
                            self.raw_content_modified = false;
                            self.do_open_file_dialog();
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_open_confirm = false;
                            self.pending_open_file = false;
                        }
                    });
                });
        }

        // --- Settings dialog ---
        if let Some(new_settings) = self.settings_dialog.show(ctx) {
            let icon_changed = self.settings_dialog.icon_changed;
            let font_changed = self.settings_dialog.font_changed;
            let theme_changed = self.settings_dialog.theme_changed;

            self.settings = new_settings;
            self.settings.save();

            if theme_changed {
                self.theme_mode = self.settings.default_theme;
            }
            if font_changed || theme_changed {
                ui::theme::apply_theme(ctx, self.theme_mode, self.settings.font_size);
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

**Supported formats:** Parquet, CSV, TSV, JSON, JSONL, Excel (.xlsx), Avro, Arrow IPC, XML, TOML, YAML, PDF, Markdown, Plain Text.

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
                            .show(ui, &mut self.commonmark_cache, docs);
                    });
                });
            self.show_documentation_dialog = open;
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

        // Bottom status bar
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(28.0)
            .show(ctx, |ui| {
                ui::status_bar::draw_status_bar(
                    ui,
                    &self.table,
                    &self.table_state,
                    self.theme_mode,
                    filtered_count,
                    search_active,
                );
            });

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
            if self.filter_dirty {
                self.recompute_filter();
            }

            // --- PDF rendered view ---
            if self.view_mode == ViewMode::Pdf {
                // Lazily create textures from rendered images
                if self.pdf_textures.len() != self.pdf_page_images.len() {
                    self.pdf_textures.clear();
                    for (i, image) in self.pdf_page_images.iter().enumerate() {
                        let texture = ctx.load_texture(
                            format!("pdf_page_{}", i),
                            image.clone(),
                            egui::TextureOptions::LINEAR,
                        );
                        self.pdf_textures.push(texture);
                    }
                }

                if self.pdf_textures.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("No PDF pages to display")
                                .size(16.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                } else {
                    let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                for (page_idx, texture) in self.pdf_textures.iter().enumerate() {
                                    let size = texture.size_vec2();
                                    let page_text = self.pdf_page_texts
                                        .get(page_idx)
                                        .cloned()
                                        .unwrap_or_default();
                                    // Page header
                                    ui.label(
                                        RichText::new(format!(
                                            "Page {} of {}",
                                            page_idx + 1,
                                            self.pdf_textures.len()
                                        ))
                                        .size(11.0)
                                        .color(colors.text_muted),
                                    );
                                    ui.add_space(4.0);
                                    // Rendered page image
                                    egui::Frame::new()
                                        .fill(egui::Color32::WHITE)
                                        .shadow(egui::epaint::Shadow {
                                            offset: [2, 2],
                                            blur: 8,
                                            spread: 0,
                                            color: colors.border.gamma_multiply(0.5),
                                        })
                                        .show(ui, |ui| {
                                            ui.image(egui::load::SizedTexture::new(
                                                texture.id(),
                                                size,
                                            ));
                                        });
                                    // Selectable text below the page image
                                    if !page_text.is_empty() {
                                        ui.add_space(4.0);
                                        egui::Frame::new()
                                            .fill(colors.bg_secondary)
                                            .stroke(Stroke::new(1.0, colors.border_subtle))
                                            .corner_radius(4.0)
                                            .inner_margin(8.0)
                                            .show(ui, |ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(&page_text)
                                                            .font(egui::FontId::new(
                                                                12.0,
                                                                egui::FontFamily::Monospace,
                                                            ))
                                                            .color(colors.text_primary),
                                                    )
                                                    .selectable(true),
                                                );
                                            });
                                    }
                                    ui.add_space(16.0);
                                    ui.separator();
                                    ui.add_space(8.0);
                                }
                            });
                        });
                }
                return;
            }

            // --- Jupyter Notebook rendered view ---
            if self.view_mode == ViewMode::Notebook {
                let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
                let is_dark = self.theme_mode == ui::theme::ThemeMode::Dark;

                if self.table.row_count() == 0 {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Empty notebook")
                                .size(16.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                } else {
                    // Helper: build a LayoutJob for line number gutter (non-selectable)
                    let build_line_numbers =
                        |line_count: usize, line_num_color: Color32| {
                            let mono = egui::FontId::new(13.0, egui::FontFamily::Monospace);
                            let gutter_width = line_count.max(1).to_string().len();
                            let mut job = egui::text::LayoutJob::default();
                            for i in 0..line_count.max(1) {
                                let num_str = format!("{:>width$}", i + 1, width = gutter_width);
                                let suffix = if i + 1 < line_count.max(1) { "\n" } else { "" };
                                job.append(
                                    &format!("{}{}", num_str, suffix),
                                    0.0,
                                    egui::text::TextFormat {
                                        font_id: mono.clone(),
                                        color: line_num_color,
                                        ..Default::default()
                                    },
                                );
                            }
                            job
                        };

                    // Collect all cell text for Ctrl+C on the whole notebook
                    let mut all_notebook_text = String::new();

                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.add_space(16.0);
                                ui.vertical(|ui| {
                                    for row_idx in 0..self.table.row_count() {
                                        let cell_num = match self.table.get(row_idx, 0) {
                                            Some(data::CellValue::Int(n)) => Some(n),
                                            _ => None,
                                        };
                                        let cell_type = match self.table.get(row_idx, 1) {
                                            Some(data::CellValue::String(s)) => s.clone(),
                                            _ => "code".to_string(),
                                        };
                                        let source = match self.table.get(row_idx, 2) {
                                            Some(data::CellValue::String(s)) => s.clone(),
                                            Some(v) => v.to_string(),
                                            None => String::new(),
                                        };
                                        let output = match self.table.get(row_idx, 3) {
                                            Some(data::CellValue::String(s)) => s.clone(),
                                            Some(v) => v.to_string(),
                                            None => String::new(),
                                        };

                                        // Accumulate for whole-notebook copy
                                        if !all_notebook_text.is_empty() {
                                            all_notebook_text.push_str("\n\n");
                                        }
                                        all_notebook_text.push_str(&source);
                                        if !output.is_empty() {
                                            all_notebook_text.push('\n');
                                            all_notebook_text.push_str(&output);
                                        }

                                        let is_code = cell_type == "code";
                                        let is_markdown = cell_type == "markdown";

                                        // Cell container
                                        let cell_bg = if is_code {
                                            if is_dark {
                                                Color32::from_rgb(30, 34, 42)
                                            } else {
                                                Color32::from_rgb(248, 249, 250)
                                            }
                                        } else if is_dark {
                                            Color32::from_rgb(35, 38, 45)
                                        } else {
                                            Color32::from_rgb(252, 252, 254)
                                        };

                                        let border_color = if is_code {
                                            if is_dark {
                                                Color32::from_rgb(60, 70, 90)
                                            } else {
                                                Color32::from_rgb(200, 210, 220)
                                            }
                                        } else {
                                            colors.border_subtle
                                        };

                                        let line_num_color = if is_dark {
                                            Color32::from_rgb(100, 110, 130)
                                        } else {
                                            Color32::from_rgb(150, 160, 175)
                                        };

                                        let text_color = if is_markdown {
                                            colors.text_secondary
                                        } else {
                                            colors.text_primary
                                        };

                                        let line_count = source.lines().count();

                                        // Cell label (e.g. "In [1]:" or nothing for markdown)
                                        ui.horizontal(|ui| {
                                            // Left label area
                                            let label_width = 80.0;
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(label_width, 20.0),
                                                Layout::right_to_left(Align::TOP),
                                                |ui| {
                                                    if is_code {
                                                        let label = if let Some(n) = cell_num {
                                                            format!("In [{}]:", n)
                                                        } else {
                                                            "In [ ]:".to_string()
                                                        };
                                                        ui.label(
                                                            RichText::new(label)
                                                                .font(egui::FontId::new(
                                                                    12.0,
                                                                    egui::FontFamily::Monospace,
                                                                ))
                                                                .color(colors.accent),
                                                        );
                                                    }
                                                },
                                            );

                                            // Cell content area with separate gutter + source
                                            let frame_response = egui::Frame::new()
                                                .fill(cell_bg)
                                                .stroke(Stroke::new(1.0, border_color))
                                                .corner_radius(4.0)
                                                .inner_margin(8.0)
                                                .show(ui, |ui| {
                                                    ui.horizontal_top(|ui| {
                                                        // Line number gutter (not selectable)
                                                        let gutter_job = build_line_numbers(
                                                            line_count,
                                                            line_num_color,
                                                        );
                                                        ui.add(
                                                            egui::Label::new(gutter_job)
                                                                .selectable(false),
                                                        );
                                                        ui.add_space(8.0);
                                                        // Source text (selectable — no line numbers)
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(&source)
                                                                    .font(egui::FontId::new(
                                                                        13.0,
                                                                        egui::FontFamily::Monospace,
                                                                    ))
                                                                    .color(text_color),
                                                            )
                                                            .selectable(true),
                                                        );
                                                    });
                                                });
                                            let copy_source = source.clone();
                                            let all_text = all_notebook_text.clone();
                                            frame_response.response.context_menu(|ui| {
                                                if ui.button("Copy cell").clicked() {
                                                    ui.ctx().copy_text(copy_source.clone());
                                                    ui.close_menu();
                                                }
                                                if ui.button("Copy all cells").clicked() {
                                                    ui.ctx().copy_text(all_text.clone());
                                                    ui.close_menu();
                                                }
                                            });

                                            // Output area (code cells only)
                                            if is_code && !output.is_empty() {
                                                let out_bg = if is_dark {
                                                    Color32::from_rgb(25, 28, 35)
                                                } else {
                                                    Color32::from_rgb(255, 255, 255)
                                                };
                                                let out_frame = egui::Frame::new()
                                                    .fill(out_bg)
                                                    .stroke(Stroke::new(1.0, border_color))
                                                    .corner_radius(4.0)
                                                    .inner_margin(8.0)
                                                    .show(ui, |ui| {
                                                        let out_label =
                                                            if let Some(n) = cell_num {
                                                                format!("Out[{}]:", n)
                                                            } else {
                                                                "Out[ ]:".to_string()
                                                            };
                                                        ui.horizontal(|ui| {
                                                            ui.label(
                                                                RichText::new(out_label)
                                                                    .font(egui::FontId::new(
                                                                        12.0,
                                                                        egui::FontFamily::Monospace,
                                                                    ))
                                                                    .color(colors.error),
                                                            );
                                                        });
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(&output)
                                                                    .font(egui::FontId::new(
                                                                        13.0,
                                                                        egui::FontFamily::Monospace,
                                                                    ))
                                                                    .color(colors.text_secondary),
                                                            )
                                                            .selectable(true),
                                                        );
                                                    });
                                                let copy_output = output.clone();
                                                out_frame.response.context_menu(|ui| {
                                                    if ui.button("Copy output").clicked() {
                                                        ui.ctx().copy_text(copy_output.clone());
                                                        ui.close_menu();
                                                    }
                                                });
                                            }
                                        });

                                        // Separator between cells
                                        ui.add_space(8.0);
                                        ui.separator();
                                        ui.add_space(4.0);
                                    }
                                });
                            });
                        });

                    // Ctrl+X: cut (copy all notebook content — notebook view is read-only)
                    if ui.input(|i| {
                        i.modifiers.command && i.key_pressed(egui::Key::X)
                    }) {
                        ctx.copy_text(all_notebook_text);
                    }
                }
                return;
            }

            // --- Markdown rendered view ---
            if self.view_mode == ViewMode::Markdown {
                if let Some(ref content) = self.raw_content {
                    let md_content = content.clone();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.add_space(16.0);
                                ui.vertical(|ui| {
                                    ui.set_max_width(900.0);
                                    egui_commonmark::CommonMarkViewer::new().show(
                                        ui,
                                        &mut self.commonmark_cache,
                                        &md_content,
                                    );
                                });
                            });
                        });
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Markdown content not available")
                                .size(16.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                }
                return;
            }

            // --- Raw text view ---
            if self.view_mode == ViewMode::Raw {
                if let Some(ref mut content) = self.raw_content {
                    let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);

                    // Toolbar for CSV/TSV: align columns + delimiter selector
                    let is_csv = self.table.format_name.as_deref() == Some("CSV");
                    let is_tsv = self.table.format_name.as_deref() == Some("TSV");
                    if is_csv || is_tsv {
                        ui.horizontal(|ui| {
                            if ui
                                .checkbox(&mut self.raw_view_formatted, "Align Columns")
                                .changed()
                                && self.raw_view_formatted
                            {
                                let delim = self.csv_delimiter as char;
                                *content = format_delimited_text(content, delim);
                                self.raw_content_modified = true;
                            }
                            ui.add_space(16.0);
                            if is_csv {
                                ui.label("Delimiter:");
                                let delim_label = match self.csv_delimiter {
                                    b',' => "Comma (,)",
                                    b';' => "Semicolon (;)",
                                    b'|' => "Pipe (|)",
                                    b'\t' => "Tab (\\t)",
                                    _ => "Comma (,)",
                                };
                                egui::ComboBox::from_id_salt("csv_delimiter_combo")
                                    .selected_text(delim_label)
                                    .show_ui(ui, |ui| {
                                        let options: &[(u8, &str)] = &[
                                            (b',', "Comma (,)"),
                                            (b';', "Semicolon (;)"),
                                            (b'|', "Pipe (|)"),
                                            (b'\t', "Tab (\\t)"),
                                        ];
                                        for &(delim, label) in options {
                                            if ui
                                                .selectable_value(
                                                    &mut self.csv_delimiter,
                                                    delim,
                                                    label,
                                                )
                                                .clicked()
                                            {
                                                self.raw_content_modified = true;
                                            }
                                        }
                                    });
                            }
                        });
                        ui.add_space(2.0);
                    }

                    // Line numbers + text editor side by side
                    let line_count = content.lines().count().max(1);
                    let line_num_text: String = (1..=line_count)
                        .map(|n| format!("{:>width$}", n, width = line_count.to_string().len()))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let line_num_width = line_count.to_string().len() as f32 * 8.0 + 16.0;

                    let mono_font = egui::FontId::new(13.0, egui::FontFamily::Monospace);
                    let nowrap_layouter = |ui: &egui::Ui, text: &str, _wrap_width: f32| {
                        let mut job = egui::text::LayoutJob::simple(
                            text.to_owned(),
                            egui::FontId::new(13.0, egui::FontFamily::Monospace),
                            ui.visuals().text_color(),
                            f32::INFINITY,
                        );
                        job.wrap.max_width = f32::INFINITY;
                        ui.fonts(|f| f.layout_job(job))
                    };

                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                // Line numbers column (non-editable)
                                ui.add_sized(
                                    [line_num_width, ui.available_height()],
                                    egui::TextEdit::multiline(&mut line_num_text.clone())
                                        .font(mono_font.clone())
                                        .interactive(false)
                                        .desired_width(line_num_width)
                                        .text_color(colors.text_muted)
                                        .frame(false)
                                        .layouter(&mut nowrap_layouter.clone()),
                                );
                                // Separator line
                                ui.add_space(2.0);
                                let sep_rect = egui::Rect::from_min_size(
                                    ui.cursor().left_top(),
                                    egui::vec2(1.0, ui.available_height()),
                                );
                                ui.painter().rect_filled(sep_rect, 0.0, colors.border);
                                ui.add_space(4.0);
                                // Text editor (no wrapping — scroll horizontally)
                                let response = ui.add(
                                    egui::TextEdit::multiline(content)
                                        .font(mono_font)
                                        .desired_width(f32::INFINITY)
                                        .text_color(colors.text_primary)
                                        .layouter(&mut nowrap_layouter.clone()),
                                );
                                if response.changed() {
                                    self.raw_content_modified = true;
                                }
                            });
                        });
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Raw text view is not available for binary formats")
                                .size(16.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                }
                return;
            }

            // --- Table view ---
            let filtered = self.filtered_rows.clone();
            let os_has_clip = self.table_state.clipboard.is_some() || self.os_clipboard_has_text();
            let interaction = ui::table_view::draw_table(
                ui,
                &mut self.table,
                &mut self.table_state,
                self.theme_mode,
                &filtered,
                os_has_clip,
                self.settings.show_row_numbers,
                self.settings.alternating_row_colors,
                self.settings.negative_numbers_red,
                self.settings.highlight_edits,
                self.settings.font_size,
            );

            // Handle column header click: update insert position for "Add Column" dialog
            if let Some(col_idx) = interaction.header_col_clicked {
                self.insert_col_at = Some(col_idx + 1);
                if let Some((row, _)) = self.table_state.selected_cell {
                    self.table_state.selected_cell = Some((row, col_idx));
                }
            }

            // Handle drag-and-drop column move
            if let Some((from, to)) = interaction.col_drag_move {
                self.table.move_column(from, to);
                if let Some((row, col)) = self.table_state.selected_cell {
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
                    self.table_state.selected_cell = Some((row, new_col));
                }
                if from < self.table_state.col_widths.len()
                    && to < self.table_state.col_widths.len()
                {
                    let w = self.table_state.col_widths.remove(from);
                    self.table_state.col_widths.insert(to, w);
                }
                self.filter_dirty = true;
            }

            // Handle column rename
            if let Some((col_idx, new_name)) = interaction.rename_column {
                if col_idx < self.table.columns.len() && !new_name.is_empty() {
                    self.table.columns[col_idx].name = new_name;
                    self.table.structural_changes = true;
                    self.table_state.widths_initialized = false;
                }
            }

            // Handle column type change
            if let Some((col_idx, new_type)) = interaction.change_col_type {
                if col_idx < self.table.columns.len() {
                    self.table.columns[col_idx].data_type = new_type;
                    self.table.structural_changes = true;
                }
            }

            // Sort rows by column (from table header arrows or context menu)
            if let Some(col_idx) = interaction.sort_rows_asc_by {
                self.table.sort_rows_by_column(col_idx, true);
                self.filter_dirty = true;
            }
            if let Some(col_idx) = interaction.sort_rows_desc_by {
                self.table.sort_rows_by_column(col_idx, false);
                self.filter_dirty = true;
            }

            // --- Context menu: row operations ---
            if interaction.ctx_insert_row {
                let insert_at = match self.table_state.selected_cell {
                    Some((row, _)) => row + 1,
                    None => self.table.row_count(),
                };
                self.table.insert_row(insert_at);
                let sel_col = self.table_state.selected_cell.map(|(_, c)| c).unwrap_or(0);
                self.table_state.selected_cell = Some((insert_at, sel_col));
                self.table_state.editing_cell = None;
                self.filter_dirty = true;
            }
            if interaction.ctx_delete_row {
                if let Some((row, col)) = self.table_state.selected_cell {
                    self.table.delete_row(row);
                    self.table_state.editing_cell = None;
                    if self.table.row_count() == 0 {
                        self.table_state.selected_cell = None;
                    } else {
                        let new_row = row.min(self.table.row_count() - 1);
                        self.table_state.selected_cell = Some((new_row, col));
                    }
                    self.filter_dirty = true;
                }
            }
            if interaction.ctx_move_row_up {
                if let Some((row, col)) = self.table_state.selected_cell {
                    if row > 0 {
                        self.table.move_row(row, row - 1);
                        self.table_state.selected_cell = Some((row - 1, col));
                        self.filter_dirty = true;
                    }
                }
            }
            if interaction.ctx_move_row_down {
                if let Some((row, col)) = self.table_state.selected_cell {
                    if row + 1 < self.table.row_count() {
                        self.table.move_row(row, row + 1);
                        self.table_state.selected_cell = Some((row + 1, col));
                        self.filter_dirty = true;
                    }
                }
            }

            // --- Context menu: column operations ---
            if interaction.ctx_insert_column {
                self.show_add_column_dialog = true;
                self.new_col_name.clear();
                self.new_col_type = "String".to_string();
                self.new_col_formula.clear();
                self.insert_col_at = self.table_state.selected_cell.map(|(_, c)| c + 1);
            }
            if interaction.ctx_delete_column && self.table.col_count() > 0 {
                self.open_delete_columns_dialog();
            }
            if interaction.ctx_move_col_left {
                if let Some((row, col)) = self.table_state.selected_cell {
                    if col > 0 {
                        self.table.move_column(col, col - 1);
                        self.table_state.selected_cell = Some((row, col - 1));
                        self.table_state.widths_initialized = false;
                    }
                }
            }
            if interaction.ctx_move_col_right {
                if let Some((row, col)) = self.table_state.selected_cell {
                    if col + 1 < self.table.col_count() {
                        self.table.move_column(col, col + 1);
                        self.table_state.selected_cell = Some((row, col + 1));
                        self.table_state.widths_initialized = false;
                    }
                }
            }

            // --- Copy / Paste ---
            if interaction.ctx_copy_cell {
                if let Some((row, col)) = self.table_state.selected_cell {
                    let text = self
                        .table
                        .get(row, col)
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    self.table_state.clipboard = Some(text.clone());
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
            if interaction.undo {
                self.table.undo();
                self.filter_dirty = true;
                self.table_state.widths_initialized = false;
            }
            if interaction.redo {
                self.table.redo();
                self.filter_dirty = true;
                self.table_state.widths_initialized = false;
            }

            // --- Color marks ---
            if let Some((key, color)) = interaction.set_mark {
                self.table.set_mark(key, color);
            }
            if let Some(key) = interaction.clear_mark {
                self.table.clear_mark(key);
            }

            // --- Lazy loading: load more rows on demand ---
            if interaction.needs_more_rows
                && self.bg_can_load_more
                && self.bg_row_buffer.is_none()
                && self.table.total_rows.is_some()
            {
                self.bg_can_load_more = false;
                let buffer = Arc::new(Mutex::new(Vec::<Vec<data::CellValue>>::new()));
                let done_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let exhausted_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
                self.bg_row_buffer = Some(buffer.clone());
                self.bg_loading_done = done_flag.clone();
                self.bg_file_exhausted = exhausted_flag.clone();

                let skip_rows = self.table.row_offset + self.table.row_count();
                let max_chunk = 1_000_000usize;

                if let Some(ref source_path) = self.table.source_path.clone() {
                    let path = std::path::PathBuf::from(source_path);
                    let format_name = self.table.format_name.clone().unwrap_or_default();
                    let num_cols = self.table.col_count();
                    let csv_delimiter = self.csv_delimiter;

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
