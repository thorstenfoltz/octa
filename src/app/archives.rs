//! Archive-tab helpers: detect that the active tab was loaded as an
//! archive listing, and provide the "Open selected entry" gesture
//! that extracts an entry into a tempfile and opens it as a new tab
//! via the normal `OctaApp::load_file` path.

use std::io::Write;

use eframe::egui;

use octa::formats::archive_reader::extract_entry_bytes;

use super::state::OctaApp;

impl OctaApp {
    /// Whether the active tab was opened as an archive (zip / tar /
    /// tgz). Drives the action-bar visibility above the table.
    pub(crate) fn active_tab_is_archive(&self) -> bool {
        self.tabs[self.active_tab]
            .table
            .format_name
            .as_deref()
            .is_some_and(|n| n.starts_with("Archive"))
    }

    /// Render a one-row action bar above the archive table. Currently
    /// the only action is **Open selected entry** — extracts the
    /// entry referenced by the active cell's row into a tempfile and
    /// opens it as a new tab. Greyed when no row is selected or the
    /// row points at a directory entry.
    pub(crate) fn render_archive_action_bar(&mut self, ui: &mut egui::Ui) {
        if !self.active_tab_is_archive() {
            return;
        }

        // Capture the bits we need from the active tab up front so the
        // borrow doesn't conflict with the open-entry mutation below.
        let (selected_row, entry_path, is_dir, archive_path, format_label) = {
            let tab = &self.tabs[self.active_tab];
            let selected_row = tab.table_state.selected_cell.map(|(r, _)| r);
            let entry_path = selected_row
                .and_then(|r| tab.table.get(r, 0))
                .map(|v| v.to_string());
            let is_dir = selected_row
                .and_then(|r| tab.table.get(r, 4))
                .map(|v| v.to_string() == "true")
                .unwrap_or(false);
            let archive_path = tab.table.source_path.clone();
            let label = tab
                .table
                .format_name
                .clone()
                .unwrap_or_else(|| "Archive".to_string());
            (selected_row, entry_path, is_dir, archive_path, label)
        };

        let can_open =
            selected_row.is_some() && entry_path.is_some() && !is_dir && archive_path.is_some();

        let mut requested = false;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format_label)
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(8.0);
            let btn = ui.add_enabled(can_open, egui::Button::new("Open selected entry"));
            if btn.clicked() {
                requested = true;
            }
            if !can_open {
                let hint = if selected_row.is_none() {
                    "Select a row first."
                } else if is_dir {
                    "Directories can't be opened."
                } else {
                    ""
                };
                if !hint.is_empty() {
                    ui.label(
                        egui::RichText::new(hint)
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                }
            }
        });
        ui.add_space(2.0);

        if requested && let (Some(entry_path), Some(archive_path)) = (entry_path, archive_path) {
            self.open_archive_entry(&archive_path, &entry_path);
        }
    }

    fn open_archive_entry(&mut self, archive_path: &str, entry_path: &str) {
        let bytes = match extract_entry_bytes(std::path::Path::new(archive_path), entry_path) {
            Ok(b) => b,
            Err(e) => {
                self.status_message = Some((
                    format!("Archive: extract failed: {}", e),
                    std::time::Instant::now(),
                ));
                return;
            }
        };

        // Pick the extension from the entry path so the format
        // registry routes the tempfile to the right reader. Fall back
        // to .bin for extension-less entries.
        let ext = std::path::Path::new(entry_path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_string())
            .unwrap_or_else(|| "bin".to_string());
        let suffix = format!(".{}", ext);
        let file_name = std::path::Path::new(entry_path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "entry".to_string());
        let prefix = format!("octa-archive-{}-", sanitize_prefix(&file_name));

        let tmp = match tempfile::Builder::new()
            .prefix(&prefix)
            .suffix(&suffix)
            .tempfile()
        {
            Ok(t) => t,
            Err(e) => {
                self.status_message = Some((
                    format!("Archive: tempfile create: {}", e),
                    std::time::Instant::now(),
                ));
                return;
            }
        };
        let path = tmp.path().to_path_buf();
        if let Err(e) = tmp.as_file().write_all(&bytes) {
            self.status_message = Some((
                format!("Archive: tempfile write: {}", e),
                std::time::Instant::now(),
            ));
            return;
        }
        // Leak the handle so the file survives past the load — readers
        // may stream from disk. OS cleans /tmp on reboot. Same trick
        // Parse-in-new-tab uses.
        let _ = tmp.keep();

        self.load_file(path);

        // Stamp a friendly label so the new tab's status row hints at
        // the origin, then clear the source path so Save prompts
        // instead of overwriting /tmp.
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.table.source_path = None;
            let label = format!(
                "{} (from {}!/{})",
                tab.table.format_name.clone().unwrap_or_default(),
                std::path::Path::new(archive_path)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| archive_path.to_string()),
                entry_path
            );
            tab.table.format_name = Some(label);
        }
        self.status_message = Some((
            format!("Archive: opened \"{}\"", entry_path),
            std::time::Instant::now(),
        ));
    }
}

/// Make a string safe to drop into a tempfile prefix. Replaces
/// anything outside `[A-Za-z0-9_-]` with `_` so the prefix is
/// portable.
fn sanitize_prefix(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
