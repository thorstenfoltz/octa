//! Dispatch for the Columns menu's [`ToolbarAction`] fields.
//!
//! One file per menu, mirroring `ui::toolbar::columns_menu`, so a menu entry and
//! the code it runs sit in matching files.

use octa::ui;

use crate::app::state::OctaApp;

impl OctaApp {
    /// Run every Columns-menu action set on `action`.
    pub(super) fn dispatch_columns_menu(&mut self, action: &ui::toolbar::ToolbarAction) {
        if action.add_column {
            self.tabs[self.active_tab].show_add_column_dialog = true;
            self.tabs[self.active_tab].new_col_name.clear();
            self.tabs[self.active_tab].new_col_type = "String".to_string();
            self.tabs[self.active_tab].new_col_formula.clear();
            self.tabs[self.active_tab].insert_col_at = self.tabs[self.active_tab]
                .table_state
                .selected_cell
                .map(|(_, c)| c + 1);
        }
        if action.delete_column && self.tabs[self.active_tab].table.col_count() > 0 {
            self.open_delete_columns_dialog();
        }
        if action.move_col_left
            && let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell
            && col > 0
        {
            self.tabs[self.active_tab].table.move_column(col, col - 1);
            self.tabs[self.active_tab].table_state.selected_cell = Some((row, col - 1));
            self.tabs[self.active_tab].table_state.widths_initialized = false;
        }
        if action.move_col_right
            && let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell
            && col + 1 < self.tabs[self.active_tab].table.col_count()
        {
            self.tabs[self.active_tab].table.move_column(col, col + 1);
            self.tabs[self.active_tab].table_state.selected_cell = Some((row, col + 1));
            self.tabs[self.active_tab].table_state.widths_initialized = false;
        }
        if action.sort_columns_asc {
            self.sort_columns_alphabetically(true);
        }
        if action.sort_columns_desc {
            self.sort_columns_alphabetically(false);
        }
        if action.show_all_columns {
            self.tabs[self.active_tab].hidden_columns.clear();
        }
        if action.open_column_format {
            self.open_column_format_for_selection();
        }
        if action.open_conditional_format {
            let tab = &mut self.tabs[self.active_tab];
            if tab.table.col_count() > 0 {
                tab.show_conditional_format = true;
            }
        }
        if (action.open_rename_columns || action.fix_duplicate_columns)
            && self.tabs[self.active_tab].table.col_count() > 0
            && !self.is_readonly()
        {
            let columns: Vec<String> = self.tabs[self.active_tab]
                .table
                .columns
                .iter()
                .map(|c| c.name.clone())
                .collect();
            let mut state = crate::app::state::RenameColumnsState::from_columns(&columns);
            // Same dialog either way; this entry just opens it onto its
            // duplicates half.
            state.fix_duplicates = action.fix_duplicate_columns;
            self.rename_columns_state = Some(state);
        }
    }
}
