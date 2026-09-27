//! "Filter to marked" toggle and session bookmark navigation. Both operate on
//! the active tab and mutate its session-only state.

use super::state::{Bookmark, BookmarkDraft, OctaApp, TabRenameDraft};

impl OctaApp {
    /// Toggle "Filter to marked" on the active tab. Engaging it snapshots the
    /// current manual `hidden_columns`, hides every unmarked column (only when
    /// the marks constrain columns), and lets `recompute_filter` keep only the
    /// marked rows. Disengaging restores the snapshot exactly.
    pub(crate) fn toggle_filter_to_marked(&mut self) {
        let mode = self.settings.mark_filter_cell_mode;
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        if tab.mark_filter_active {
            // Toggle OFF: restore the pre-filter hidden set.
            if let Some(snap) = tab.mark_filter_hidden_snapshot.take() {
                tab.hidden_columns = snap;
            }
            tab.mark_filter_active = false;
            tab.filter_dirty = true;
            return;
        }

        let ks = octa::data::mark_filter::mark_keep_set(&tab.table.marks, mode);
        if ks.rows.is_empty() && ks.cols.is_empty() {
            self.status_message = Some((
                octa::i18n::t("edit_menu.filter_to_marked_none"),
                std::time::Instant::now(),
            ));
            return;
        }
        tab.mark_filter_hidden_snapshot = Some(tab.hidden_columns.clone());
        // Hide every column not in the keep-set, but only when at least one
        // column is marked (a rows-only mark set keeps all columns visible).
        if !ks.cols.is_empty() {
            for c in 0..tab.table.col_count() {
                if !ks.cols.contains(&c) {
                    tab.hidden_columns.insert(c);
                }
            }
        }
        tab.mark_filter_active = true;
        tab.filter_dirty = true;
    }

    /// Begin adding a bookmark: open the naming dialog seeded with the active
    /// tab's current selection (a selected cell -> row+col, else a selected row
    /// -> whole row). No selection raises a status toast.
    pub(crate) fn begin_add_bookmark(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let target = if let Some((row, col)) = tab.table_state.selected_cell {
            Some((row, Some(col)))
        } else if let Some(&row) = tab.table_state.selected_rows.iter().next() {
            Some((row, None))
        } else {
            None
        };
        let Some((row, col)) = target else {
            self.status_message = Some((
                octa::i18n::t("bookmarks.need_selection"),
                std::time::Instant::now(),
            ));
            return;
        };
        self.bookmark_draft = Some(BookmarkDraft {
            name_buf: String::new(),
            row,
            col,
            size: octa::ui::settings::DialogSize::default(),
        });
    }

    /// Commit the pending bookmark draft to the active tab.
    pub(crate) fn commit_bookmark_draft(&mut self, name: String, row: usize, col: Option<usize>) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.bookmarks.push(Bookmark { name, row, col });
        }
    }

    /// Open the "Rename tab" dialog for tab `index`, seeded with its current
    /// display name (the auto label if it has not been renamed yet).
    pub(crate) fn begin_rename_tab(&mut self, index: usize) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        // Seed with the user's existing name, else the auto title (minus any
        // trailing " *" modified marker so the user does not edit the marker).
        let seed = tab.user_tab_name.clone().unwrap_or_else(|| {
            let t = tab.title_display();
            t.strip_suffix(" *").map(str::to_string).unwrap_or(t)
        });
        self.tab_rename_draft = Some(TabRenameDraft {
            tab_index: index,
            name_buf: seed,
            size: octa::ui::settings::DialogSize::default(),
        });
    }

    /// Commit a tab rename: set (or clear, when empty) the tab's display name.
    pub(crate) fn commit_tab_rename(&mut self, index: usize, name: String) {
        if let Some(tab) = self.tabs.get_mut(index) {
            let trimmed = name.trim();
            tab.user_tab_name = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
    }

    /// Jump to the bookmark at `index` in the active tab: select the target
    /// cell and scroll it into view.
    pub(crate) fn jump_to_bookmark(&mut self, index: usize) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let Some(bm) = tab.bookmarks.get(index) else {
            return;
        };
        self.jump_to_cell(bm.row, bm.col.unwrap_or(0));
    }

    /// Select `(row, col)` in the active tab and scroll it into view. Shared
    /// by bookmark jump and the edit audit trail's jump button.
    ///
    /// The vertical scroll goes through `TableViewState::scroll_to_row`
    /// rather than a guessed pixel offset: the row height is only known
    /// inside `draw_table`, so a guess here would land the viewport short on
    /// a large page. If the row is currently filtered out of view
    /// (`display_idx` not found), the cell is still selected but the
    /// viewport is left alone - there is nowhere on screen to scroll it to.
    pub(crate) fn jump_to_cell(&mut self, row: usize, col: usize) {
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        if row >= tab.table.row_count() {
            return;
        }
        tab.table_state.selected_cell = Some((row, col));
        tab.table_state.selected_rows.clear();
        tab.table_state.selected_cols.clear();
        tab.table_state.selected_cells.clear();
        if let Some(display_idx) = tab.filtered_rows.iter().position(|&r| r == row) {
            tab.table_state.scroll_to_row(display_idx);
        }
        if col < tab.table_state.col_widths.len() {
            let col_left: f32 = tab.table_state.col_widths[..col].iter().sum();
            tab.table_state.set_scroll_x(col_left);
        }
    }
}
