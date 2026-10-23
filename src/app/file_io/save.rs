//! Save / Save As / export, including the per-column rounding prompt, the DB
//! schema-change prompt, cloud upload, and transparent re-compression. Split
//! out of `file_io/mod.rs`.

use super::save_job::{Carry, JobWriter, PreStep, Steps};
use crate::app::state::{OctaApp, SaveMode};
use octa::formats;

/// Whether `path` names a `.csv` file.
pub(crate) fn is_csv_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
}

/// The delimiter a delimited-text target implies: a comma for `.csv` (which
/// the tab or the Save As dialog may change), a tab for `.tsv`. `None` for
/// every other format.
pub(crate) fn delimited_target(path: &std::path::Path) -> Option<u8> {
    let ext = path.extension()?.to_str()?;
    if ext.eq_ignore_ascii_case("csv") {
        Some(b',')
    } else if ext.eq_ignore_ascii_case("tsv") {
        Some(b'\t')
    } else {
        None
    }
}

/// Apply per-column value-rounding formats to a table in place. Only columns
/// whose format `rounds_values()` are touched; `Int` / `Float` cells are
/// replaced with their rounded `Float`. Used to build the on-disk snapshot
/// when the user opts to "save rounded".
pub(crate) fn round_table_in_place(
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

/// The file a save of `tab` to `path` really replaces. A decompressed tab
/// writes its temp copy and then its `.gz` original; a protected tab writes
/// its plain copy and then its encrypted zip. Any other save, a Save As
/// elsewhere included, replaces `path` itself. Backups and kept versions are
/// taken of this file, never of a temp copy that goes with the tab.
pub(crate) fn save_target(
    tab: &crate::app::state::TabState,
    path: &std::path::Path,
) -> std::path::PathBuf {
    match (&tab.compressed_origin, &tab.protected_origin) {
        (Some(origin), _) if origin.temp == path => origin.original.clone(),
        (_, Some(origin)) if origin.temp_file == path => origin.zip.clone(),
        _ => path.to_path_buf(),
    }
}

/// What a save of a tab's own file still owes the file it stands for: the
/// plain copy of a protected zip goes back into the zip, a decompressed copy
/// is compressed back onto its original. Read on the window thread, run on
/// whichever thread wrote the file. Not `Debug`: it carries the passphrase.
pub(crate) enum SaveBack {
    Zip {
        zip: std::path::PathBuf,
        entry: String,
        passphrase: String,
    },
    Compress {
        original: std::path::PathBuf,
        codec: octa::formats::compression::Codec,
    },
}

impl SaveBack {
    /// `None` unless `written` is the copy the tab stands in for, so a Save
    /// As elsewhere touches neither the zip nor the compressed original.
    pub(crate) fn of(tab: &crate::app::state::TabState, written: &std::path::Path) -> Option<Self> {
        if let Some(o) = &tab.protected_origin {
            return (o.temp_file == written).then(|| Self::Zip {
                zip: o.zip.clone(),
                entry: o.entry.clone(),
                passphrase: o.passphrase.clone(),
            });
        }
        let o = tab.compressed_origin.as_ref()?;
        (o.temp == written).then(|| Self::Compress {
            original: o.original.clone(),
            codec: o.codec,
        })
    }

    pub(crate) fn run(&self, written: &std::path::Path) -> Result<(), String> {
        match self {
            Self::Zip {
                zip,
                entry,
                passphrase,
            } => octa::formats::encrypt::replace_entry_encrypted(zip, entry, written, passphrase),
            Self::Compress { original, codec } => {
                octa::formats::compression::compress_file(written, original, *codec)
            }
        }
        .map_err(|e| format!("{e:#}"))
    }
}

/// What landing a table save needs once its write worked; see
/// [`OctaApp::finish_table_save`].
pub(crate) struct WrittenSave {
    pub(crate) path: std::path::PathBuf,
    pub(crate) mode: SaveMode,
    /// How a CSV or TSV was written, which the tab keeps for its next Save.
    pub(crate) delimited: Option<(octa::formats::write_options::CsvOptions, u8)>,
    pub(crate) filtered_copy: bool,
    pub(crate) written_rows: usize,
}

/// The version of `path` a save of `tab` is about to replace, to keep first
/// when the user turned that on ([`PreStep::KeepVersion`]); `None` when there
/// is nothing to keep.
///
/// A cloud tab's `path` is only its temp download, so it keeps nothing; for
/// a decompressed or protected tab the kept file is [`save_target`]'s.
/// Auto-save (`automatic`) keeps a file's version only the first time this
/// run, which `versioned` remembers.
fn keep_version(
    settings: &octa::ui::settings::AppSettings,
    tab: &crate::app::state::TabState,
    versioned: &mut std::collections::HashSet<std::path::PathBuf>,
    automatic: bool,
    path: &std::path::Path,
) -> Option<PreStep> {
    if !settings.keep_versions || tab.cloud_origin.is_some() {
        return None;
    }
    let target = save_target(tab, path);
    let first_this_run = versioned.insert(target.clone());
    if automatic && !first_this_run {
        return None;
    }
    Some(PreStep::KeepVersion {
        store: settings.version_store()?,
        target,
        limits: settings.version_limits(),
    })
}

/// The shared tail of [`keep_version`]: keep the file `target` is about to
/// lose when Keep earlier versions is on. Ok when it is off, there is no
/// place to keep them, or the file does not exist yet. SQL editor saves
/// call it too.
pub(crate) fn keep_earlier_version(
    settings: &octa::ui::settings::AppSettings,
    target: &std::path::Path,
) -> Result<(), String> {
    if !settings.keep_versions {
        return Ok(());
    }
    let Some(store) = settings.version_store() else {
        return Ok(());
    };
    octa::file_versions::keep(&store, target, settings.version_limits())
        .map(|_| ())
        .map_err(|e| format!("Keeping the earlier version failed, save aborted: {e:#}"))
}

impl crate::app::state::TabState {
    /// How this tab writes delimited text: the Settings defaults, then either
    /// what the Save As dialog picked (`choice`) or what the tab's own file
    /// had. `format_delimiter` is what the target implies (a tab for `.tsv`).
    /// Returns the options and the delimiter to write with. Shared by the
    /// save and by the table-to-text sync, so the text view shows exactly the
    /// bytes a save would write.
    pub(crate) fn delimited_options(
        &self,
        settings: &octa::formats::write_options::CsvOptions,
        choice: Option<crate::app::state::CsvChoice>,
        format_delimiter: u8,
    ) -> (octa::formats::write_options::CsvOptions, u8) {
        let mut csv = settings.clone();
        match choice {
            Some(choice) => {
                csv.delimiter = choice.delimiter;
                csv.decimal_comma = choice.decimal_comma;
                csv.bom = choice.bom;
            }
            None => {
                // The Settings delimiter applies only while the tab's own is
                // the default comma, as it always has. Decimal mark and byte
                // order mark are kept when either side has them.
                if self.csv_delimiter != b',' && format_delimiter != b'\t' {
                    csv.delimiter = self.csv_delimiter;
                }
                csv.decimal_comma |= self.csv_decimal_comma;
                csv.bom |= self.csv_bom;
            }
        }
        // A CSV is written with its chosen delimiter; a TSV always with tabs.
        let delimiter = if format_delimiter == b'\t' {
            b'\t'
        } else {
            csv.delimiter
        };
        (csv, delimiter)
    }
}

impl OctaApp {
    /// Save the active tab in place. Every in-place save goes through
    /// [`OctaApp::save_tab`], so the guards live in one function rather than
    /// in each caller.
    pub(crate) fn save_file(&mut self) {
        self.save_tab(self.active_tab);
    }

    /// Has the tab's source file changed on disk since this tab last read or
    /// wrote it?
    ///
    /// `false` for a tab with no recorded stamp (nothing to compare against)
    /// and for a file that is now missing: saving recreates a deleted file,
    /// which is what the user asked for, and there is no other version to
    /// lose. Only a file that exists AND differs is worth stopping for.
    pub(crate) fn source_changed_on_disk(&self, tab_idx: usize) -> bool {
        let Some(tab) = self.tabs.get(tab_idx) else {
            return false;
        };
        let (Some(known), Some(path)) = (tab.file_stamp, tab.table.source_path.as_ref()) else {
            return false;
        };
        match super::file_stamp(std::path::Path::new(path)) {
            Some(current) => current != known,
            None => false,
        }
    }

    pub(crate) fn save_file_as(&mut self) {
        self.save_tab_as(self.active_tab);
    }

    /// A save-file picker with every writable format as a filter.
    pub(crate) fn save_filters(&self) -> rfd::FileDialog {
        let mut dialog = rfd::FileDialog::new();
        for (label, exts) in self.registry.save_format_descriptions() {
            let ext_refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter(&label, &ext_refs);
        }
        dialog
    }

    /// The save picker for tab `tab_idx`, pre-filled with its file name.
    pub(crate) fn save_picker(&self, tab_idx: usize) -> rfd::FileDialog {
        let dialog = self.save_filters();
        if let Some(ref source) = self.tabs[tab_idx].table.source_path
            && let Some(name) = std::path::Path::new(source).file_name()
        {
            dialog.set_file_name(name.to_string_lossy().to_string())
        } else if let Some(name) = self.result_tab_file_name(tab_idx) {
            // Detached result tab (union, summary, pivot, ...): no source path,
            // but if it remembers its format we can pre-fill a name with the
            // matching extension so one click writes that format again.
            dialog.set_file_name(name)
        } else {
            dialog
        }
    }

    /// Save As for tab `tab_idx`, which need not be the one on screen (the
    /// close question of a background tab).
    pub(crate) fn save_tab_as(&mut self, tab_idx: usize) {
        // A table holding a window of its file gives the new file only that
        // window, with a success message: a CSV of 18 million rows saved as a
        // Parquet of its first 5 million, which then looked like a whole
        // conversion. The source survives, the copy is silently short. Ask,
        // as a Save does.
        // A large-file tab's Save As writes the whole file (`large_save`).
        let acknowledged = std::mem::take(&mut self.partial_save_acknowledged);
        if !acknowledged
            && self
                .tabs
                .get(tab_idx)
                .is_some_and(|t| t.table.is_partial() && t.large.is_none())
        {
            self.pending_partial_save_confirm = Some((tab_idx, true));
            return;
        }
        if let Some(path) = self.save_picker(tab_idx).save_file() {
            self.begin_save_as(tab_idx, path);
        }
    }

    /// The questions a Save As to `path` asks before writing: which rows
    /// (only while a search or filter hides some), whether to open the new
    /// file, and for a `.csv` how to write it. Each of the first two can be
    /// switched off in Settings, which then answers it. Nothing left to ask
    /// saves at once.
    pub(crate) fn begin_save_as(&mut self, tab_idx: usize, path: std::path::PathBuf) {
        self.begin_save_as_for(tab_idx, path, None);
    }

    /// [`Self::begin_save_as`] for File > Save to cloud/SFTP: `path` is a temp
    /// copy that `url` is uploaded from once written, so there is no new file
    /// to open and the questions name the upload.
    pub(crate) fn begin_upload(&mut self, tab_idx: usize, path: std::path::PathBuf, url: String) {
        self.begin_save_as_for(tab_idx, path, Some(url));
    }

    fn begin_save_as_for(
        &mut self,
        tab_idx: usize,
        path: std::path::PathBuf,
        upload: Option<String>,
    ) {
        let Some(tab) = self.tabs.get(tab_idx) else {
            return;
        };
        // Raw text edits are written as typed: no rows to pick, no CSV knobs.
        let text = tab.saves_text();
        // A large tab counts the file: its table is the page on screen, and
        // `total_rows` carries what its filters keep.
        let (visible, total) = match &tab.large {
            Some(large) => (
                tab.table.total_rows.unwrap_or(large.row_count()),
                large.row_count(),
            ),
            None => (tab.filtered_rows.len(), tab.table.row_count()),
        };
        let ask_rows = !text && visible < total && self.settings.save_as_ask_rows;
        let ask_open = upload.is_none() && self.settings.save_as_ask_open;
        let csv = (!text && is_csv_path(&path)).then(|| self.default_csv_choice(tab_idx));
        let filtered = !text && visible < total && self.settings.save_as_filtered_rows;
        let open = upload.is_none() && self.settings.save_as_open_new_file;
        if ask_rows || ask_open || csv.is_some() {
            self.pending_save_as = Some(crate::app::state::SaveAsPrompt {
                tab_idx,
                path,
                ask_rows,
                ask_open,
                filtered,
                open,
                visible,
                total,
                csv,
                upload,
            });
            return;
        }
        self.finish_save_as(tab_idx, path, filtered, open, None);
    }

    /// Write a Save As with its questions answered. `filtered` writes only
    /// the rows on screen; `open` leaves the user in the new file: the tab
    /// itself moves to it for a whole-table save, a new tab opens it for a
    /// filtered one. Without `open` the tab stays on its own file and the new
    /// one is just written.
    pub(crate) fn finish_save_as(
        &mut self,
        tab_idx: usize,
        path: std::path::PathBuf,
        filtered: bool,
        open: bool,
        csv: Option<crate::app::state::CsvChoice>,
    ) {
        let Some(tab) = self.tabs.get_mut(tab_idx) else {
            return;
        };
        tab.csv_save_choice = csv;
        // Writing the whole table over the tab's own file is a Save, whatever
        // the open question said: a copy would leave the tab believing it
        // still differs from its file.
        let own_file = tab.table.source_path.as_deref().map(std::path::Path::new) == Some(&path);
        let mode = if !filtered && (open || own_file) {
            SaveMode::Rebase
        } else {
            SaveMode::Copy { filtered, open }
        };
        let new_path = path.to_string_lossy().to_string();
        self.do_save_tab(tab_idx, path, mode);
        // A rebase repoints source_path at the local file; detach from the
        // cloud so future Saves stay local. A copy leaves source_path
        // untouched, so the cloud origin stays.
        if let Some(tab) = self.tabs.get_mut(tab_idx)
            && tab.table.source_path.as_deref() == Some(new_path.as_str())
        {
            tab.cloud_origin = None;
        }
    }

    /// What the "Save as CSV" choices start at: how this tab's own CSV is
    /// written, or for anything else what is usual where the user is
    /// (semicolons and decimal commas in most of Europe).
    pub(crate) fn default_csv_choice(&self, tab_idx: usize) -> crate::app::state::CsvChoice {
        let csv = &self.settings.write_options.csv;
        let tab = &self.tabs[tab_idx];
        // By the file, not `format_name`: a tab moved onto a `.csv` by an
        // earlier Save As keeps the format it was read as.
        if tab
            .table
            .source_path
            .as_deref()
            .is_some_and(|p| is_csv_path(std::path::Path::new(p)))
        {
            // Exactly what a plain Save of this tab would write.
            let (csv, delimiter) = tab.delimited_options(csv, None, b',');
            return crate::app::state::CsvChoice {
                delimiter,
                decimal_comma: csv.decimal_comma,
                bom: csv.bom,
            };
        }
        let decimal_comma = csv.decimal_comma
            || self.settings.number_separator_style
                == octa::data::num_format::SeparatorStyle::European
            || octa::i18n::uses_decimal_comma(&octa::i18n::current_language());
        crate::app::state::CsvChoice {
            delimiter: if csv.delimiter != b',' {
                csv.delimiter
            } else if decimal_comma {
                b';'
            } else {
                b','
            },
            decimal_comma,
            bom: csv.bom,
        }
    }

    /// Pre-filled Save-As file name for a tab with no source path: the tab's
    /// label plus the extension of the format it remembers. `None` when the tab
    /// has no format (nothing sensible to suggest) - the picker then opens with
    /// an empty name, as before.
    ///
    /// The extension comes from the registry itself
    /// (`reader_by_name` + `FormatReader::extensions`), so a new format needs no
    /// change here.
    pub(crate) fn result_tab_file_name(&self, tab_idx: usize) -> Option<String> {
        let tab = self.tabs.get(tab_idx)?;
        let fmt = tab.table.format_name.as_deref()?;
        let ext = *self.registry.reader_by_name(fmt)?.extensions().first()?;
        let label = tab.title_display();
        // Drop the trailing " *" unsaved marker before it becomes part of a name.
        let stem: String = label
            .trim_end_matches('*')
            .trim_end()
            .chars()
            .map(|c| if r#"/\:*?"<>|"#.contains(c) { '_' } else { c })
            .collect();
        let stem = stem.trim().to_string();
        let stem = if stem.is_empty() {
            "untitled".to_string()
        } else {
            stem
        };
        Some(format!("{stem}.{ext}"))
    }

    /// Upload a freshly-saved cloud-backed tab if writes are enabled and the
    /// local write actually completed (a rounding / DB-schema prompt defers the
    /// write and leaves the tab modified; the user re-Saves after confirming).
    ///
    // ponytail: upload fires on the direct Save path. A deferred prompt skips
    // it; re-Save after confirming pushes to the cloud. Hook the writer's
    // success points only if a real cloud DB / rounding case turns up.
    pub(super) fn maybe_upload_cloud(&mut self, tab_idx: usize, path: &std::path::Path) {
        if self.tabs[tab_idx].cloud_origin.is_some()
            && self.cloud_tab_writable(tab_idx)
            && !self.tabs[tab_idx].is_modified()
        {
            self.upload_cloud_tab(tab_idx, path.to_path_buf());
        }
    }

    /// Whether saving this tab may write back to its cloud origin. True for
    /// non-cloud tabs; a cloud tab needs BOTH the global "Allow writing to
    /// cloud storage" switch and the connection's own per-connection write
    /// permission (a deleted connection blocks).
    pub(crate) fn cloud_tab_writable(&self, tab_idx: usize) -> bool {
        let Some(origin) = self.tabs[tab_idx].cloud_origin.as_ref() else {
            return true;
        };
        self.settings
            .cloud_connections
            .iter()
            .any(|c| c.id == origin.conn_id && c.allow_writes)
    }

    pub(crate) fn export_sql_result(&mut self) {
        // A paged result holds only the pages scrolled to. Exporting that
        // would write the page and call it the answer, so pull the whole
        // materialised result back out of DuckDB first.
        let tab = &self.tabs[self.active_tab];
        let partial = match (tab.sql.result_total, tab.sql.result.as_ref()) {
            (Some(total), Some(loaded)) => loaded.row_count() < total,
            _ => false,
        };
        let result = if partial {
            match tab
                .sql_workspace
                .as_ref()
                .map(|ws| ws.result_all(tab.sql.id))
            {
                Some(Ok(all)) => all,
                Some(Err(e)) => {
                    self.status_message =
                        Some((format!("Error exporting: {e:#}"), std::time::Instant::now()));
                    return;
                }
                None => return,
            }
        } else {
            let Some(result) = tab.sql.result.clone() else {
                return;
            };
            result
        };
        if result.col_count() == 0 {
            return;
        }

        let dialog = self.save_filters().set_file_name("sql_result.csv");

        let Some(path) = dialog.save_file() else {
            return;
        };

        match self.registry.reader_for_path(&path) {
            Some(reader) if reader.supports_write() => {
                match reader.write_file_with_options(&path, &result, &self.settings.write_options) {
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
                }
            }
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

    /// Save one tab back over its own file. The single in-place save path:
    /// Ctrl+S, the close prompt, and auto-save all arrive here.
    pub(crate) fn save_tab(&mut self, tab_idx: usize) {
        // Consumed on every attempt, whichever branch it leaves by: a "Save
        // anyway" that then runs into a different prompt must not leave the
        // acknowledgement lying around for an unrelated later save.
        let partial_acknowledged = std::mem::take(&mut self.partial_save_acknowledged);
        // A live-database tab has no source file: Save means "write the diff
        // back to the server", confirmed via the write-back dialog.
        if self.tabs[tab_idx].db_origin.is_some() {
            self.begin_db_write_back(tab_idx);
            return;
        }
        // A large-file tab is read-only, so there is nothing to save, and its
        // table is the page on screen: writing it back would replace the file
        // with 2,000 of its rows.
        if self.tabs[tab_idx].large.is_some() {
            self.status_message = Some((
                octa::i18n::t("largefile.save_in_place"),
                std::time::Instant::now(),
            ));
            return;
        }
        // Cloud-opened tab: block when writes are disabled globally or for
        // this connection (the user can still Save As to a local copy),
        // otherwise upload after the local write.
        if !self.cloud_tab_writable(tab_idx) {
            self.status_message = Some((
                octa::i18n::t("cloud.write_disabled"),
                std::time::Instant::now(),
            ));
            return;
        }
        if let Some(ref path) = self.tabs[tab_idx].table.source_path.clone() {
            let path = std::path::Path::new(path);
            // Something else rewrote the file while this tab held it. Saving
            // now would replace that with a snapshot taken before it existed,
            // silently. Ask instead. Save As is not guarded: the user picked
            // that path in a file dialog moments ago, and the picker asks
            // about overwriting itself.
            if self.source_changed_on_disk(tab_idx) {
                self.pending_overwrite_confirm = Some(tab_idx);
                return;
            }
            // The table holds a window of the file. Writing it back would
            // drop every row that was never read, over the original, with a
            // success message. Ask, and offer to read the rest first.
            if self.tabs[tab_idx].table.is_partial() && !partial_acknowledged {
                self.pending_partial_save_confirm = Some((tab_idx, false));
                return;
            }
            // Regular save writes the full table back to the source path,
            // never the filtered view - the file on disk represents the
            // user's data, not their current view.
            let busy = self.tabs[tab_idx].saving.is_some();
            self.do_save_tab(tab_idx, path.to_path_buf(), SaveMode::Rebase);
            match self.tabs[tab_idx].saving.as_mut() {
                // Written in the background: upload once it lands.
                Some(job) if !busy => job.upload = true,
                // Refused: another save of this tab is still writing.
                Some(_) => {}
                None => self.maybe_upload_cloud(tab_idx, path),
            }
        }
    }

    /// Write tab `tab_idx` to `path`. `mode` says whether the tab moves to the
    /// written file ([`SaveMode::Rebase`]) or a copy is written and the tab
    /// stays where it is ([`SaveMode::Copy`]); see there.
    pub(crate) fn do_save_tab(&mut self, tab_idx: usize, path: std::path::PathBuf, mode: SaveMode) {
        self.do_save_tab_inner(tab_idx, path, mode, None, None, None);
    }

    /// Re-entry point for the "include formatting?" prompt (`.xlsx` saves
    /// only). `round_decision` is threaded through from whatever was already
    /// decided earlier in the same save (carried on
    /// `crate::app::state::XlsxStylePrompt::round_decision`), so answering
    /// this prompt cannot reopen the rounding prompt.
    pub(crate) fn do_save_tab_with_style(
        &mut self,
        tab_idx: usize,
        path: std::path::PathBuf,
        mode: SaveMode,
        round_decision: Option<bool>,
        style_decision: Option<bool>,
    ) {
        self.do_save_tab_inner(tab_idx, path, mode, round_decision, None, style_decision);
    }

    /// Inner save implementation. `round_decision` resolves the per-column
    /// rounding prompt: `None` = ask the user if the tab has rounding formats;
    /// `Some(true)` = write rounded values; `Some(false)` = write full
    /// precision. The prompt dialog re-enters here with `Some(_)`.
    ///
    /// `schema_decision` resolves the DB schema-change prompt: `None` = ask the
    /// user if a DB save adds/removes columns; `Some(true)` = proceed (back up
    /// and reconcile). The confirm dialog re-enters here with `Some(true)`.
    ///
    /// `style_decision` resolves the `.xlsx` "include formatting?" prompt:
    /// `None` = ask the user if the target is `.xlsx` and the tab carries
    /// marks, conditional colours, frozen columns or number formats;
    /// `Some(true)`/`Some(false)` = carry the formatting or write plain data.
    /// The three prompts are independent and can all fire for one save (round
    /// first, then formatting, then schema); each deferred prompt carries
    /// forward whatever the earlier ones already decided so resuming one never
    /// reopens another.
    pub(crate) fn do_save_tab_inner(
        &mut self,
        tab_idx: usize,
        path: std::path::PathBuf,
        mode: SaveMode,
        round_decision: Option<bool>,
        schema_decision: Option<bool>,
        style_decision: Option<bool>,
    ) {
        // One write per tab at a time, and never two into one file: they would
        // share a temp name.
        if self.tabs[tab_idx].saving.is_some()
            || self.tabs[tab_idx].large_export.is_some()
            || self.tabs.iter().any(|t| {
                t.saving.as_ref().is_some_and(|j| j.save.path == path)
                    || t.large_export.as_ref().is_some_and(|j| j.path == path)
            })
        {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            self.status_message = Some((
                octa::i18n::t("saving.busy").replace("{name}", &name),
                std::time::Instant::now(),
            ));
            self.forget_upload(&path);
            return;
        }
        if self.tabs[tab_idx].large.is_some() {
            self.start_large_export(tab_idx, path, mode);
            return;
        }
        // Whatever this write does to the file, the sidebar's git marks for
        // its repository are stale after it. Noted now, refreshed next tick.
        self.git_marks.note_saved(&path);

        // Answered before the `&mut` borrow of the tab below, since the shared
        // predicate takes `&self`.
        let ask_about_style =
            style_decision.is_none() && self.save_would_ask_about_style(tab_idx, &path);

        // If the chat assistant changed this tab, the file it is about to
        // replace is backed up first (the user's own edits don't trigger
        // this). Taken when the write happens, after every question, and the
        // flag is cleared once the save worked.
        let assistant_backup =
            self.settings.backup_before_modify && self.tabs[tab_idx].assistant_modified;

        if self.tabs[tab_idx].saves_text() && self.tabs[tab_idx].raw_content.is_some() {
            let mut before = Vec::new();
            if assistant_backup {
                before.push(PreStep::Backup(save_target(&self.tabs[tab_idx], &path)));
            }
            before.extend(keep_version(
                &self.settings,
                &self.tabs[tab_idx],
                &mut self.versioned_paths,
                self.saving_automatically,
                &path,
            ));
            for step in &before {
                if let Err(msg) = step.run() {
                    self.status_message = Some((msg, std::time::Instant::now()));
                    return;
                }
            }
            if assistant_backup {
                self.tabs[tab_idx].assistant_modified = false;
            }
            let tab = &mut self.tabs[tab_idx];
            let content = tab.raw_content.as_deref().unwrap_or_default();
            match std::fs::write(&path, content) {
                Ok(()) if mode == SaveMode::Rebase => {
                    tab.table.source_path = Some(path.to_string_lossy().to_string());
                    // The table catches up with the text it was saved from.
                    // Text it cannot read leaves the table stale: drop its
                    // edits, or a later save of it would overwrite this one.
                    if let Err(msg) = tab.table_from_text(&self.registry) {
                        tab.table.discard_edits();
                        tab.table.clear_modified();
                        tab.parse_error_banner = Some(msg);
                    }
                    tab.raw_content_modified = false;
                    self.status_message = Some((
                        format!("Saved to {}", path.display()),
                        std::time::Instant::now(),
                    ));
                    let back = SaveBack::of(&self.tabs[tab_idx], &path).map(|b| b.run(&path));
                    self.after_tab_saved(tab_idx, &path, true, back);
                }
                Ok(()) => {
                    self.status_message = Some((
                        format!("Saved a copy to {}", path.display()),
                        std::time::Instant::now(),
                    ));
                    self.upload_saved_copy(&path);
                    if let SaveMode::Copy { open: true, .. } = mode {
                        self.load_file_in_new_tab(path);
                    }
                }
                Err(e) => {
                    self.forget_upload(&path);
                    self.status_message = Some((
                        format!("Error saving file: {}", e),
                        std::time::Instant::now(),
                    ));
                }
            }
            return;
        }
        let tab = &mut self.tabs[tab_idx];

        // Per-column rounding is display-only. If the user set any
        // value-rounding format, ask whether the saved file should carry the
        // rounded values or full precision - unless that decision was already
        // made (the prompt dialog re-enters with `Some(_)`).
        let has_rounding = tab
            .column_number_formats
            .values()
            .any(|f| f.rounds_values());
        if has_rounding && round_decision.is_none() {
            self.pending_round_save = Some(crate::app::state::RoundSavePrompt {
                tab_idx,
                path,
                mode,
            });
            return;
        }
        let apply_rounding = has_rounding && round_decision == Some(true);
        let formats = tab.column_number_formats.clone();

        // Formatting is display-only until a writer is asked to carry it.
        // Only `.xlsx` can hold it, so only that extension raises the prompt.
        if ask_about_style {
            self.pending_xlsx_style_save = Some(crate::app::state::XlsxStylePrompt {
                tab_idx,
                path,
                mode,
                round_decision,
            });
            return;
        }

        // A copy is written from a snapshot (the rows on screen, or all of
        // them), so the tab itself is not touched. A rebase writes the live
        // table and moves the tab onto the written file.
        let snapshot_rows: Option<Vec<usize>> = match mode {
            SaveMode::Rebase => None,
            SaveMode::Copy { filtered: true, .. }
                if tab.filtered_rows.len() < tab.table.row_count() =>
            {
                Some(tab.filtered_rows.clone())
            }
            SaveMode::Copy { .. } => Some((0..tab.table.row_count()).collect()),
        };
        let filtered_copy = snapshot_rows
            .as_ref()
            .is_some_and(|rows| rows.len() < tab.table.row_count());
        // A rebase of a live-database tab moves it onto the target file
        // (plain Save was intercepted into the server write-back flow). Strip
        // the server row identity so the SQLite/DuckDB file writers do not try
        // to diff-save against the brand-new file, and detach the tab from the
        // server. A copy writes a clone (db_meta already dropped by
        // clone_with_rows) and keeps the tab live.
        if mode == SaveMode::Rebase && tab.db_origin.is_some() {
            tab.db_origin = None;
            tab.table.db_meta = None;
        }
        let written_rows = snapshot_rows.as_ref().map_or(0, Vec::len);

        // A `.csv` or `.tsv` target is written here rather than by the
        // registry's writer, because how it is written depends on the tab:
        // the delimiter, decimal mark and byte order mark its file had, or
        // what the Save As dialog picked. Keyed on the TARGET's extension: a
        // `;` CSV saved as Parquet is written as Parquet.
        let delimited = delimited_target(&path).map(|format_delimiter| {
            let choice = tab.csv_save_choice.take();
            tab.delimited_options(&self.settings.write_options.csv, choice, format_delimiter)
        });

        let Some(reader) = self.registry.reader_for_path(&path) else {
            self.status_message = Some((
                format!(
                    "No writer available for extension: {}",
                    path.extension()
                        .map(|e| e.to_string_lossy().to_string())
                        .unwrap_or_default()
                ),
                std::time::Instant::now(),
            ));
            return;
        };
        if !reader.supports_write() {
            self.status_message = Some((
                format!("Writing is not supported for {} format", reader.name()),
                std::time::Instant::now(),
            ));
            return;
        }
        // `style_decision` overrides the Settings default only when the prompt
        // actually asked (an `.xlsx` target with something to carry);
        // otherwise the Settings default drives, same as every other write
        // option.
        let mut opts = self.settings.write_options.clone();
        let include = style_decision.unwrap_or(opts.xlsx.include_formatting);
        opts.xlsx.include_formatting = include;
        // The style travels either way: column widths and validation
        // rules are carried on every save, and `TableStyle::formatting`
        // tells the writer whether the decoration goes with them.
        opts.style = self.tab_table_style(tab_idx).map(|mut s| {
            s.formatting = include;
            s
        });
        opts.notebook = self.tabs[tab_idx].notebook_text();

        let tab = &mut self.tabs[tab_idx];
        // DB schema-change detection (only DB tabs have db_meta).
        let schema_changed = tab
            .table
            .db_meta
            .as_ref()
            .map(|m| {
                let cur: Vec<&str> = tab.table.columns.iter().map(|c| c.name.as_str()).collect();
                let orig: Vec<&str> = m.original_columns.iter().map(|s| s.as_str()).collect();
                cur != orig
            })
            .unwrap_or(false);

        if schema_changed && mode == SaveMode::Rebase {
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
                    Some(format!(
                        "{}.bak-<timestamp>",
                        save_target(&self.tabs[tab_idx], &path).display()
                    ))
                } else {
                    None
                };
                self.pending_schema_change_save = Some(crate::app::state::SchemaChangeSavePrompt {
                    tab_idx,
                    path,
                    mode,
                    changes,
                    backup_note,
                    round_decision,
                    style_decision,
                });
                return;
            }
        }

        // The copies taken of the file before it is replaced: the
        // assistant's backup, a backup for a schema-changing DB save
        // (risky-writes scope; routine Ctrl+S does not spawn a .bak), and the
        // earlier version.
        // ponytail: GUI schema-change detection is name-based (add/remove),
        // matching the GUI's column ops. A pure retype with no name change
        // skips the modal but the writer still reconciles it and a backup is
        // taken. Upgrade to type-aware GUI detection only if a retype-only
        // GUI path appears.
        let mut before = Vec::new();
        if assistant_backup {
            before.push(PreStep::Backup(save_target(&self.tabs[tab_idx], &path)));
        }
        if schema_changed && mode == SaveMode::Rebase && self.settings.backup_before_modify {
            before.push(PreStep::Backup(save_target(&self.tabs[tab_idx], &path)));
        }
        before.extend(keep_version(
            &self.settings,
            &self.tabs[tab_idx],
            &mut self.versioned_paths,
            self.saving_automatically,
            &path,
        ));

        // A big save is written by a worker fed a slice at a time
        // (`save_job`): CSV, TSV and Parquet slice by slice, any other format
        // put back together by the worker. Only the diff-based database
        // writers, which need the live table's row identity, write while the
        // window waits.
        let tab = &self.tabs[tab_idx];
        let rows_to_write = snapshot_rows
            .as_ref()
            .map_or(tab.table.row_count(), Vec::len);
        let slice_rows = crate::app::slice_feed::slice_rows(tab.table.col_count());
        let reader_name = reader.name().to_string();
        let writer = if rows_to_write <= slice_rows {
            None
        } else if let Some((opts, delimiter)) = &delimited {
            Some(JobWriter::Slices(formats::SliceFormat::Delimited {
                opts: opts.clone(),
                delimiter: *delimiter,
            }))
        } else if reader_name == "Parquet" {
            Some(JobWriter::Slices(formats::SliceFormat::Parquet(
                opts.parquet.clone(),
            )))
        } else if tab.table.db_meta.is_none() && !reader.writes_in_place() {
            Some(JobWriter::Whole {
                reader: reader_name,
                opts: Box::new(opts.clone()),
                carry: match mode {
                    SaveMode::Rebase => Carry::of(&tab.table),
                    SaveMode::Copy { .. } => Carry::default(),
                },
            })
        } else {
            None
        };
        if let Some(writer) = writer {
            let tab = &mut self.tabs[tab_idx];
            let back = SaveBack::of(tab, &path).filter(|_| mode == SaveMode::Rebase);
            let save = WrittenSave {
                path,
                mode,
                delimited,
                filtered_copy,
                written_rows,
            };
            let mut job = super::save_job::SaveJob::start(
                &tab.table,
                snapshot_rows,
                writer,
                apply_rounding.then_some(formats),
                Steps { before, back },
                save,
                slice_rows,
            );
            job.assistant_backup = assistant_backup;
            tab.saving = Some(job);
            return;
        }

        for step in &before {
            if let Err(msg) = step.run() {
                self.status_message = Some((msg, std::time::Instant::now()));
                return;
            }
        }
        if assistant_backup {
            self.tabs[tab_idx].assistant_modified = false;
        }
        let allow_schema = !self.settings.write_protection;
        let tab = &mut self.tabs[tab_idx];
        let mut snapshot = snapshot_rows
            .as_ref()
            .map(|rows| tab.table.clone_with_rows(rows));
        // When rounding is requested, the bytes on disk are rounded; the live
        // table is left at full precision (rounding stays display-only).
        if apply_rounding && let Some(t) = snapshot.as_mut() {
            round_table_in_place(t, &formats);
        }
        let rounded_live = if apply_rounding && mode == SaveMode::Rebase {
            let mut t = tab.table.clone();
            t.apply_edits();
            round_table_in_place(&mut t, &formats);
            Some(t)
        } else {
            None
        };
        // The Settings write-option defaults apply to every GUI save,
        // except what the Save As dialog picked for a `.csv` (above).
        // `opts` (built above) additionally carries the resolved
        // `.xlsx` formatting choice for this save.
        let mut pending_edits = None;
        let write_result = match (&snapshot, &delimited) {
            (Some(t), Some((csv, delim))) => {
                formats::csv_reader::write_delimited_with(&path, t, csv, *delim).map(|()| None)
            }
            (Some(t), None) => reader
                .write_file_with_options(&path, t, &opts)
                .map(|()| None),
            (None, _) => {
                pending_edits = Some(tab.table.apply_edits_recoverable());
                let to_write = rounded_live.as_ref().unwrap_or(&tab.table);
                match &delimited {
                    Some((csv, delim)) => {
                        formats::csv_reader::write_delimited_with(&path, to_write, csv, *delim)
                            .map(|()| None)
                    }
                    None => reader.write_file_retagged(&path, to_write, allow_schema, &opts),
                }
            }
        };
        match write_result {
            Ok(new_tags) => {
                let back = SaveBack::of(&self.tabs[tab_idx], &path)
                    .filter(|_| mode == SaveMode::Rebase)
                    .map(|b| b.run(&path));
                let save = WrittenSave {
                    path,
                    mode,
                    delimited,
                    filtered_copy,
                    written_rows,
                };
                self.finish_table_save(tab_idx, save, new_tags, back);
            }
            Err(e) => {
                // Nothing reached the disk, so the tab must stay
                // unsaved: otherwise the dirty marker clears, closing
                // it asks nothing and the edits are lost.
                if let Some(edits) = pending_edits {
                    self.tabs[tab_idx].table.edits = edits;
                }
                self.forget_upload(&path);
                self.status_message = Some((
                    format!("Error saving file: {e:#}"),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    /// Land a table save whose write worked, written at once or by a
    /// [`save_job`](super::save_job): a rebase moves the tab onto the file and
    /// marks it saved, then the status line says what was written. `back` is
    /// what the save owed a zip or a compressed original, already done.
    pub(crate) fn finish_table_save(
        &mut self,
        tab_idx: usize,
        save: WrittenSave,
        new_tags: Option<Vec<Option<i64>>>,
        back: Option<Result<(), String>>,
    ) {
        let WrittenSave {
            path,
            mode,
            delimited,
            filtered_copy,
            written_rows,
        } = save;
        if mode == SaveMode::Rebase {
            let tab = &mut self.tabs[tab_idx];
            // A background save wrote the edits without merging them first.
            tab.table.apply_edits();
            tab.table.source_path = Some(path.to_string_lossy().to_string());
            tab.table.clear_modified();
            tab.raw_content_modified = false;
            tab.file_stamp = super::file_stamp(&path);
            // A diff-based writer (SQLite / DuckDB) reports the identity the
            // saved rows now have. Without it the rows added this session stay
            // untagged and the next save INSERTs them again.
            if let Some(tags) = new_tags {
                tab.table.retag_db_rows(tags);
            }
            // The tab's file is now this one, written this way, so the next
            // Save writes it the same way again.
            if let Some((csv, delim)) = &delimited {
                tab.csv_delimiter = *delim;
                tab.csv_decimal_comma = csv.decimal_comma;
                tab.csv_bom = csv.bom;
            }
        }
        self.status_message = Some((
            if filtered_copy {
                format!(
                    "Exported {} filtered row{} to {} (in-memory table unchanged)",
                    written_rows,
                    if written_rows == 1 { "" } else { "s" },
                    path.display()
                )
            } else if mode == SaveMode::Rebase {
                format!("Saved to {}", path.display())
            } else {
                format!("Saved a copy to {}", path.display())
            },
            std::time::Instant::now(),
        ));
        self.upload_saved_copy(&path);
        match mode {
            SaveMode::Rebase => self.after_tab_saved(tab_idx, &path, false, back),
            SaveMode::Copy { open: true, .. } => self.load_file_in_new_tab(path),
            SaveMode::Copy { open: false, .. } => {}
        }
    }

    /// Whether saving tab `tab_idx` to `path` would raise the "include
    /// formatting?" prompt: an `.xlsx` target and a tab that actually carries
    /// something only `.xlsx` can hold.
    ///
    /// Shared with `auto_save::tab_is_auto_saveable`, which must skip a tab
    /// whose save would ask a question. Auto-save is documented never to
    /// interrupt, so this predicate has to have exactly one definition.
    pub(crate) fn save_would_ask_about_style(
        &self,
        tab_idx: usize,
        path: &std::path::Path,
    ) -> bool {
        let Some(tab) = self.tabs.get(tab_idx) else {
            return false;
        };
        let is_xlsx = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("xlsx"));
        is_xlsx
            && (!tab.conditional_format_rules.is_empty()
                || !tab.column_number_formats.is_empty()
                || tab.table_state.frozen_cols > 0
                || !tab.table.marks.is_empty())
    }

    /// Snapshot the active presentation of a tab for a writer that can carry
    /// it. Manual marks are not included: they already travel on `DataTable`.
    pub(crate) fn tab_table_style(
        &self,
        tab_idx: usize,
    ) -> Option<octa::formats::write_options::TableStyle> {
        let tab = self.tabs.get(tab_idx)?;
        Some(octa::formats::write_options::TableStyle {
            conditional: tab.conditional_format_rules.clone(),
            number_formats: tab.column_number_formats.clone(),
            frozen_cols: tab.table_state.frozen_cols,
            col_widths: tab.table_state.col_widths.clone(),
            validation: tab.validation_rules.clone(),
            // The caller decides; the snapshot itself carries no opinion.
            formatting: false,
        })
    }

    /// After a successful save of the tab's own file: refresh the raw text,
    /// then report on `back`, the caller's putting back of a decompressed or
    /// protected tab's copy ([`SaveBack`]; `None` when the save landed
    /// anywhere but that copy, so a Save As elsewhere touches neither the
    /// compressed original nor the zip). A protected tab saved elsewhere is
    /// detached from its zip. `wrote_text` says this save wrote the tab's raw
    /// text rather than its table.
    fn after_tab_saved(
        &mut self,
        tab_idx: usize,
        written: &std::path::Path,
        wrote_text: bool,
        back: Option<Result<(), String>>,
    ) {
        // Raw view shows the file's text, read once at load: re-read it, or
        // switching to Raw after a save shows the file as it was before.
        let tab = &mut self.tabs[tab_idx];
        if tab.raw_content.is_some() && !tab.raw_content_modified {
            tab.raw_content = std::fs::read_to_string(written).ok();
            tab.raw_content_original = tab.raw_content.clone();
        }
        if let Some(origin) = self.tabs[tab_idx].protected_origin.clone() {
            // Only a Rebase reaches here, so the tab now belongs to `written`.
            if back.is_none() {
                // Save As to a new file: that file is plain and the tab now
                // belongs to it, so it no longer writes into the zip, nor
                // wears its name.
                let tab = &mut self.tabs[tab_idx];
                tab.protected_origin = None;
                tab.custom_tab_label = None;
            } else if let Some(Err(e)) = back {
                // The edits now live only in the plain copy, which goes with
                // the tab. Keep the tab unsaved, so closing it or Octa asks
                // first and the next Save tries the zip again, the same way:
                // text stays text, and a table is not re-parsed from it.
                let tab = &mut self.tabs[tab_idx];
                if wrote_text {
                    tab.raw_content_modified = true;
                } else {
                    // ponytail: `structural_changes` is the only "unsaved"
                    // flag a table has, and it also makes
                    // `DataTable::formula` withhold formulas, so the retried
                    // save of a protected xlsx writes values. Fix: a separate
                    // write-back-pending flag on the tab, counted in
                    // `is_modified`.
                    tab.table.structural_changes = true;
                }
                self.status_message = Some((
                    octa::i18n::t("passphrase.save_back_failed")
                        .replace("{zip}", &origin.zip.display().to_string())
                        .replace("{error}", &e),
                    std::time::Instant::now(),
                ));
            } else {
                // The save named the plain copy; what the user saved is the zip.
                self.status_message = Some((
                    format!("Saved to {}", origin.zip.display()),
                    std::time::Instant::now(),
                ));
            }
            return;
        }
        if let (Some(origin), Some(Err(e))) = (&self.tabs[tab_idx].compressed_origin, back) {
            self.status_message = Some((
                format!(
                    "Saved, but re-compressing to {} failed: {e}",
                    origin.original.display()
                ),
                std::time::Instant::now(),
            ));
        }
    }
}

#[cfg(test)]
#[path = "save_tests.rs"]
mod tests;
