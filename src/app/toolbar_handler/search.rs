//! Dispatch for the Search menu's [`ToolbarAction`] fields.
//!
//! One file per menu, mirroring `ui::toolbar::search_menu`, so a menu entry and
//! the code it runs sit in matching files.

use octa::ui;

use crate::app::state::OctaApp;

impl OctaApp {
    /// Run every Search-menu action set on `action`.
    pub(super) fn dispatch_search_menu(&mut self, action: &ui::toolbar::ToolbarAction) {
        if action.toggle_replace_bar {
            self.tabs[self.active_tab].show_replace_bar =
                !self.tabs[self.active_tab].show_replace_bar;
        }
        if action.search_focus {
            self.search_focus_requested = true;
        }
        if let Some(preselect) = action.show_column_filter {
            self.open_column_filter_dialog(preselect);
        }
        if action.toggle_multi_search {
            self.toggle_multi_search();
        }
        if action.show_find_duplicates {
            let tab = &mut self.tabs[self.active_tab];
            if tab.table.col_count() > 0 {
                // Seed the key with the currently selected column (or the
                // selected cell's column) so common workflows don't need
                // an extra click. Otherwise leave empty and let the user
                // tick boxes.
                tab.find_duplicates_key_cols.clear();
                if !tab.table_state.selected_cols.is_empty() {
                    for &c in &tab.table_state.selected_cols {
                        if c < tab.table.col_count() {
                            tab.find_duplicates_key_cols.insert(c);
                        }
                    }
                } else if let Some((_, c)) = tab.table_state.selected_cell
                    && c < tab.table.col_count()
                {
                    tab.find_duplicates_key_cols.insert(c);
                }
                tab.show_find_duplicates = true;
            }
        }
        if action.open_fuzzy_duplicates && self.tabs[self.active_tab].table.col_count() > 0 {
            let mut st = crate::app::state::FuzzyDuplicatesState::default();
            // Seed the compare set from the selected column, if any.
            if let Some((_, c)) = self.tabs[self.active_tab].table_state.selected_cell
                && c < self.tabs[self.active_tab].table.col_count()
            {
                st.key_cols.insert(c);
            }
            self.fuzzy_duplicates_dialog = Some(st);
        }
    }
}
