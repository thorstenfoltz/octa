//! Save / Save As / export, including the per-column rounding prompt, the DB
//! schema-change prompt, cloud upload, and transparent re-compression. Split
//! out of `file_io/mod.rs`.

use crate::app::state::OctaApp;
use octa::formats;

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

impl OctaApp {
    pub(crate) fn save_file(&mut self) {
        // A live-database tab has no source file: Save means "write the diff
        // back to the server", confirmed via the write-back dialog.
        if self.tabs[self.active_tab].db_origin.is_some() {
            self.begin_db_write_back(self.active_tab);
            return;
        }
        // Cloud-opened tab: block when writes are disabled globally or for
        // this connection (the user can still Save As to a local copy),
        // otherwise upload after the local write.
        if !self.cloud_tab_writable(self.active_tab) {
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
        } else if let Some(name) = self.result_tab_file_name(self.active_tab) {
            // Detached result tab (union, summary, pivot, ...): no source path,
            // but if it remembers its format we can pre-fill a name with the
            // matching extension so one click writes that format again.
            dialog = dialog.set_file_name(name);
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

    /// Pre-filled Save-As file name for a tab with no source path: the tab's
    /// label plus the extension of the format it remembers. `None` when the tab
    /// has no format (nothing sensible to suggest) - the picker then opens with
    /// an empty name, as before.
    ///
    /// The extension comes from the registry itself
    /// (`reader_by_name` + `FormatReader::extensions`), so a new format needs no
    /// change here.
    fn result_tab_file_name(&self, tab_idx: usize) -> Option<String> {
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
    fn maybe_upload_cloud(&mut self, tab_idx: usize, path: &std::path::Path) {
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

    pub(crate) fn save_tab(&mut self, tab_idx: usize) {
        if self.tabs[tab_idx].db_origin.is_some() {
            self.begin_db_write_back(tab_idx);
            return;
        }
        if !self.cloud_tab_writable(tab_idx) {
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
        self.do_save_tab_inner(tab_idx, path, save_filtered_view, None, None, None);
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
        save_filtered_view: bool,
        round_decision: Option<bool>,
        style_decision: Option<bool>,
    ) {
        self.do_save_tab_inner(
            tab_idx,
            path,
            save_filtered_view,
            round_decision,
            None,
            style_decision,
        );
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
        save_filtered_view: bool,
        round_decision: Option<bool>,
        schema_decision: Option<bool>,
        style_decision: Option<bool>,
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

        // Answered before the `&mut` borrow of the tab below, since the shared
        // predicate takes `&self`.
        let ask_about_style =
            style_decision.is_none() && self.save_would_ask_about_style(tab_idx, &path);

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
                    self.maybe_recompress_saved(tab_idx, &path);
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
            self.pending_round_save = Some(crate::app::state::RoundSavePrompt {
                tab_idx,
                path,
                save_filtered_view,
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
                save_filtered_view,
                round_decision,
            });
            return;
        }

        // Decide once whether the writer should see a filtered snapshot of
        // the table or the live in-memory table. A filtered view is built
        // when (a) the caller asked for it (Save As), and (b) the active
        // filter actually hides some rows.
        let filtered_active = save_filtered_view && tab.filtered_rows.len() < tab.table.row_count();
        // A full Save As of a live-database tab rebases it onto the target
        // file (plain Save was intercepted into the server write-back flow).
        // Strip the server row identity so the SQLite/DuckDB file writers do
        // not try to diff-save against the brand-new file, and detach the tab
        // from the server. A filtered export writes a clone (db_meta already
        // dropped by clone_with_rows) and keeps the tab live.
        if !filtered_active && tab.db_origin.is_some() {
            tab.db_origin = None;
            tab.table.db_meta = None;
        }
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
            let mut pending_edits = None;
            let write_result = if let Some(ref ftab) = filtered_table {
                formats::csv_reader::write_delimited(&path, tab.csv_delimiter, ftab)
            } else {
                pending_edits = Some(tab.table.apply_edits_recoverable());
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
                    if filtered_table.is_none() {
                        self.maybe_recompress_saved(tab_idx, &path);
                    }
                }
                Err(e) => {
                    // Nothing reached the disk, so the tab must stay unsaved.
                    if let Some(edits) = pending_edits {
                        self.tabs[tab_idx].table.edits = edits;
                    }
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
                // Resolve the write options before taking a mutable borrow of
                // the tab below: `tab_table_style` needs `&self`. `style_decision`
                // overrides the Settings default only when the prompt actually
                // asked (an `.xlsx` target with something to carry); otherwise
                // the Settings default drives, same as every other write option.
                let mut opts = self.settings.write_options.clone();
                let include = style_decision.unwrap_or(opts.xlsx.include_formatting);
                opts.xlsx.include_formatting = include;
                opts.style = include.then(|| self.tab_table_style(tab_idx)).flatten();

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
                            Some(crate::app::state::SchemaChangeSavePrompt {
                                tab_idx,
                                path,
                                save_filtered_view,
                                changes,
                                backup_note,
                                round_decision,
                                style_decision,
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
                // The Settings write-option defaults apply to every GUI save;
                // the per-run override lives in the batch convert dialog, since
                // Save As is the OS file picker and has nowhere to host one.
                // `opts` (built above) additionally carries the resolved
                // `.xlsx` formatting choice for this save.
                let mut pending_edits = None;
                let write_result = if let Some(ref ftab) = filtered_table {
                    reader
                        .write_file_with_options(&path, ftab, &opts)
                        .map(|()| None)
                } else {
                    pending_edits = Some(tab.table.apply_edits_recoverable());
                    let to_write = rounded_live.as_ref().unwrap_or(&tab.table);
                    reader.write_file_retagged(&path, to_write, allow_schema, &opts)
                };
                match write_result {
                    Ok(new_tags) => {
                        if filtered_table.is_none() {
                            tab.table.source_path = Some(path.to_string_lossy().to_string());
                            tab.table.clear_modified();
                            // A diff-based writer (SQLite / DuckDB) reports the
                            // identity the saved rows now have. Without it the
                            // rows added this session stay untagged and the
                            // next save INSERTs them again.
                            if let Some(tags) = new_tags {
                                tab.table.retag_db_rows(tags);
                            }
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
                        if filtered_table.is_none() {
                            self.maybe_recompress_saved(tab_idx, &path);
                        }
                    }
                    Err(e) => {
                        // Nothing reached the disk, so the tab must stay
                        // unsaved: otherwise the dirty marker clears, closing
                        // it asks nothing and the edits are lost.
                        if let Some(edits) = pending_edits {
                            self.tabs[tab_idx].table.edits = edits;
                        }
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
        })
    }

    /// After a successful save of a transparently decompressed tab, compress
    /// the written temp file back onto the original `.gz` / `.zst` path.
    /// No-op unless the save landed on exactly the recorded temp path (so
    /// Save As to another file never touches the compressed original).
    fn maybe_recompress_saved(&mut self, tab_idx: usize, written: &std::path::Path) {
        let Some(origin) = self.tabs[tab_idx].compressed_origin.clone() else {
            return;
        };
        if origin.temp != written {
            return;
        }
        if let Err(e) =
            octa::formats::compression::compress_file(written, &origin.original, origin.codec)
        {
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
