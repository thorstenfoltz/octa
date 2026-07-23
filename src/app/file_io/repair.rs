//! Post-read recovery: content-sniff reload for wrong extensions, the opt-in
//! malformed-CSV repair prompt, and the raw-text fallback after a parse
//! failure. Split out of `file_io/mod.rs`.

use crate::app::state::OctaApp;
use octa::data::{DataTable, ViewMode};
use octa::formats::{self, FormatReader};

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

impl OctaApp {
    /// After an extension-chosen reader fails, attempt to reload using the
    /// reader identified by [`formats::sniff::sniff_format`]. Handles the
    /// wrong-extension case (a binary format with a misleading extension).
    /// Returns `true` when a sniffed reader successfully loaded the file.
    ///
    /// Multi-table sources (databases, Excel) are skipped here: they keep
    /// conventional extensions, and the missing-extension case is already
    /// handled in `reader_for_path` (which routes them through the normal
    /// table-picker dispatch).
    pub(crate) fn try_content_sniff_reload(
        &mut self,
        path: &std::path::Path,
        failed_format: &str,
    ) -> bool {
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
    pub(crate) fn maybe_offer_repair(&mut self, path: &std::path::Path, reader_name: &str) -> bool {
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
        self.pending_file_repair = Some(crate::app::state::FileRepair {
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
    pub(crate) fn fallback_to_raw_text(
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
}
