//! Tab lifecycle: closing, pinning and reopening a closed tab.

use eframe::egui;

use crate::app::state::{OctaApp, TabState};

impl OctaApp {
    pub(crate) fn close_tab(&mut self, idx: usize) {
        // Pinned tabs refuse to close. The user has to unpin them from the
        // tab right-click context menu first; the status bar tells them.
        if self.tabs.get(idx).is_some_and(|t| t.pinned) {
            self.status_message =
                Some((octa::i18n::t("msg.tab_pinned"), std::time::Instant::now()));
            return;
        }
        // Take a snapshot before removal so Ctrl+Shift+T can restore it.
        // Skip wholly empty tabs (no source path, no raw content, no
        // columns) - those would just be re-created empty.
        if let Some(tab) = self.tabs.get(idx) {
            // A protected tab reopens from its zip: the plain copy goes with
            // the tab.
            let snapshot = if let Some(o) = &tab.protected_origin {
                Some(crate::app::state::ClosedTabSnapshot::Path(o.zip.clone()))
            } else if let Some(ref p) = tab.table.source_path {
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

        let was_active = idx == self.active_tab;
        let closed = self.tabs.remove(idx);
        // The active tab stays the active tab when one before it goes.
        if idx < self.active_tab {
            self.active_tab -= 1;
        }
        self.shift_refresh_indices(idx);
        self.normalise_groups();
        self.dissolve_small_tile_sets();
        if self.tabs.is_empty() {
            self.tabs
                .push(TabState::new(self.settings.default_search_mode));
        }
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.keep_sql_editors(closed, was_active);
    }

    /// An SQL editor only goes when the user closes it (Settings -> SQL), not
    /// with its tab. The active tab takes the editors over whole when it can
    /// show them and has none of its own in use. Otherwise they get a tab of
    /// their own at the end of the bar, so closing a tab in the background
    /// never disturbs the one on screen.
    ///
    /// A tab that held nothing but editors, with nowhere free for them, is
    /// the exception: its close button can only have meant them, and another
    /// tab of their own would make it impossible to close.
    fn keep_sql_editors(&mut self, mut closed: TabState, was_active: bool) {
        if !self.settings.sql_keep_open {
            return;
        }
        if !closed.sql_survives_close() {
            if was_active && closed.sql_panel_open {
                self.open_sql_panel(self.active_tab);
            }
            return;
        }
        let host = if was_active && self.tabs[self.active_tab].sql_can_host() {
            self.active_tab
        } else if closed.has_own_content() {
            self.tabs
                .push(TabState::new(self.settings.default_search_mode));
            self.tabs.len() - 1
        } else {
            return;
        };
        closed.move_sql_to(&mut self.tabs[host]);
        if was_active {
            self.active_tab = host;
        }
        self.open_sql_panel(host);
        self.rehome_sql_workspace(host);
    }

    /// Quit, after the unsaved-changes prompt for every modified tab, the one
    /// on screen first. No tab closes on the way, so Reopen last session still
    /// sees them all.
    pub(crate) fn request_exit(&mut self, ctx: &egui::Context) {
        self.pending_close_tab = None;
        self.close_queue.clear();
        let active = self.tabs[self.active_tab].id;
        let mut queue: Vec<_> = self
            .tabs
            .iter()
            .filter(|t| t.is_modified())
            .map(|t| t.id)
            .collect();
        queue.sort_by_key(|&id| id != active);
        self.exit_queue = Some(queue);
        self.next_exit_tab(ctx);
    }

    /// A tab of a quit was dealt with: show the next modified one with the
    /// prompt, or close Octa when none is left. Does nothing once the quit was
    /// cancelled, so a save landing afterwards cannot finish it.
    pub(crate) fn next_exit_tab(&mut self, ctx: &egui::Context) {
        let Some(queue) = self.exit_queue.as_mut() else {
            return;
        };
        while !queue.is_empty() {
            let id = queue.remove(0);
            if let Some(i) = self.tabs.iter().position(|t| t.id == id && t.is_modified()) {
                self.active_tab = i;
                self.show_close_confirm = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.tabs[i].title_display()));
                return;
            }
        }
        self.exit_queue = None;
        self.confirmed_close = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// The unsaved-changes prompt was dismissed: nothing closes, and whatever
    /// was going to close after it, a tab group or Octa, stays open too.
    pub(crate) fn dismiss_close_prompt(&mut self) {
        self.show_close_confirm = false;
        self.pending_close_tab = None;
        self.close_queue.clear();
        self.exit_queue = None;
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

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
