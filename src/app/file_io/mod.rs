//! File open/save orchestration, delimiter detection, and background
//! Parquet row streaming.

use std::sync::{Arc, Mutex};

use octa::data::{self, DataTable, ViewMode};
use octa::formats::{self, FormatReader};
use octa::ui;
use octa::ui::table_view::TableViewState;

use super::state::{OctaApp, TabState};

mod compare;
mod lakehouse;
mod open_as;
mod passes;
mod repair;
mod save;

/// Whether a format-name string belongs to a text-shaped reader (one whose
/// `read_file` opens UTF-8 text on disk). Only these formats are eligible to
/// fall back to a raw text view when parsing fails - binary formats would
/// render as garbage. Update this set when adding a new text reader.
fn format_is_text_fallback_eligible(format_name: &str) -> bool {
    matches!(
        format_name,
        "CSV"
            | "TSV"
            | "JSON"
            | "JSONL"
            | "XML"
            | "YAML"
            | "TOML"
            | "Markdown"
            | "Jupyter Notebook"
            | "Text"
    )
}

/// Files at or above this size read on a background thread so the window stays
/// responsive and a spinner can show. Smaller files read inline (instant, no
/// spinner flash).
const BACKGROUND_LOAD_MIN_BYTES: u64 = 8 * 1024 * 1024;

/// Re-sync a DB-backed table's diff-save baseline from its current rows and
/// column names, so an in-place normalization (whitespace trim, date revert,
/// etc.) isn't later mistaken for user edits or a schema change. No-op for
/// non-DB tables.
pub(crate) fn resync_db_meta_baseline(tab: &mut TabState) {
    if tab.table.db_meta.is_none() {
        return;
    }
    let rows = tab.table.rows.clone();
    let col_names: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();
    if let Some(meta) = tab.table.db_meta.as_mut() {
        for (row_idx, tag) in meta.row_tags.iter().enumerate() {
            if let Some(t) = tag
                && let Some(row) = rows.get(row_idx)
            {
                meta.original.insert(*t, row.clone());
            }
        }
        meta.original_columns = col_names;
    }
}

