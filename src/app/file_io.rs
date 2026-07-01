//! File open/save orchestration, delimiter detection, and background
//! Parquet row streaming.

use std::sync::{Arc, Mutex};

use octa::data::{self, DataTable, ViewMode};
use octa::formats::{self, FormatReader};
use octa::ui;
use octa::ui::table_view::TableViewState;

use super::state::{OctaApp, TabState};

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

/// Build a small preview grid (row 0 is the header) from a table, capped to
/// `max_rows` data rows and `max_cols` columns. Used by the malformed-file
/// repair dialog to show what the repaired result looks like.
fn repair_preview_rows(table: &DataTable, max_rows: usize, max_cols: usize) -> Vec<Vec<String>> {
    let ncols = table.col_count().min(max_cols);
    let mut out: Vec<Vec<String>> = Vec::new();
    out.push(
        table
            .columns
            .iter()
            .take(ncols)
            .map(|c| c.name.clone())
            .collect(),
    );
    for r in 0..table.row_count().min(max_rows) {
        out.push(
            (0..ncols)
                .map(|c| table.get(r, c).map(|v| v.to_string()).unwrap_or_default())
                .collect(),
        );
    }
    out
}

/// Apply per-column value-rounding formats to a table in place. Only columns
/// whose format `rounds_values()` are touched; `Int` / `Float` cells are
/// replaced with their rounded `Float`. Used to build the on-disk snapshot
/// when the user opts to "save rounded".
fn round_table_in_place(
    table: &mut octa::data::DataTable,
    formats: &std::collections::HashMap<usize, octa::data::num_format::NumberFormat>,
) {
    use octa::data::CellValue;
    use octa::data::num_format::round_value;
    for (&col_idx, fmt) in formats {
        if !fmt.rounds_values() || col_idx >= table.col_count() {
            continue;
        }
        for row in &mut table.rows {
            if let Some(cell) = row.get_mut(col_idx) {
                let rounded = match cell {
                    CellValue::Int(n) => Some(round_value(*n as f64, *fmt)),
                    CellValue::Float(f) => Some(round_value(*f, *fmt)),
                    _ => None,
                };
                if let Some(v) = rounded {
                    *cell = CellValue::Float(v);
                }
            }
        }
    }
}

