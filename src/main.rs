use octo::data::{self, DataTable, ViewMode};
use octo::formats::{self, FormatRegistry};
use octo::ui;
use ui::table_view::TableViewState;
use ui::theme::ThemeMode;

use eframe::egui;
use egui::RichText;

use std::sync::{Arc, Mutex};

const OCTO_SVG: &str = include_str!("../assets/octo.svg");

fn render_icon() -> egui::IconData {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(OCTO_SVG, &opt).expect("Failed to parse SVG");
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

fn main() -> eframe::Result<()> {
    // Parse CLI arguments: octo [file_path]
    let initial_file = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists());

    let title = match &initial_file {
        Some(p) => format!(
            "Octo - {}",
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        ),
        None => "Octo".to_string(),
    };

    let icon = render_icon();
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
        "Octo",
        options,
        Box::new(move |cc| {
            ui::theme::apply_theme(&cc.egui_ctx, ThemeMode::Light);
            Ok(Box::new(OctoApp::new(initial_file)))
        }),
    )
}

struct OctoApp {
    table: DataTable,
    registry: FormatRegistry,
    theme_mode: ThemeMode,
    table_state: TableViewState,
    search_text: String,
    filtered_rows: Vec<usize>,
    filter_dirty: bool,
    status_message: Option<(String, std::time::Instant)>,
    /// "Add Column" dialog state
    show_add_column_dialog: bool,
    new_col_name: String,
    new_col_type: String,
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
}

/// Detect delimiter from a file by reading only the first few KB.
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

impl OctoApp {
    fn new(initial_file: Option<std::path::PathBuf>) -> Self {
        Self {
            table: DataTable::empty(),
            registry: FormatRegistry::new(),
            theme_mode: ThemeMode::Light,
            table_state: TableViewState::default(),
            search_text: String::new(),
            filtered_rows: Vec::new(),
            filter_dirty: true,
            status_message: None,
            show_add_column_dialog: false,
            new_col_name: String::new(),
            new_col_type: "String".to_string(),
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
                    if self.table.format_name.as_deref() == Some("PDF") {
                        match formats::pdf_reader::render_pdf_pages(&path, 2.0) {
                            Ok(images) => {
                                self.pdf_page_images = images;
                                self.view_mode = ViewMode::Pdf;
                            }
                            Err(_) => {
                                self.view_mode = ViewMode::Table;
                            }
                        }
                    } else if self.table.format_name.as_deref() == Some("Markdown") {
                        self.view_mode = ViewMode::Markdown;
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

    fn recompute_filter(&mut self) {
        if self.search_text.is_empty() {
            self.filtered_rows = (0..self.table.row_count()).collect();
        } else {
            let query = self.search_text.to_lowercase();
            self.filtered_rows = (0..self.table.row_count())
                .filter(|&row_idx| {
                    (0..self.table.col_count()).any(|col_idx| {
                        self.table
                            .get(row_idx, col_idx)
                            .map(|v| v.to_string().to_lowercase().contains(&query))
                            .unwrap_or(false)
                    })
                })
                .collect();
        }
        self.filter_dirty = false;
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
            if ascending {
                cmp
            } else {
                cmp.reverse()
            }
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

impl eframe::App for OctoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Load file from CLI on first frame ---
        if let Some(path) = self.initial_file.take() {
            self.load_file(path);
        }

        // --- Global keyboard shortcuts ---
        let ctrl_held = ctx.input(|i| i.modifiers.command);
        if ctrl_held && ctx.input(|i| i.key_pressed(egui::Key::S)) {
            if self.table.source_path.is_some() {
                self.save_file();
            } else if self.table.col_count() > 0 {
                self.save_file_as();
            }
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
                    if let Ok(tree) = resvg::usvg::Tree::from_str(OCTO_SVG, &opt) {
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
                                "octo_logo",
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
                    self.logo_texture.as_ref(),
                );

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
                    ui::theme::apply_theme(ctx, self.theme_mode);
                }
                if action.search_changed {
                    self.filter_dirty = true;
                }

                // --- View mode change ---
                if let Some(new_mode) = action.view_mode_changed {
                    self.view_mode = new_mode;
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
                    ui.label(
                        RichText::new("Tip: click a column header to set insert position")
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
                self.table
                    .insert_column(idx, self.new_col_name.clone(), self.new_col_type.clone());
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
                                for texture in &self.pdf_textures {
                                    let size = texture.size_vec2();
                                    // Add a subtle border/shadow around each page
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
                                    ui.add_space(12.0);
                                }
                            });
                        });
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

                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                // Line numbers column (non-editable)
                                ui.add_sized(
                                    [line_num_width, ui.available_height()],
                                    egui::TextEdit::multiline(&mut line_num_text.clone())
                                        .font(egui::FontId::new(13.0, egui::FontFamily::Monospace))
                                        .interactive(false)
                                        .desired_width(line_num_width)
                                        .text_color(colors.text_muted)
                                        .frame(false),
                                );
                                // Separator line
                                ui.add_space(2.0);
                                let sep_rect = egui::Rect::from_min_size(
                                    ui.cursor().left_top(),
                                    egui::vec2(1.0, ui.available_height()),
                                );
                                ui.painter().rect_filled(sep_rect, 0.0, colors.border);
                                ui.add_space(4.0);
                                // Text editor
                                let response = ui.add(
                                    egui::TextEdit::multiline(content)
                                        .font(egui::FontId::new(13.0, egui::FontFamily::Monospace))
                                        .desired_width(f32::INFINITY)
                                        .text_color(colors.text_primary),
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
