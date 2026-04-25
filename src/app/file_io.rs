//! File open/save orchestration, delimiter detection, and background
//! Parquet row streaming.

use std::sync::{Arc, Mutex};

use octa::data::{self, DataTable, ViewMode};
use octa::formats::{self};
use octa::ui;
use octa::ui::table_view::TableViewState;

use super::state::{OctaApp, TabState};

/// Shift cell references in a formula to target a specific row. The formula
/// is written as a template using row 1 (e.g. "A1+B1"). For `target_row=4`
/// (0-indexed), references are shifted so row 1 → row 5 (1-indexed).
/// References that already use a different row number are shifted by the same
/// offset.
pub(crate) fn shift_formula_row(formula: &str, target_row: usize) -> String {
    let chars: Vec<char> = formula.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_alphabetic() {
            let col_start = i;
            while i < chars.len() && chars[i].is_ascii_alphabetic() {
                i += 1;
            }
            if i < chars.len() && chars[i].is_ascii_digit() {
                let col_part: String = chars[col_start..i].iter().collect();
                let num_start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let num_str: String = chars[num_start..i].iter().collect();
                if let Ok(orig_row) = num_str.parse::<usize>() {
                    let new_row = target_row + orig_row;
                    result.push_str(&col_part);
                    result.push_str(&new_row.to_string());
                } else {
                    result.push_str(&col_part);
                    result.push_str(&num_str);
                }
            } else {
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

pub(crate) fn detect_delimiter_from_file(path: &std::path::Path) -> u8 {
    use std::io::Read;
    let mut buf = vec![0u8; 1_048_576];
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
pub(crate) fn detect_delimiter_from_content(content: &str) -> u8 {
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

/// Background-load remaining Parquet rows after the initial batch.
/// Writes batches of rows into the shared buffer, which the UI thread drains.
pub(crate) fn load_remaining_parquet_rows(
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

impl OctaApp {
    pub(crate) fn open_file(&mut self) {
        self.do_open_file_dialog();
    }

    pub(crate) fn do_open_file_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new();

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

    pub(crate) fn load_file(&mut self, path: std::path::PathBuf) {
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
            Ok(Some(_)) => {}
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
    pub(crate) fn load_table(&mut self, path: std::path::PathBuf, table_name: String) {
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
    pub(crate) fn apply_loaded_table(&mut self, path: std::path::PathBuf, table: DataTable) {
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

            if tab.table.format_name.as_deref() == Some("CSV") {
                tab.csv_delimiter = detect_delimiter_from_file(&path);
            } else if tab.table.format_name.as_deref() == Some("TSV") {
                tab.csv_delimiter = b'\t';
            }

            let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if file_size <= 500_000_000 {
                tab.raw_content = std::fs::read_to_string(&path).ok();
            } else {
                tab.raw_content = None;
            }
            tab.raw_content_modified = false;

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

            tab.sql_query.clear();
            tab.sql_result = None;
            tab.sql_error = None;
            tab.sql_panel_open =
                self.settings.sql_panel_default_open && tab.view_mode == ViewMode::Table;

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
            tab.json_expand_depth = tab
                .json_value
                .as_ref()
                .map(octa::data::json_util::max_json_depth)
                .unwrap_or(0);
            tab.json_expand_depth_str = tab.json_expand_depth.to_string();

            self.add_recent_file(&path.to_string_lossy());
        }
    }

    pub(crate) fn save_file(&mut self) {
        if let Some(ref path) = self.tabs[self.active_tab].table.source_path.clone() {
            let path = std::path::Path::new(path);
            self.do_save(path.to_path_buf());
        }
    }

    pub(crate) fn save_file_as(&mut self) {
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

    pub(crate) fn export_sql_result(&mut self) {
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

    pub(crate) fn save_tab(&mut self, tab_idx: usize) {
        if let Some(ref path) = self.tabs[tab_idx].table.source_path.clone() {
            let path = std::path::Path::new(path);
            self.do_save_tab(tab_idx, path.to_path_buf());
        }
    }

    pub(crate) fn do_save(&mut self, path: std::path::PathBuf) {
        self.do_save_tab(self.active_tab, path);
    }

    pub(crate) fn do_save_tab(&mut self, tab_idx: usize, path: std::path::PathBuf) {
        let tab = &mut self.tabs[tab_idx];
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
}