/// Capture the source-string content of every cell in `col_idx` (None for
/// pre-existing nulls). Used right before date promotion so the warning
/// banner's Dismiss button can revert the column back to its on-disk shape.
fn snapshot_column_strings(table: &octa::data::DataTable, col_idx: usize) -> Vec<Option<String>> {
    use octa::data::CellValue;
    let mut out = Vec::with_capacity(table.row_count());
    for row in 0..table.row_count() {
        out.push(match table.get(row, col_idx) {
            Some(CellValue::String(s)) => Some(s.clone()),
            Some(CellValue::Null) | None => None,
            Some(other) => Some(other.to_string()),
        });
    }
    out
}

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

    /// Trigger the "Compare with..." flow: pick a right-side file and load
    /// both raw content (for TextDiff) and a `DataTable` (for RowHashDiff)
    /// onto the active tab's `compare_*` fields. Switches the active tab
    /// into `ViewMode::Compare` on success.
    pub(crate) fn begin_compare_with(&mut self) {
        let mut dialog = rfd::FileDialog::new();
        let mut all_exts = self.registry.all_extensions();
        for ext in &self.settings.text_mode_extensions {
            if !all_exts.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
                all_exts.push(ext.clone());
            }
        }
        let all_ext_refs: Vec<&str> = all_exts.iter().map(|s| s.as_str()).collect();
        dialog = dialog.add_filter("All Supported", &all_ext_refs);
        dialog = dialog.add_filter("All Files", &["*"]);
        let Some(path) = dialog.pick_file() else {
            return;
        };

        // Best-effort raw text load, gated by the configurable raw-view size
        // cap (read before the mutable tab borrow below).
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let raw_allowed = self.settings.raw_view_allows(size);

        let tab = &mut self.tabs[self.active_tab];
        tab.compare_error = None;
        tab.compare_right_path = Some(path.clone());

        tab.compare_right_raw = if raw_allowed {
            std::fs::read_to_string(&path).ok()
        } else {
            None
        };

        // Best-effort DataTable load via the registry. Failures fall
        // through with `compare_right_table = None` - the row-diff
        // renderer surfaces a friendly message in that case.
        tab.compare_right_table = match self.registry.reader_for_path(&path) {
            Some(r) => match r.read_file(&path) {
                Ok(t) => Some(Box::new(t)),
                Err(e) => {
                    tab.compare_error = Some(format!("Failed to load right side as table: {e}"));
                    None
                }
            },
            None => None,
        };

        // Reset column picks so the user starts from "hash every column"
        // until they pick. Without this, stale picks from a previous
        // compare would silently leak in.
        tab.compare_columns_left.clear();
        tab.compare_columns_right.clear();

        // Pick a sensible default sub-mode: TextDiff only makes sense when
        // both sides have raw text content. For two binary tabular files
        // (Parquet vs Parquet, etc.) jump straight to RowHashDiff so the
        // user doesn't see an empty diff and have to manually flip.
        let both_have_text = tab.raw_content.is_some() && tab.compare_right_raw.is_some();
        tab.compare_mode = if both_have_text {
            octa::data::CompareMode::TextDiff
        } else {
            octa::data::CompareMode::RowHashDiff
        };
        tab.view_mode = octa::data::ViewMode::Compare;
    }

    /// Build and open the git revision-picker for the active tab. Surfaces a
    /// status message (no dialog) when the file is not in a git repo.
    pub(crate) fn open_git_compare_dialog(&mut self) {
        let Some(src) = self.tabs[self.active_tab].table.source_path.clone() else {
            self.status_message = Some((
                "Save the file first; git compare needs a file on disk.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        };
        let path = std::path::PathBuf::from(&src);
        let Some(root) = octa::git::repo_root(&path) else {
            self.status_message = Some((
                octa::i18n::t("dialog.gitcmp_not_repo"),
                std::time::Instant::now(),
            ));
            return;
        };
        let Some(relpath) = octa::git::relative_path(&path, &root) else {
            self.status_message = Some((
                octa::i18n::t("dialog.gitcmp_not_tracked"),
                std::time::Instant::now(),
            ));
            return;
        };
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let commits = octa::git::recent_commits(&root, &relpath, 20);
        self.git_compare_dialog = Some(crate::app::state::GitCompareState {
            repo_root: root,
            relpath,
            ext,
            commits,
            selected_rev: "HEAD".to_string(),
            selected_label: octa::i18n::t("dialog.gitcmp_head"),
            size: octa::ui::settings::DialogSize::default(),
        });
    }

    /// Load the comparison "right side" from in-memory bytes (the git compare
    /// path). `ext` is the original file extension (no dot) so the temp file
    /// routes to the correct reader. Mirrors the populate logic in
    /// `begin_compare_with`.
    pub(crate) fn begin_compare_with_git_bytes(
        &mut self,
        bytes: Vec<u8>,
        ext: &str,
        label: String,
    ) {
        use std::io::Write;
        // `tempfile::Builder` borrows the suffix by reference, so it must
        // outlive the builder (hence the binding before `builder`).
        let suffix = if ext.is_empty() {
            String::new()
        } else {
            format!(".{ext}")
        };
        let mut builder = tempfile::Builder::new();
        builder.prefix("octa-git-");
        if !suffix.is_empty() {
            builder.suffix(suffix.as_str());
        }
        let tmp = match builder.tempfile() {
            Ok(mut f) => {
                if f.write_all(&bytes).is_err() {
                    self.status_message = Some((
                        "Failed to write temporary git file".to_string(),
                        std::time::Instant::now(),
                    ));
                    return;
                }
                f
            }
            Err(e) => {
                self.status_message =
                    Some((format!("Temp file error: {e}"), std::time::Instant::now()));
                return;
            }
        };
        let path = tmp.path().to_path_buf();
        let size = bytes.len() as u64;
        let raw_allowed = self.settings.raw_view_allows(size);

        let tab = &mut self.tabs[self.active_tab];
        tab.compare_error = None;
        tab.compare_right_path = Some(path.clone());
        tab.compare_right_raw = if raw_allowed {
            String::from_utf8(bytes).ok()
        } else {
            None
        };
        tab.compare_right_table = match self.registry.reader_for_path(&path) {
            Some(r) => match r.read_file(&path) {
                Ok(t) => Some(Box::new(t)),
                Err(e) => {
                    tab.compare_error = Some(format!("Failed to load git version as table: {e}"));
                    None
                }
            },
            None => None,
        };
        tab.compare_columns_left.clear();
        tab.compare_columns_right.clear();
        let both_have_text = tab.raw_content.is_some() && tab.compare_right_raw.is_some();
        tab.compare_mode = if both_have_text {
            octa::data::CompareMode::TextDiff
        } else {
            octa::data::CompareMode::RowHashDiff
        };
        tab.view_mode = octa::data::ViewMode::Compare;
        // The table was cloned into `compare_right_table` and the raw text
        // copied, so the temp file can drop now.
        drop(tmp);
        self.status_message = Some((
            format!("{}: {label}", octa::i18n::t("dialog.gitcmp_title")),
            std::time::Instant::now(),
        ));
    }

    /// Write git bytes to a temp file and open it as a new tab (the "Open git
    /// version" action). The temp file is leaked (`keep()`) rather than deleted
    /// on return: a file >= `BACKGROUND_LOAD_MIN_BYTES` is read on a worker
    /// thread that outlives this call, so deleting the temp here would race the
    /// read. (Same reason `open_cloud_object` keeps its temp; the OS cleans /tmp.)
    pub(crate) fn open_git_bytes_in_new_tab(
        &mut self,
        bytes: Vec<u8>,
        ext: &str,
        relpath: &str,
        rev: &str,
    ) {
        use std::io::Write;
        let suffix = if ext.is_empty() {
            String::new()
        } else {
            format!(".{ext}")
        };
        let mut builder = tempfile::Builder::new();
        builder.prefix("octa-git-");
        if !suffix.is_empty() {
            builder.suffix(suffix.as_str());
        }
        let mut tmp = match builder.tempfile() {
            Ok(f) => f,
            Err(e) => {
                self.status_message =
                    Some((format!("Temp file error: {e}"), std::time::Instant::now()));
                return;
            }
        };
        if tmp.write_all(&bytes).is_err() {
            self.status_message = Some((
                "Failed to write temporary git file".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        let path = tmp.path().to_path_buf();
        self.load_file_in_new_tab(path);
        // Label the new tab so it reads as a git version, not the temp filename.
        if let Some(tab) = self.tabs.last_mut() {
            let name = std::path::Path::new(relpath)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| relpath.to_string());
            tab.custom_tab_label = Some(format!("{name} @ {rev}"));
        }
        // Leak the temp so a backgrounded (>= 8 MB) read still finds it.
        let _ = tmp.keep();
    }

    /// Compare the active tab against a **sibling tab** (no file picker).
    /// Used by:
    ///   - The tab right-click context menu ("Compare with active tab").
    ///   - The `CompareSelectedTabs` shortcut (when exactly one tab is in
    ///     `tab_multi_selection`).
    ///
    /// Clones the sibling's state into the active tab's `compare_*`
    /// fields and switches the active tab into `ViewMode::Compare`. No-op
    /// if `target_idx` is the active tab or out of range.
    pub(crate) fn begin_compare_with_tab(&mut self, target_idx: usize) {
        if target_idx == self.active_tab || target_idx >= self.tabs.len() {
            return;
        }
        // Read out everything we need from the sibling tab before borrowing
        // the active tab mutably - split-borrowing two indices of the same
        // Vec needs the snapshots be plain values, not references.
        let right_path = self.tabs[target_idx]
            .table
            .source_path
            .as_ref()
            .map(std::path::PathBuf::from);
        let right_raw = self.tabs[target_idx].raw_content.clone();
        let right_table = self.tabs[target_idx].table.clone();
        let left_has_text = self.tabs[self.active_tab].raw_content.is_some();

        let tab = &mut self.tabs[self.active_tab];
        tab.compare_error = None;
        tab.compare_right_path = right_path;
        tab.compare_right_raw = right_raw;
        tab.compare_right_table = Some(Box::new(right_table));
        tab.compare_columns_left.clear();
        tab.compare_columns_right.clear();
        let both_have_text = left_has_text && tab.compare_right_raw.is_some();
        tab.compare_mode = if both_have_text {
            octa::data::CompareMode::TextDiff
        } else {
            octa::data::CompareMode::RowHashDiff
        };
        tab.view_mode = octa::data::ViewMode::Compare;
        // Clear the staging set since we've consumed it.
        self.tab_multi_selection.clear();
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
        let placeholder = TabState::new(self.settings.default_search_mode);
        self.tabs.push(placeholder);
        self.active_tab = self.tabs.len() - 1;
        self.load_file(path);
    }

    /// Open a directory as a Delta Lake / Apache Iceberg table. Detects the
    /// format from the marker subdirectory and reads it via DuckDB; reports a
    /// clear status message for plain directories or read failures (e.g. the
    /// `delta`/`iceberg` extension failing to install offline).
    fn load_lakehouse_dir(&mut self, path: std::path::PathBuf) {
        let Some(kind) = octa::formats::lakehouse_reader::detect(&path) else {
            self.status_message = Some((
                format!(
                    "Not a Delta Lake or Iceberg table directory: {}",
                    path.display()
                ),
                std::time::Instant::now(),
            ));
            return;
        };
        match octa::formats::lakehouse_reader::read_dir(&path, kind) {
            Ok(table) => self.apply_loaded_table(path, table),
            Err(e) => {
                self.status_message = Some((
                    format!("Error reading {} table: {e}", kind.format_name()),
                    std::time::Instant::now(),
                ));
            }
        }
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

    /// After an extension-chosen reader fails, attempt to reload using the
    /// reader identified by [`formats::sniff::sniff_format`]. Handles the
    /// wrong-extension case (a binary format with a misleading extension).
    /// Returns `true` when a sniffed reader successfully loaded the file.
    ///
    /// Multi-table sources (databases, Excel) are skipped here: they keep
    /// conventional extensions, and the missing-extension case is already
    /// handled in `reader_for_path` (which routes them through the normal
    /// table-picker dispatch).
    fn try_content_sniff_reload(&mut self, path: &std::path::Path, failed_format: &str) -> bool {
        let Some(name) = formats::sniff::sniff_format(path) else {
            return false;
        };
        if name == failed_format {
            return false;
        }
        let Some(reader) = self.registry.reader_by_name(name) else {
            return false;
        };
        // Skip multi-table sniff targets (e.g. SQLite via a wrong extension);
        // we can't raise the picker cleanly from here.
        if matches!(reader.list_tables(path), Ok(Some(ref t)) if !t.is_empty()) {
            return false;
        }
        match reader.read_file(path) {
            Ok(table) => {
                self.apply_loaded_table(path.to_path_buf(), table);
                true
            }
            Err(_) => false,
        }
    }

    /// If malformed-file repair is enabled and the CSV/TSV at `path` looks
    /// malformed, stage the interactive repair prompt and return `true` (the
    /// caller should stop loading). Returns `false` for healthy files, other
    /// formats, or when the setting is off.
    fn maybe_offer_repair(&mut self, path: &std::path::Path, reader_name: &str) -> bool {
        if !self.settings.offer_repair_on_malformed {
            return false;
        }
        let default_delim = match reader_name {
            "CSV" => formats::csv_reader::detect_delimiter(path).unwrap_or(b','),
            "TSV" => b'\t',
            _ => return false,
        };
        let Some(plan) = formats::csv_reader::analyze_delimited(path, default_delim) else {
            return false;
        };
        let preview = formats::csv_reader::read_delimited_opts(
            path,
            default_delim,
            reader_name,
            &plan.options,
        )
        .ok()
        .map(|t| repair_preview_rows(&t, 8, 8))
        .unwrap_or_default();
        self.pending_file_repair = Some(super::state::FileRepair {
            path: path.to_path_buf(),
            format_name: reader_name.to_string(),
            default_delimiter: default_delim,
            issues: plan.issues,
            options: plan.options,
            preview,
        });
        true
    }

    /// Recompute the repaired preview for the pending repair prompt after the
    /// user toggled an option (e.g. "keep extra values"). Best effort: leaves
    /// the existing preview in place if the re-read fails.
    pub(crate) fn refresh_repair_preview(&mut self) {
        let Some(repair) = self.pending_file_repair.as_ref() else {
            return;
        };
        if let Ok(table) = formats::csv_reader::read_delimited_opts(
            &repair.path,
            repair.default_delimiter,
            &repair.format_name,
            &repair.options,
        ) {
            let preview = repair_preview_rows(&table, 8, 8);
            if let Some(r) = self.pending_file_repair.as_mut() {
                r.preview = preview;
            }
        }
    }

    /// Resolve a pending repair prompt. `apply_repair = true` applies the
    /// suggested fixes; `false` opens the file without repair (lossy decode
    /// only, so a bad-encoding file still loads). Clears the pending state.
    pub(crate) fn resolve_file_repair(&mut self, apply_repair: bool) {
        let Some(repair) = self.pending_file_repair.take() else {
            return;
        };
        let opts = if apply_repair {
            repair.options.clone()
        } else {
            formats::csv_reader::ReadOptions {
                lossy_utf8: true,
                delimiter: Some(repair.default_delimiter),
                strip_bom_controls: false,
                preserve_ragged: false,
            }
        };
        match formats::csv_reader::read_delimited_opts(
            &repair.path,
            repair.default_delimiter,
            &repair.format_name,
            &opts,
        ) {
            Ok(table) => self.apply_loaded_table(repair.path, table),
            Err(e) => {
                self.status_message = Some((
                    format!("Failed to open {}: {e}", repair.path.display()),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    /// Open a file as plain text after a parse failure, surfacing a banner
    /// above the raw view that explains the original format's error. Only
    /// invoked for text-shaped formats - binary formats (parquet, xlsx, ...)
    /// would render as garbage and skip this fallback.
    fn fallback_to_raw_text(
        &mut self,
        path: std::path::PathBuf,
        format_name: String,
        err: anyhow::Error,
    ) {
        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if !self.settings.raw_view_allows(file_size) {
            self.status_message = Some((
                format!(
                    "Failed to parse {format_name}: {err}. File exceeds the raw-view size cap (Settings -> Performance)."
                ),
                std::time::Instant::now(),
            ));
            return;
        }
        let banner = format!("Failed to parse as {format_name}: {err}");
        match formats::text_reader::TextReader.read_file(&path) {
            Ok(table) => {
                self.apply_loaded_table(path, table);
                let tab = &mut self.tabs[self.active_tab];
                tab.view_mode = ViewMode::Raw;
                tab.parse_error_banner = Some(banner);
            }
            Err(text_err) => {
                self.status_message = Some((
                    format!(
                        "Failed to parse as {format_name}: {err}. Raw text fallback also failed: {text_err}"
                    ),
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

    /// Strip leading/trailing whitespace from every string cell in the tab's
    /// table when `trim_whitespace_on_load` is on. For DB-backed tables the
    /// `db_meta.original` snapshot is re-synced from the trimmed rows so the
    /// diff-on-save logic doesn't mistake trimming for user edits. Surfaces a
    /// dismissible banner listing the affected columns when
    /// `warn_on_whitespace_trim` is on.
    fn run_trim_pass(&mut self, tab_idx: usize) {
        if !self.settings.trim_whitespace_on_load || tab_idx >= self.tabs.len() {
            return;
        }
        let tab = &mut self.tabs[tab_idx];
        let (trimmed, undo) = octa::data::trim::trim_string_columns_tracked(&mut tab.table);
        if trimmed.is_empty() {
            return;
        }
        // Re-sync the DB diff-save baseline so trimming (cells or titles)
        // isn't seen as edits / a schema change.
        resync_db_meta_baseline(tab);
        tab.filter_dirty = true;
        if self.settings.warn_on_whitespace_trim {
            self.pending_trim_warning = Some(super::state::TrimWarning {
                tab_idx,
                columns: trimmed,
                undo,
            });
        }
    }

    /// Normalise the tab's column headers to lower snake_case identifiers when
    /// `clean_headers_on_load` is on. For DB-backed tables the diff-save
    /// baseline is re-synced so the rename isn't seen as a schema change.
    /// Surfaces a status message listing how many headers changed.
    fn run_clean_headers_pass(&mut self, tab_idx: usize) {
        if !self.settings.clean_headers_on_load || tab_idx >= self.tabs.len() {
            return;
        }
        let tab = &mut self.tabs[tab_idx];
        let changed = octa::data::trim::clean_headers(&mut tab.table);
        if changed.is_empty() {
            return;
        }
        resync_db_meta_baseline(tab);
        tab.filter_dirty = true;
        tab.table_state.widths_initialized = false;
        self.status_message = Some((
            octa::i18n::t("settings.clean_headers_status")
                .replace("{n}", &changed.len().to_string()),
            std::time::Instant::now(),
        ));
    }

    /// Walk the freshly-loaded tab's columns and either (a) promote a
    /// uniformly-formatted string column to typed `Date`/`DateTime`, or (b)
    /// queue a modal date-ambiguity dialog when the values are consistent
    /// with multiple layouts (US vs European).
    fn run_date_inference_pass(&mut self, tab_idx: usize) {
        if tab_idx >= self.tabs.len() {
            return;
        }

        use octa::data::date_infer;
        let col_count = self.tabs[tab_idx].table.col_count();
        let mut format_changes: Vec<super::state::DatePromotionInfo> = Vec::new();
        let mut parse_failures: Vec<super::state::DateParseFailure> = Vec::new();
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
                    let col_name = self.tabs[tab_idx]
                        .table
                        .columns
                        .get(col_idx)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    let snapshot = if layout.is_canonical() {
                        Vec::new()
                    } else {
                        snapshot_column_strings(&self.tabs[tab_idx].table, col_idx)
                    };
                    date_infer::apply_date(&mut self.tabs[tab_idx].table, col_idx, layout);
                    self.tabs[tab_idx].filter_dirty = true;
                    self.tabs[tab_idx].table_state.invalidate_row_heights();
                    if !layout.is_canonical() {
                        format_changes.push(super::state::DatePromotionInfo {
                            col_idx,
                            column_name: col_name,
                            source_label: layout.label(),
                            original_values: snapshot,
                        });
                    }
                }
                date_infer::InferOutcome::PromotedDateTime(layout) => {
                    let col_name = self.tabs[tab_idx]
                        .table
                        .columns
                        .get(col_idx)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    let snapshot = if layout.is_canonical() {
                        Vec::new()
                    } else {
                        snapshot_column_strings(&self.tabs[tab_idx].table, col_idx)
                    };
                    date_infer::apply_datetime(&mut self.tabs[tab_idx].table, col_idx, layout);
                    self.tabs[tab_idx].filter_dirty = true;
                    self.tabs[tab_idx].table_state.invalidate_row_heights();
                    if !layout.is_canonical() {
                        format_changes.push(super::state::DatePromotionInfo {
                            col_idx,
                            column_name: col_name,
                            source_label: layout.label(),
                            original_values: snapshot,
                        });
                    }
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
                date_infer::InferOutcome::Failed {
                    label,
                    parsed,
                    total,
                    failures,
                } => {
                    let col_name = self.tabs[tab_idx]
                        .table
                        .columns
                        .get(col_idx)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    parse_failures.push(super::state::DateParseFailure {
                        column_name: col_name,
                        source_label: label,
                        parsed,
                        total,
                        samples: failures,
                    });
                }
            }
        }

        if !format_changes.is_empty() && self.settings.warn_on_date_format_change {
            self.pending_date_warning = Some(super::state::DateWarning {
                tab_idx,
                entries: format_changes,
            });
        }
        if !parse_failures.is_empty() && self.settings.warn_on_date_format_change {
            self.pending_date_parse_warning = Some(super::state::DateParseWarning {
                tab_idx,
                entries: parse_failures,
            });
        }
    }

    pub(crate) fn save_file(&mut self) {
        // Cloud-opened tab: block when writes are disabled (the user can still
        // Save As to a local copy), otherwise upload after the local write.
        if self.tabs[self.active_tab].cloud_origin.is_some() && !self.settings.cloud_writes_enabled
        {
            self.status_message = Some((
                octa::i18n::t("cloud.write_disabled"),
                std::time::Instant::now(),
            ));
            return;
        }
        if let Some(ref path) = self.tabs[self.active_tab].table.source_path.clone() {
            let path = std::path::Path::new(path);
            // Regular save writes the full table back to the source path,
            // never the filtered view - the file on disk represents the
            // user's data, not their current view.
            self.do_save(path.to_path_buf(), false);
            self.maybe_upload_cloud(self.active_tab, path);
        }
    }

    pub(crate) fn save_file_as(&mut self) {
        let mut dialog = rfd::FileDialog::new();
        for (label, exts) in self.registry.save_format_descriptions() {
            let ext_refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter(&label, &ext_refs);
        }
        if let Some(ref source) = self.tabs[self.active_tab].table.source_path
            && let Some(name) = std::path::Path::new(source).file_name()
        {
            dialog = dialog.set_file_name(name.to_string_lossy().to_string());
        }

        if let Some(path) = dialog.save_file() {
            // Save As respects the current row filter (text search + column
            // filters). The on-disk output mirrors what the user sees; the
            // in-memory table is left intact.
            let new_path = path.to_string_lossy().to_string();
            self.do_save(path, true);
            // A real rebase (no active filter) repoints source_path at the
            // local file; detach from the cloud so future Saves stay local.
            // A filtered *export* leaves source_path untouched - keep the
            // cloud origin.
            if self.tabs[self.active_tab].table.source_path.as_deref() == Some(new_path.as_str()) {
                self.tabs[self.active_tab].cloud_origin = None;
            }
        }
    }

    /// Upload a freshly-saved cloud-backed tab if writes are enabled and the
    /// local write actually completed (a rounding / DB-schema prompt defers the
    /// write and leaves the tab modified; the user re-Saves after confirming).
    ///
    // ponytail: upload fires on the direct Save path. A deferred prompt skips
    // it; re-Save after confirming pushes to the cloud. Hook the writer's
    // success points only if a real cloud DB / rounding case turns up.
    fn maybe_upload_cloud(&mut self, tab_idx: usize, path: &std::path::Path) {
        if self.tabs[tab_idx].cloud_origin.is_some()
            && self.settings.cloud_writes_enabled
            && !self.tabs[tab_idx].is_modified()
        {
            self.upload_cloud_tab(tab_idx, path.to_path_buf());
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
        if self.tabs[tab_idx].cloud_origin.is_some() && !self.settings.cloud_writes_enabled {
            self.status_message = Some((
                octa::i18n::t("cloud.write_disabled"),
                std::time::Instant::now(),
            ));
            return;
        }
        if let Some(ref path) = self.tabs[tab_idx].table.source_path.clone() {
            let path = std::path::Path::new(path);
            self.do_save_tab(tab_idx, path.to_path_buf(), false);
            self.maybe_upload_cloud(tab_idx, path);
        }
    }

    pub(crate) fn do_save(&mut self, path: std::path::PathBuf, save_filtered_view: bool) {
        self.do_save_tab(self.active_tab, path, save_filtered_view);
    }

    /// `save_filtered_view`: when `true` and the active tab has a reduced
    /// row filter (text search and/or column filters), write only the
    /// visible rows to disk. The in-memory table is left untouched -
    /// `source_path` and the modified flag are not updated, so the save
    /// acts as a one-shot export of the current view.
    pub(crate) fn do_save_tab(
        &mut self,
        tab_idx: usize,
        path: std::path::PathBuf,
        save_filtered_view: bool,
    ) {
        self.do_save_tab_inner(tab_idx, path, save_filtered_view, None, None);
    }

    /// Inner save implementation. `round_decision` resolves the per-column
    /// rounding prompt: `None` = ask the user if the tab has rounding formats;
    /// `Some(true)` = write rounded values; `Some(false)` = write full
    /// precision. The prompt dialog re-enters here with `Some(_)`.
    ///
    /// `schema_decision` resolves the DB schema-change prompt: `None` = ask the
    /// user if a DB save adds/removes columns; `Some(true)` = proceed (back up
    /// and reconcile). The confirm dialog re-enters here with `Some(true)`.
    pub(crate) fn do_save_tab_inner(
        &mut self,
        tab_idx: usize,
        path: std::path::PathBuf,
        save_filtered_view: bool,
        round_decision: Option<bool>,
        schema_decision: Option<bool>,
    ) {
        // If the chat assistant changed this tab, back up the original file
        // before our save overwrites it (the user's own edits don't trigger
        // this). The flag is consumed once the backup is taken so a rounding /
        // schema re-entry of this function does not back up twice.
        // ponytail: if the user then cancels a follow-up Save dialog, the .bak
        // is already made and the flag cleared - a rare extra backup, not a
        // missed one. Restore-on-failure keeps a real backup error retryable.
        if self.settings.backup_before_modify && self.tabs[tab_idx].assistant_modified {
            match octa::formats::backup_existing_file(&path) {
                Ok(_) => self.tabs[tab_idx].assistant_modified = false,
                Err(e) => {
                    self.status_message = Some((
                        format!("Backup failed, save aborted: {e}"),
                        std::time::Instant::now(),
                    ));
                    return;
                }
            }
        }

        let tab = &mut self.tabs[tab_idx];
        if tab.raw_content_modified
            && let Some(ref content) = tab.raw_content
        {
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

        // Per-column rounding is display-only. If the user set any
        // value-rounding format, ask whether the saved file should carry the
        // rounded values or full precision - unless that decision was already
        // made (the prompt dialog re-enters with `Some(_)`).
        let has_rounding = tab
            .column_number_formats
            .values()
            .any(|f| f.rounds_values());
        if has_rounding && round_decision.is_none() {
            self.pending_round_save = Some(super::state::RoundSavePrompt {
                tab_idx,
                path,
                save_filtered_view,
            });
            return;
        }
        let apply_rounding = has_rounding && round_decision == Some(true);
        let formats = tab.column_number_formats.clone();

        // Decide once whether the writer should see a filtered snapshot of
        // the table or the live in-memory table. A filtered view is built
        // when (a) the caller asked for it (Save As), and (b) the active
        // filter actually hides some rows.
        let filtered_active = save_filtered_view && tab.filtered_rows.len() < tab.table.row_count();
        let mut filtered_table = if filtered_active {
            Some(tab.table.clone_with_rows(&tab.filtered_rows))
        } else {
            None
        };
        // When rounding is requested and we're not already writing a filtered
        // snapshot, build a rounded clone of the live table. The live table is
        // left at full precision (rounding stays display-only); only the
        // bytes on disk are rounded.
        if apply_rounding && let Some(t) = filtered_table.as_mut() {
            round_table_in_place(t, &formats);
        }
        let rounded_live = if apply_rounding && filtered_table.is_none() {
            let mut t = tab.table.clone();
            t.apply_edits();
            round_table_in_place(&mut t, &formats);
            Some(t)
        } else {
            None
        };
        let filtered_count = filtered_table.as_ref().map(|t| t.row_count()).unwrap_or(0);

        if tab.table.format_name.as_deref() == Some("CSV") && tab.csv_delimiter != b',' {
            let write_result = if let Some(ref ftab) = filtered_table {
                formats::csv_reader::write_delimited(&path, tab.csv_delimiter, ftab)
            } else {
                tab.table.apply_edits();
                let to_write = rounded_live.as_ref().unwrap_or(&tab.table);
                formats::csv_reader::write_delimited(&path, tab.csv_delimiter, to_write)
            };
            match write_result {
                Ok(()) => {
                    if filtered_table.is_none() {
                        tab.table.source_path = Some(path.to_string_lossy().to_string());
                        tab.table.clear_modified();
                    }
                    self.status_message = Some((
                        if filtered_table.is_some() {
                            format!(
                                "Exported {} filtered row{} to {} (in-memory table unchanged)",
                                filtered_count,
                                if filtered_count == 1 { "" } else { "s" },
                                path.display()
                            )
                        } else {
                            format!("Saved to {}", path.display())
                        },
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
                // DB schema-change detection (only DB tabs have db_meta).
                let schema_changed = tab
                    .table
                    .db_meta
                    .as_ref()
                    .map(|m| {
                        let cur: Vec<&str> =
                            tab.table.columns.iter().map(|c| c.name.as_str()).collect();
                        let orig: Vec<&str> =
                            m.original_columns.iter().map(|s| s.as_str()).collect();
                        cur != orig
                    })
                    .unwrap_or(false);

                if schema_changed && filtered_table.is_none() {
                    if self.settings.write_protection {
                        self.status_message = Some((
                            "This save adds or removes database columns, which is turned off. \
                             Turn off Write protection in Settings to allow it."
                                .to_string(),
                            std::time::Instant::now(),
                        ));
                        return;
                    }
                    if schema_decision.is_none() {
                        // Build the change list + backup note and defer to the modal.
                        let m = tab.table.db_meta.as_ref().unwrap();
                        let orig: std::collections::HashSet<&str> =
                            m.original_columns.iter().map(|s| s.as_str()).collect();
                        let cur: std::collections::HashSet<&str> =
                            tab.table.columns.iter().map(|c| c.name.as_str()).collect();
                        let mut changes = Vec::new();
                        for c in tab.table.columns.iter().map(|c| c.name.as_str()) {
                            if !orig.contains(c) {
                                changes.push(format!("+ add column \"{c}\""));
                            }
                        }
                        for c in m.original_columns.iter().map(|s| s.as_str()) {
                            if !cur.contains(c) {
                                changes.push(format!("- remove column \"{c}\""));
                            }
                        }
                        let backup_note = if self.settings.backup_before_modify {
                            Some(format!("{}.bak-<timestamp>", path.display()))
                        } else {
                            None
                        };
                        self.pending_schema_change_save =
                            Some(super::state::SchemaChangeSavePrompt {
                                tab_idx,
                                path,
                                save_filtered_view,
                                changes,
                                backup_note,
                            });
                        return;
                    }
                }

                // Back up ONLY for a schema-changing DB save (risky-writes
                // scope; routine Ctrl+S does not spawn a .bak).
                // ponytail: GUI schema-change detection is name-based (add/remove),
                // matching the GUI's column ops. A pure retype with no name change
                // skips the modal but the writer still reconciles it and a backup is
                // taken. Upgrade to type-aware GUI detection only if a retype-only
                // GUI path appears.
                if schema_changed
                    && filtered_table.is_none()
                    && self.settings.backup_before_modify
                    && let Err(e) = octa::formats::backup_existing_file(&path)
                {
                    self.status_message = Some((
                        format!("Backup failed, save aborted: {e}"),
                        std::time::Instant::now(),
                    ));
                    return;
                }

                let allow_schema = !self.settings.write_protection;
                let tab = &mut self.tabs[tab_idx];
                let write_result = if let Some(ref ftab) = filtered_table {
                    reader.write_file(&path, ftab)
                } else {
                    tab.table.apply_edits();
                    let to_write = rounded_live.as_ref().unwrap_or(&tab.table);
                    reader.write_file_schema_aware(&path, to_write, allow_schema)
                };
                match write_result {
                    Ok(()) => {
                        if filtered_table.is_none() {
                            tab.table.source_path = Some(path.to_string_lossy().to_string());
                            tab.table.clear_modified();
                        }
                        self.status_message = Some((
                            if filtered_table.is_some() {
                                format!(
                                    "Exported {} filtered row{} to {} (in-memory table unchanged)",
                                    filtered_count,
                                    if filtered_count == 1 { "" } else { "s" },
                                    path.display()
                                )
                            } else {
                                format!("Saved to {}", path.display())
                            },
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
