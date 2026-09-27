//! Tab lifecycle: closing, pinning and reopening a closed tab.

use eframe::egui;

use crate::app::state::{OctaApp, TabState};

impl OctaApp {
    pub(crate) fn close_tab(&mut self, idx: usize) {
        // Pinned tabs refuse to close. The user has to unpin them from the
        // tab right-click context menu first; the status bar tells them.
        if self.tabs.get(idx).is_some_and(|t| t.pinned) {
            self.status_message = Some((
                "Tab is pinned; unpin from the tab right-click menu first.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        // Take a snapshot before removal so Ctrl+Shift+T can restore it.
        // Skip wholly empty tabs (no source path, no raw content, no
        // columns) - those would just be re-created empty.
        if let Some(tab) = self.tabs.get(idx) {
            let snapshot = if let Some(ref p) = tab.table.source_path {
                Some(crate::app::state::ClosedTabSnapshot::Path(
                    std::path::PathBuf::from(p),
                ))
            } else if let Some(ref content) = tab.raw_content {
                if !content.is_empty() || tab.table.col_count() > 0 {
                    Some(crate::app::state::ClosedTabSnapshot::Scratch {
                        raw_content: content.clone(),
                        view_mode: tab.view_mode,
                        format_name: tab.table.format_name.clone(),
                    })
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(snap) = snapshot {
                if self.recently_closed_tabs.len() >= crate::app::state::MAX_CLOSED_TAB_HISTORY {
                    self.recently_closed_tabs.pop_front();
                }
                self.recently_closed_tabs.push_back(snap);
            }
        }

        self.tabs.remove(idx);
        self.shift_refresh_indices(idx);
        if self.tabs.is_empty() {
            self.tabs
                .push(TabState::new(self.settings.default_search_mode));
        }
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    /// Toggle the pinned state for the tab at `idx`. Pinning a file-backed
    /// tab adds its absolute path to `AppSettings.pinned_tabs` so the file
    /// re-opens on next launch; unpinning removes it. Settings are saved
    /// immediately so the change survives a crash.
    ///
    /// Pinning a scratch tab (no `source_path`) is a no-op - the UI already
    /// greys out the menu entry for those.
    pub(crate) fn toggle_tab_pinned(&mut self, idx: usize) {
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        let Some(path) = tab.table.source_path.clone() else {
            return;
        };
        tab.pinned = !tab.pinned;
        let now_pinned = tab.pinned;
        let pinned_list = &mut self.settings.pinned_tabs;
        if now_pinned {
            if !pinned_list.contains(&path) {
                pinned_list.push(path);
            }
        } else {
            pinned_list.retain(|p| p != &path);
        }
        self.settings.save();
    }

    /// Restore the most-recently-closed tab (Ctrl+Shift+T). Path-backed tabs
    /// reload through the standard `load_file` pipeline; scratch tabs
    /// recreate from the stored raw_content. No-op when the close stack
    /// is empty.
    pub(crate) fn reopen_last_closed_tab(&mut self, ctx: &egui::Context) {
        let Some(snap) = self.recently_closed_tabs.pop_back() else {
            return;
        };
        match snap {
            crate::app::state::ClosedTabSnapshot::Path(path) => {
                self.load_file(path);
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                    self.tabs[self.active_tab].title_display(),
                ));
            }
            crate::app::state::ClosedTabSnapshot::Scratch {
                raw_content,
                view_mode,
                format_name,
            } => {
                let mut tab = TabState::new(self.settings.default_search_mode);
                tab.raw_content = Some(raw_content);
                tab.raw_content_original = tab.raw_content.clone();
                tab.view_mode = view_mode;
                tab.table.format_name = format_name;
                self.tabs.push(tab);
                self.active_tab = self.tabs.len() - 1;
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                    self.tabs[self.active_tab].title_display(),
                ));
            }
        }
    }
}
