//! "Open as..." / "Reopen as...": re-read a file through an explicitly chosen
//! reader (for files whose extension lies). Split out of `file_io/mod.rs`.

use crate::app::state::{OctaApp, TabState};
use octa::data::DataTable;

impl OctaApp {
    /// Re-read the active tab's file through the reader named `reader_name`
    /// (one of the `FormatRegistry` names: "JSON", "CSV", "Text", ...),
    /// replacing the tab's content in place. This is how a mislabelled file
    /// gets opened with the reader that actually understands it: a `.log` that
    /// is really JSON re-reads as a JSON tree instead of plain text.
    ///
    /// The file is re-read from disk, so any unsaved in-memory edits on this
    /// tab are discarded. No-op when the tab has no file behind it.
    pub(crate) fn reopen_active_as(&mut self, reader_name: &str) {
        let Some(path) = self.tabs[self.active_tab].table.source_path.clone() else {
            self.status_message =
                Some((octa::i18n::t("open_as.no_file"), std::time::Instant::now()));
            return;
        };
        self.read_into_active_tab_as(std::path::PathBuf::from(path), reader_name);
    }

    /// **File -> Open as...**: pick one or more files and read them all through
    /// the chosen reader, a tab each. These files are not open yet, so the
    /// picker is deliberately unfiltered: the whole point is to reach a file
    /// whose extension Octa would otherwise route to the wrong reader (a `.log`
    /// that is really JSON) or not list at all.
    pub(crate) fn open_files_as(&mut self, reader_name: &'static str) {
        let Some(paths) = rfd::FileDialog::new()
            .add_filter("All Files", &["*"])
            .pick_files()
        else {
            return;
        };
        for path in paths {
            // One tab per file, except that a blank tab is reused rather than
            // left stranded (same rule as every other open path).
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
            self.read_into_active_tab_as(path, reader_name);
        }
    }

    /// Read `path` through the reader named `reader_name` into the active tab.
    /// Shared by **File -> Open as...** (a file that is not open yet) and
    /// **View -> Reopen as...** (the file already in this tab).
    fn read_into_active_tab_as(&mut self, path: std::path::PathBuf, reader_name: &str) {
        // Read before touching the tab, so a failed parse leaves the current
        // content untouched. The registry borrow ends with this `match`.
        let read = match self.registry.reader_by_name(reader_name) {
            Some(reader) => reader.read_file(&path),
            None => {
                self.status_message = Some((
                    octa::i18n::t("open_as.no_reader"),
                    std::time::Instant::now(),
                ));
                return;
            }
        };

        match read {
            Ok(table) => {
                // `apply_loaded_table` spills into a new tab unless the active
                // one is empty and unmodified. The tab we want is the one we are
                // standing on, so blank it first to make it reuse that tab.
                self.tabs[self.active_tab].table = DataTable::empty();
                self.tabs[self.active_tab].raw_content_modified = false;
                self.apply_loaded_table(path, table);
            }
            Err(e) => {
                self.status_message = Some((
                    format!("{}: {e}", octa::i18n::t("open_as.failed")),
                    std::time::Instant::now(),
                ));
            }
        }
    }
}