/// Shift cell references in a formula to target a specific row. The formula
/// is written as a template using row 1 (e.g. "A1+B1"). For `target_row=4`
/// (0-indexed), references are shifted so row 1 -> row 5 (1-indexed).
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

    if !batch_buf.is_empty()
        && let Ok(mut buf) = buffer.lock()
    {
        buf.append(&mut batch_buf);
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

        // "All Supported" filter unions the format registry's extensions with
        // any user-configured "Open as text" extensions, so the picker shows
        // those files without requiring "All Files".
        let mut all_exts = self.registry.all_extensions();
        for ext in &self.settings.text_mode_extensions {
            if !all_exts.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
                all_exts.push(ext.clone());
            }
        }
        let all_ext_refs: Vec<&str> = all_exts.iter().map(|s| s.as_str()).collect();
        dialog = dialog.add_filter("All Supported", &all_ext_refs);

        for (name, exts) in self.registry.format_descriptions() {
            let ext_refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter(&name, &ext_refs);
        }
        // Surface the user's extra extensions as a labelled filter so they
        // can pick "Custom (text)" directly. Skipped when the list is empty.
        if !self.settings.text_mode_extensions.is_empty() {
            let custom_refs: Vec<&str> = self
                .settings
                .text_mode_extensions
                .iter()
                .map(|s| s.as_str())
                .collect();
            dialog = dialog.add_filter("Custom (text)", &custom_refs);
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
        // Wait for any in-flight background read to finish before starting the
        // next queued file (one load at a time).
        if self.pending_load.is_some() {
            return;
        }
        if self.pending_table_picker.is_some()
            || self.pending_sheet_picker.is_some()
            || self.pending_file_repair.is_some()
            || !self.pending_date_pickers.is_empty()
        {
            return;
        }
        if let Some(path) = self.pending_open_queue.pop_front() {
            self.load_file(path);
        }
    }

    /// Like [`Self::load_file`] but guarantees the result lands in a *new*
    /// tab - even if the active tab happened to look "empty" by
    /// `apply_loaded_table`'s heuristic. Pushes an empty placeholder tab
    /// first and switches to it; `load_file` then fills the placeholder.
    ///
    /// Used by the archive viewer's "Open selected entry" action so the
    /// archive listing tab is never accidentally replaced by the extracted
    /// entry's data.
    pub(crate) fn load_file_in_new_tab(&mut self, path: std::path::PathBuf) {
        // Reuse the active tab when it is completely blank (the tab Octa starts
        // with, before anything is open). Pushing a placeholder on top of it
        // would strand it as a stray "Untitled" tab: the tab bar only appears
        // once a second tab exists, so the blank one would suddenly become
        // visible next to the file the user actually opened.
        //
        // A tab holding an archive listing, a table, or a raw-text buffer is not
        // blank, so those still get a new tab and are never clobbered.
        let blank = self
            .tabs
            .get(self.active_tab)
            .map(|t| t.table.col_count() == 0 && t.raw_content.is_none() && !t.is_modified())
            .unwrap_or(false);
        if !blank {
            let placeholder = TabState::new(self.settings.default_search_mode);
            self.tabs.push(placeholder);
            self.active_tab = self.tabs.len() - 1;
        }
        self.load_file(path);
    }

    /// Create a blank Raw document. Reuses the active tab when it is already
    /// completely blank (the tab Octa starts with) instead of stranding it as a
    /// second stray "Untitled" beside the new one; same guard, and same reason,
    /// as `load_file_in_new_tab`.
    pub(crate) fn new_file(&mut self) {
        let blank = self
            .tabs
            .get(self.active_tab)
            .map(|t| t.table.col_count() == 0 && t.raw_content.is_none() && !t.is_modified())
            .unwrap_or(false);
        if !blank {
            self.tabs
                .push(TabState::new(self.settings.default_search_mode));
            self.active_tab = self.tabs.len() - 1;
        }
        let tab = &mut self.tabs[self.active_tab];
        tab.view_mode = ViewMode::Raw;
        tab.raw_content = Some(String::new());
    }

    pub(crate) fn load_file(&mut self, path: std::path::PathBuf) {
        // Directory-open path: Delta Lake / Apache Iceberg tables are
        // directories (a transaction log + Parquet files), not single files,
        // so they bypass the extension-based registry. Detect the marker
        // subdirectory and read via DuckDB; any other directory is ignored.
        if path.is_dir() {
            self.load_lakehouse_dir(path);
            return;
        }
        // Transparent decompression: a `.csv.gz` / `.jsonl.zst` decompresses
        // to a temp file named with the inner extension and loads through the
        // normal path. The temp is kept for the tab's lifetime (the OS temp
        // cleaner frees it); Save re-compresses onto the original path via
        // `TabState.compressed_origin`.
        if let Some(codec) = octa::formats::compression::detect_codec(&path) {
            let cap = if self.settings.max_decompressed_unlimited {
                u64::MAX
            } else {
                self.settings.max_decompressed_bytes
            };
            match octa::formats::compression::decompress_to_temp(&path, codec, cap) {
                Ok(tmp) => {
                    let inner = tmp.path().to_path_buf();
                    if let Err(e) = tmp.keep() {
                        self.status_message = Some((
                            format!("Cannot keep decompressed temp file: {e}"),
                            std::time::Instant::now(),
                        ));
                        return;
                    }
                    self.pending_compressed_origin = Some((path, codec, inner.clone()));
                    self.load_file(inner);
                    return;
                }
                Err(e) => {
                    self.status_message = Some((
                        format!("Cannot decompress {}: {e}", path.display()),
                        std::time::Instant::now(),
                    ));
                    return;
                }
            }
        }
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
        // User-extensible "Open as text" override: if the file's extension is
        // on `settings.text_mode_extensions`, route through TextReader before
        // consulting the format registry. This lets users force unfamiliar
        // log/config extensions to render as raw text instead of failing.
        let ext_lc = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        let force_text = ext_lc
            .as_deref()
            .map(|e| {
                self.settings
                    .text_mode_extensions
                    .iter()
                    .any(|u| u.eq_ignore_ascii_case(e))
            })
            .unwrap_or(false);
        // Opt-in malformed-file repair: probe CSV/TSV files for encoding /
        // delimiter / structure problems and, if found, raise the interactive
        // repair prompt instead of loading. Computed via a short-lived borrow
        // so the `&mut self` call below doesn't conflict with the reader
        // binding. Skipped when the user forced text mode.
        let probe_name = if force_text {
            None
        } else {
            self.registry
                .reader_for_path(&path)
                .map(|r| r.name().to_string())
        };
        if let Some(ref name) = probe_name
            && self.maybe_offer_repair(&path, name)
        {
            return;
        }

        let text_reader: formats::text_reader::TextReader = formats::text_reader::TextReader;
        let reader: &dyn formats::FormatReader = if force_text {
            &text_reader
        } else {
            match self.registry.reader_for_path(&path) {
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
            }
        };

        match reader.list_tables(&path) {
            // Multi-table sources that open *all* tables at once (Excel
            // sheets). Open up to `excel_max_auto_sheets` directly; above
            // that, prompt with a multi-select picker.
            Ok(Some(tables)) if reader.opens_all_tables() && !tables.is_empty() => {
                let names: Vec<String> = tables.iter().map(|t| t.name.clone()).collect();
                let cap = self.settings.excel_max_auto_sheets.max(1);
                if names.len() <= cap {
                    // Read every sheet while the reader borrow is alive, then
                    // apply (apply_loaded_table needs `&mut self`).
                    let loaded: Vec<(String, anyhow::Result<DataTable>)> = names
                        .iter()
                        .map(|n| (n.clone(), reader.read_table(&path, n)))
                        .collect();
                    for (name, res) in loaded {
                        match res {
                            Ok(table) => self.apply_loaded_table(path.clone(), table),
                            Err(e) => {
                                self.status_message = Some((
                                    format!("Error reading sheet '{name}': {e}"),
                                    std::time::Instant::now(),
                                ));
                            }
                        }
                    }
                } else {
                    let selected = (0..names.len()).map(|i| i < cap).collect();
                    self.pending_sheet_picker = Some(super::state::SheetPickerState {
                        path,
                        sheet_names: names,
                        selected,
                    });
                }
                return;
            }
            Ok(Some(tables)) if tables.len() > 1 => {
                self.pending_table_picker = Some(ui::table_picker::TablePickerState {
                    path,
                    format_name: reader.name().to_string(),
                    tables,
                    selected: 0,
                    visible_rows: self.settings.table_picker_visible_rows,
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

        let format_name = reader.name().to_string();
        let file_len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if file_len >= BACKGROUND_LOAD_MIN_BYTES {
            // Background the slow read so the window stays responsive. Re-resolve
            // the reader inside the worker (FormatRegistry::new is cheap and
            // IO-free) so we don't move a borrowed `&dyn FormatReader` across
            // threads. Multi-table sources already returned earlier, so this is
            // always a single read_file.
            let (tx, rx) = std::sync::mpsc::channel();
            let worker_path = path.clone();
            let worker_format = format_name.clone();
            let force_text_worker = force_text;
            std::thread::spawn(move || {
                let registry = formats::FormatRegistry::new();
                let text_reader = formats::text_reader::TextReader;
                // Re-resolve the reader by name so neither `registry` nor
                // `text_reader` need to outlive the closure (both live for the
                // whole body, avoiding lifetime trouble with the two borrows).
                let result = if force_text_worker {
                    text_reader.read_file(&worker_path)
                } else {
                    registry
                        .reader_by_name(&worker_format)
                        .unwrap_or(&text_reader)
                        .read_file(&worker_path)
                };
                let _ = tx.send(result);
            });
            self.pending_load = Some(super::state::PendingLoad {
                path,
                format_name,
                rx,
            });
            return;
        }
        let result = reader.read_file(&path);
        self.finish_single_load(path, format_name, result);
    }

    /// Poll the background file-read worker (if any). On completion, apply the
    /// result; while it runs, keep repainting so the status-bar spinner
    /// animates and the window stays responsive.
    pub(crate) fn drive_pending_load(&mut self, ctx: &eframe::egui::Context) {
        let Some(pending) = self.pending_load.as_ref() else {
            return;
        };
        match pending.rx.try_recv() {
            Ok(result) => {
                let pending = self.pending_load.take().unwrap();
                self.finish_single_load(pending.path, pending.format_name, result);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let pending = self.pending_load.take().unwrap();
                self.finish_single_load(
                    pending.path,
                    pending.format_name,
                    Err(anyhow::anyhow!("file read worker stopped unexpectedly")),
                );
            }
        }
    }

    /// Apply the result of a single-table read (sync or backgrounded). On error,
    /// try a content-sniff reload, then a raw-text fallback, else show a status.
    pub(crate) fn finish_single_load(
        &mut self,
        path: std::path::PathBuf,
        format_name: String,
        result: anyhow::Result<DataTable>,
    ) {
        match result {
            Ok(table) => self.apply_loaded_table(path, table),
            Err(e) => {
                if self.try_content_sniff_reload(&path, &format_name) {
                    return;
                }
                if format_is_text_fallback_eligible(&format_name) {
                    self.fallback_to_raw_text(path, format_name, e);
                } else {
                    self.status_message = Some((
                        format!("Error reading file: {}", e),
                        std::time::Instant::now(),
                    ));
                }
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

        // Compressed-file provenance: stamp the original path + codec on the
        // tab and show the original name (`data.csv.gz`), not the temp's.
        // Claimed only when this apply is for the recorded temp path.
        let compressed_origin = match std::mem::take(&mut self.pending_compressed_origin) {
            Some((orig, codec, temp)) if temp == path => Some(super::state::CompressedOrigin {
                original: orig,
                codec,
                temp,
            }),
            _ => None,
        };

        {
            let tab = &mut self.tabs[self.active_tab];
            tab.compressed_origin = compressed_origin.clone();
            if let Some(o) = &compressed_origin {
                tab.custom_tab_label = o
                    .original
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string());
            }
            tab.table = table;
            tab.table_state = TableViewState::default();
            if tab.table.row_count() > 0 && tab.table.col_count() > 0 {
                tab.table_state.selected_cell = Some((0, 0));
            }
            tab.first_row_is_header = true;
            tab.search_text.clear();
            tab.search_nav.reset();
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
            if self.settings.raw_view_allows(file_size) {
                tab.raw_content = std::fs::read_to_string(&path).ok();
            } else {
                tab.raw_content = None;
            }
            tab.raw_content_original = tab.raw_content.clone();
            tab.raw_content_modified = false;
            tab.raw_color_enabled = true;
            tab.raw_file_size = Some(file_size);
            tab.raw_perf_prompt_resolved = false;

            // Reset any EPUB side-state - populated below for actual EPUB
            // files, cleared here so a non-EPUB tab can't inherit it on
            // reload. Textures aren't persisted across loads (they hold
            // GPU handles tied to the previous tab content).
            tab.epub_chapters_md.clear();
            tab.epub_chapter_titles.clear();
            tab.epub_image_bytes.clear();
            tab.epub_image_textures.clear();
            tab.epub_active_chapter = 0;
            tab.epub_title = None;

            // Same reset for the GeoJSON side-state - empties out anything
            // a previous file on this tab might have left. The `walkers`
            // tile + memory boxes are dropped so the next Map render
            // creates fresh ones tied to the current egui context.
            tab.geojson_features.clear();
            tab.map_tiles = None;
            tab.map_memory = None;
            tab.map_mode = self.settings.map_default_mode;

            if tab.table.format_name.as_deref() == Some("Markdown") {
                tab.view_mode = ViewMode::Markdown;
            } else if tab.table.format_name.as_deref() == Some("Jupyter Notebook") {
                tab.view_mode = ViewMode::Notebook;
            } else if tab.table.format_name.as_deref() == Some("Text") {
                tab.view_mode = ViewMode::Raw;
            } else if tab.table.format_name.as_deref() == Some("EPUB") {
                // Read the chapter Markdown + image bytes from a second
                // pass over the file. The table view is still available
                // (paragraph-per-row) but the reading view is the default.
                if let Ok((_, extras)) = octa::formats::epub_reader::read_with_extras(&path) {
                    tab.epub_chapters_md = extras.chapters_md;
                    tab.epub_chapter_titles = extras.chapter_titles;
                    tab.epub_image_bytes = extras.image_bytes;
                    tab.epub_title = extras.title;
                }
                tab.view_mode = ViewMode::EpubReader;
            } else if tab.table.format_name.as_deref() == Some("GeoJSON") {
                // Re-parse for `geo-types` geometries - the registry's
                // `read_file` only returned the table, so we make a second
                // pass to populate the Map view's side-state. The map
                // widget itself is initialised lazily in the view (it
                // needs the egui `Context`).
                if let Ok((_, extras)) = octa::formats::geojson_reader::read_with_features(&path) {
                    tab.geojson_features = extras.features;
                }
                tab.view_mode = ViewMode::Map;
            } else if tab.table.format_name.as_deref() == Some("Shapefile") {
                // Same second-pass shape as GeoJSON: the registry returned the
                // table, so re-read for the `geo-types` geometries the Map
                // view renders.
                if let Ok((_, features)) =
                    octa::formats::shapefile_reader::read_with_features(&path)
                {
                    tab.geojson_features = features;
                }
                tab.view_mode = ViewMode::Map;
            } else {
                tab.view_mode = ViewMode::Table;
            }

            tab.sql_query.clear();
            tab.sql_result = None;
            tab.sql_error = None;
            tab.sql_panel_open =
                self.settings.sql_panel_default_open && tab.view_mode == ViewMode::Table;
            tab.sql_editor_focus_pending = tab.sql_panel_open;

            tab.parse_error_banner = None;
            tab.json_value = None;
            tab.yaml_value = None;
            tab.json_tree_expanded.clear();
            if matches!(
                tab.table.format_name.as_deref(),
                Some("JSON") | Some("JSONL")
            ) {
                if let Some(ref content) = tab.raw_content {
                    tab.json_value = serde_json::from_str(content).ok();
                }
            } else if matches!(tab.table.format_name.as_deref(), Some("YAML"))
                && let Some(ref content) = tab.raw_content
            {
                tab.yaml_value = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(content)
                    .ok()
                    .map(|v| octa::formats::yaml_reader::yaml_to_json(&v));
            }
            // Initial view for structured-text formats (overrides the Table
            // default chosen above): a `.json` file opens as a collapsible tree
            // when it parsed, `.yml`/`.yaml`/`.toml`/`.xml` open as raw text.
            // All can still be switched via the View menu. JSONL stays tabular.
            // Gating JSON on `json_value.is_some()` keeps `view_mode` in sync
            // with `available_view_modes`, which only offers JsonTree when parsed.
            if tab.table.format_name.as_deref() == Some("JSON") && tab.json_value.is_some() {
                tab.view_mode = ViewMode::JsonTree;
                tab.sql_panel_open = false;
                tab.sql_editor_focus_pending = false;
            } else if matches!(
                tab.table.format_name.as_deref(),
                Some("YAML") | Some("TOML") | Some("XML")
            ) {
                tab.view_mode = ViewMode::Raw;
                tab.sql_panel_open = false;
                tab.sql_editor_focus_pending = false;
            }
            // Both trees share the depth+expand tracking fields, since only
            // one tree view is shown per tab at a time.
            let tree_root = tab.json_value.as_ref().or(tab.yaml_value.as_ref());
            tab.json_file_max_depth = tree_root
                .map(octa::data::json_util::max_json_depth)
                .unwrap_or(0);
            tab.json_expand_depth = tab.json_file_max_depth;
            tab.json_expand_depth_str = tab.json_expand_depth.to_string();

            self.add_recent_file(&path.to_string_lossy());
        }

        // Strip leading/trailing whitespace from string cells (load-time
        // normalization, gated by the setting). Runs before date inference so
        // a value like " 2024-01-02 " can still be recognised as a date.
        self.run_trim_pass(self.active_tab);

        // Normalise column headers to snake_case identifiers (gated by the
        // setting). Runs after trimming so titles are already whitespace-clean.
        self.run_clean_headers_pass(self.active_tab);

        // Promote string columns that are uniformly date-shaped. Runs for
        // every format - the candidate check (`date_infer::column_is_candidate`)
        // only ever touches `Utf8` string columns, so typed Date/Timestamp
        // columns produced by binary readers (Parquet, Arrow, SQLite, ...) are
        // left untouched; only genuine text columns get promoted.
        self.run_date_inference_pass(self.active_tab);
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
        tab.search_nav.reset();
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
