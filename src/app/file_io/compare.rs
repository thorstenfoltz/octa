//! Compare-view entry points: pick a right-side file, a git revision, or a
//! sibling tab and load it into the active tab's `compare_*` fields, then switch
//! into `ViewMode::Compare`. Split out of `file_io/mod.rs`.

use crate::app::state::OctaApp;

impl OctaApp {
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
}
