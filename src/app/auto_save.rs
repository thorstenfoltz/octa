//! Auto-save: periodically write modified, file-backed tabs to disk.
//!
//! Driven once per frame by [`OctaApp::drive_auto_save`] from the update loop.
//! When enabled and the configured interval has elapsed, every *eligible* tab is
//! written through the normal save path. Ineligibility (no file on disk, a cloud
//! tab with writing off, a save that would pop a prompt, or a tab being edited
//! right now) is handled by skipping that tab silently, so the timer never
//! interrupts the user with a dialog. See the batch design doc for the rules.

use eframe::egui;

use super::state::OctaApp;

impl OctaApp {
    /// Per-frame auto-save tick. Cheap when disabled or the interval has not yet
    /// elapsed (a single `Instant::elapsed` comparison).
    pub(crate) fn drive_auto_save(&mut self, _ctx: &egui::Context) {
        if !self.settings.auto_save_enabled {
            return;
        }
        let interval = std::time::Duration::from_secs(
            self.settings.auto_save_interval_minutes.max(1) as u64 * 60,
        );
        if self.last_auto_save.elapsed() < interval {
            return;
        }
        self.last_auto_save = std::time::Instant::now();
        self.run_auto_save_pass();
    }

    /// Save every eligible tab. Returns nothing; sets a status note only when at
    /// least one file was written.
    fn run_auto_save_pass(&mut self) {
        let mut saved = 0usize;
        for idx in 0..self.tabs.len() {
            if !self.tab_is_auto_saveable(idx) {
                continue;
            }
            // Reuse the normal per-tab save (full table, never the filtered
            // view; also handles the cloud upload). It may set its own
            // "Saved to ..." status; we overwrite it with the aggregate note
            // below. Eligibility already excluded the cloud-writes-off case, so
            // `save_tab`'s early return never fires here.
            self.save_tab(idx);
            saved += 1;
        }
        if saved > 0 {
            self.status_message = Some((
                octa::i18n::t("auto_save.done").replace("{n}", &saved.to_string()),
                std::time::Instant::now(),
            ));
        }
    }

    /// Whether tab `idx` can be auto-saved right now without prompting the user
    /// or losing data. Mirrors the eligibility rules in the design doc.
    fn tab_is_auto_saveable(&self, idx: usize) -> bool {
        let Some(tab) = self.tabs.get(idx) else {
            return false;
        };
        // Must be a real file on disk with unsaved changes.
        if tab.table.source_path.is_none() {
            return false;
        }
        if !(tab.is_modified() || tab.raw_content_modified) {
            return false;
        }
        // Cloud tab with writing turned off (globally or per connection): the
        // manual save would just toast a "writes disabled" message, so skip
        // silently.
        if !self.cloud_tab_writable(idx) {
            return false;
        }
        // A read-only format (SAS, HDF5, ...) can't be written, so a save would
        // only toast an error every tick. Raw-text edits still save (they write
        // the buffer directly, bypassing the reader), so only gate table edits.
        if !tab.raw_content_modified
            && let Some(path) = tab.table.source_path.as_ref()
            && let Some(reader) = self.registry.reader_for_path(std::path::Path::new(path))
            && !reader.supports_write()
        {
            return false;
        }
        // A display-rounding format would pop the "save rounded values?" prompt.
        if tab
            .column_number_formats
            .values()
            .any(|f| f.rounds_values())
        {
            return false;
        }
        // A database schema change (added/removed columns vs the on-disk table)
        // would pop the schema-change confirm. Non-schema DB edits save fine.
        if let Some(meta) = tab.table.db_meta.as_ref() {
            let cur: Vec<&str> = tab.table.columns.iter().map(|c| c.name.as_str()).collect();
            let orig: Vec<&str> = meta.original_columns.iter().map(|s| s.as_str()).collect();
            if cur != orig {
                return false;
            }
        }
        // Do not commit a half-typed cell: skip a tab being edited this instant
        // (it saves on the next tick once the edit is committed).
        if tab.table_state.editing_cell.is_some() {
            return false;
        }
        true
    }
}
