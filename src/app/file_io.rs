//! File open/save orchestration, delimiter detection, and background
//! Parquet row streaming.

use std::sync::{Arc, Mutex};

use octa::data::{self, DataTable, ViewMode};
use octa::formats::{self};
use octa::ui;
use octa::ui::table_view::TableViewState;

use super::state::{OctaApp, TabState};

/// Whether the post-load date-inference pass should run for a given format.
/// Binary formats with their own typed-date support (Parquet, Arrow, SQLite,
/// DuckDB, SAS, Stata, SPSS, RDS, ORC, Avro, HDF5, GeoPackage) are
/// authoritative — re-typing their columns from string content would only
/// confuse users. Inference runs on text-style formats whose readers leave
/// non-ISO dates as plain strings.
fn date_inference_runs_on(format_name: Option<&str>) -> bool {
    matches!(
        format_name,
        Some("CSV")
            | Some("TSV")
            | Some("JSON")
            | Some("JSON Lines")
            | Some("Excel")
            | Some("XML")
            | Some("TOML")
            | Some("YAML")
            | Some("Markdown")
            | Some("DBF")
    )
}

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

        if let Some(paths) = dialog.pick_files() {
            self.enqueue_open_files(paths);
        }
    }

    /// Queue one or more files for batch open. The first file (if the queue is
    /// empty and no other modal is up) loads immediately; the rest are
    /// drained one per frame from `drain_pending_open_queue`.
    pub(crate) fn enqueue_open_files(&mut self, paths: Vec<std::path::PathBuf>) {
        if paths.is_empty() {
            return;
        }
        for p in paths {
            self.pending_open_queue.push_back(p);
        }
    }

    /// Drain at most one file per frame from the open queue. Pauses while a
    /// table-picker or date-ambiguity dialog is up so the user can resolve
    /// the modal before the next file potentially queues another one.
    pub(crate) fn drain_pending_open_queue(&mut self) {
        if self.pending_table_picker.is_some() || !self.pending_date_pickers.is_empty() {
            return;
        }
        if let Some(path) = self.pending_open_queue.pop_front() {
            self.load_file(path);
        }
    }

    pub(crate) fn load_file(&mut self, path: std::path::PathBuf) {
        // Empty-file easter egg: short-circuit before format dispatch, since
        // most readers will surface a confusing "no schema found" error on a
        // 0-byte file.
        if std::fs::metadata(&path)
            .map(|m| m.len() == 0)
            .unwrap_or(false)
        {
            self.open_empty_file_placeholder(path);
            return;
        }
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
            if tab.table.row_count() > 0 && tab.table.col_count() > 0 {
                tab.table_state.selected_cell = Some((0, 0));
            }
            tab.first_row_is_header = true;
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
            tab.json_file_max_depth = tab
                .json_value
                .as_ref()
                .map(octa::data::json_util::max_json_depth)
                .unwrap_or(0);
            tab.json_expand_depth = tab.json_file_max_depth;
            tab.json_expand_depth_str = tab.json_expand_depth.to_string();

            self.add_recent_file(&path.to_string_lossy());
        }

        // Promote string columns that are uniformly date-shaped. Runs on
        // text-style formats; binary formats already carry typed dates from
        // the reader and would only confuse users by being re-typed here.
        self.run_date_inference_pass(self.active_tab);
    }

    /// Walk the freshly-loaded tab's columns and either (a) promote a
    /// uniformly-formatted string column to typed `Date`/`DateTime`, or (b)
    /// queue a modal date-ambiguity dialog when the values are consistent
    /// with multiple layouts (US vs European).
    fn run_date_inference_pass(&mut self, tab_idx: usize) {
        if tab_idx >= self.tabs.len() {
            return;
        }
        let format_name = self.tabs[tab_idx].table.format_name.clone();
        if !date_inference_runs_on(format_name.as_deref()) {
            return;
        }

        use octa::data::date_infer;
        let col_count = self.tabs[tab_idx].table.col_count();
        for col_idx in 0..col_count {
            let table = &self.tabs[tab_idx].table;
            if !date_infer::column_is_candidate(table, col_idx) {
                continue;
            }
            let collected = date_infer::collect_column_strings(table, col_idx);
            if collected.is_empty() {
                continue;
            }
            match date_infer::infer_column(&collected) {
                date_infer::InferOutcome::Skip => {}
                date_infer::InferOutcome::PromotedDate(layout) => {
                    date_infer::apply_date(&mut self.tabs[tab_idx].table, col_idx, layout);
                    self.tabs[tab_idx].filter_dirty = true;
                    self.tabs[tab_idx].table_state.invalidate_row_heights();
                }
                date_infer::InferOutcome::PromotedDateTime(layout) => {
                    date_infer::apply_datetime(&mut self.tabs[tab_idx].table, col_idx, layout);
                    self.tabs[tab_idx].filter_dirty = true;
                    self.tabs[tab_idx].table_state.invalidate_row_heights();
                }
                date_infer::InferOutcome::AmbiguousDate {
                    candidates,
                    samples,
                } => {
                    let col_name = self.tabs[tab_idx]
                        .table
                        .columns
                        .get(col_idx)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    self.pending_date_pickers
                        .push_back(super::state::DateAmbiguity {
                            tab_idx,
                            col_idx,
                            col_name,
                            samples,
                            date_candidates: candidates,
                            datetime_candidates: Vec::new(),
                        });
                }
                date_infer::InferOutcome::AmbiguousDateTime {
                    candidates,
                    samples,
                } => {
                    let col_name = self.tabs[tab_idx]
                        .table
                        .columns
                        .get(col_idx)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    self.pending_date_pickers
                        .push_back(super::state::DateAmbiguity {
                            tab_idx,
                            col_idx,
                            col_name,
                            samples,
                            date_candidates: Vec::new(),
                            datetime_candidates: candidates,
                        });
                }
            }
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

    /// Open an empty (0-byte) file as a placeholder tab. Skips the format
    /// dispatch path so readers don't surface "missing schema" errors on
    /// genuinely-empty files; renders ASCII art on the central panel instead.
    pub(crate) fn open_empty_file_placeholder(&mut self, path: std::path::PathBuf) {
        let current_empty = self.tabs[self.active_tab].table.col_count() == 0
            && !self.tabs[self.active_tab].is_modified();
        if !current_empty {
            let new_tab = TabState::new(self.settings.default_search_mode);
            self.tabs.push(new_tab);
            self.active_tab = self.tabs.len() - 1;
        }
        let tab = &mut self.tabs[self.active_tab];
        let mut blank = DataTable::empty();
        blank.source_path = Some(path.to_string_lossy().to_string());
        tab.table = blank;
        tab.table_state = TableViewState::default();
        tab.empty_file_placeholder = true;
        tab.view_mode = ViewMode::Table;
        tab.search_text.clear();
        tab.filter_dirty = true;
        self.status_message = Some((
            format!(
                "{} is empty.",
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string())
            ),
            std::time::Instant::now(),
        ));
    }
}
