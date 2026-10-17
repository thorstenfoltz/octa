//! Open and save the SQL editor's text as a `.sql` file. The editor
//! remembers its file (`SqlPane.file`), so Save writes straight back and the
//! header shows ` *` while the text differs from what is on disk.

use std::path::Path;

use octa::ui::status_bar::format_number;

use crate::app::file_io::save::keep_earlier_version;
use crate::app::state::{OctaApp, SqlFile, file_display_name, read_sql_file, write_sql_file};

impl OctaApp {
    /// Pick a `.sql` file and show it in an editor: the active one when it
    /// is blank, else a new one beside it.
    pub(crate) fn open_sql_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(octa::i18n::t("common.open"))
            .add_filter("SQL", &["sql"])
            .pick_file()
        else {
            return;
        };
        let tab = &mut self.tabs[self.active_tab];
        match std::fs::metadata(&path) {
            Ok(m) if !self.settings.raw_view_allows(m.len()) => {
                // In the Settings field's own unit (decimal MB), rounded up so
                // a file just over the cap never reads as equal to it.
                let mb = |b: u64| format!("{} MB", format_number(b.div_ceil(1_000_000) as usize));
                tab.sql.error = Some(
                    octa::i18n::t("sql.file_too_large")
                        .replace("{name}", &file_display_name(&path))
                        .replace("{size}", &mb(m.len()))
                        .replace("{limit}", &mb(self.settings.raw_view_max_bytes as u64)),
                );
                return;
            }
            Ok(_) => {}
            Err(e) => {
                tab.sql.error = Some(format!("{e:#}"));
                return;
            }
        }
        match read_sql_file(&path) {
            Ok((text, crlf)) => {
                tab.put_sql_file(path, text, crlf);
                tab.sql.error = None;
            }
            Err(e) => tab.sql.error = Some(format!("{e:#}")),
        }
    }

    /// Write the active editor back to its file. One without a file asks
    /// where first, like Save as.
    pub(crate) fn save_sql_file(&mut self) {
        let pane = &mut self.tabs[self.active_tab].sql;
        let Some(file) = pane.file.as_mut() else {
            self.save_sql_file_as();
            return;
        };
        if let Err(e) = keep_earlier_version(&self.settings, &file.path) {
            pane.error = Some(e);
            return;
        }
        match write_sql_file(&file.path, &pane.query, file.crlf) {
            Ok(()) => {
                file.saved = pane.query.clone();
                pane.error = None;
                self.status_message = Some(saved_message(&file.path));
            }
            Err(e) => pane.error = Some(format!("{e:#}")),
        }
    }

    /// Ask where to write the active editor, write it, and keep that file.
    pub(crate) fn save_sql_file_as(&mut self) {
        let current = self.tabs[self.active_tab].sql.file.as_ref();
        let crlf = current.is_some_and(|f| f.crlf);
        let mut picker = rfd::FileDialog::new()
            .set_title(octa::i18n::t("common.save_as"))
            .add_filter("SQL", &["sql"])
            .set_file_name(
                current.map_or_else(|| "query.sql".to_string(), |f| file_display_name(&f.path)),
            );
        if let Some(dir) = current.and_then(|f| f.path.parent()) {
            picker = picker.set_directory(dir);
        }
        let Some(path) = picker.save_file() else {
            return;
        };
        let pane = &mut self.tabs[self.active_tab].sql;
        if let Err(e) = keep_earlier_version(&self.settings, &path) {
            pane.error = Some(e);
            return;
        }
        match write_sql_file(&path, &pane.query, crlf) {
            Ok(()) => {
                self.status_message = Some(saved_message(&path));
                pane.error = None;
                pane.file = Some(SqlFile {
                    path,
                    saved: pane.query.clone(),
                    crlf,
                });
            }
            Err(e) => pane.error = Some(format!("{e:#}")),
        }
    }
}

fn saved_message(path: &Path) -> (String, std::time::Instant) {
    (
        octa::i18n::t("sql.file_saved").replace("{name}", &file_display_name(path)),
        std::time::Instant::now(),
    )
}
