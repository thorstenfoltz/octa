//! Dispatch for the Edit menu's [`ToolbarAction`] fields.
//!
//! One file per menu, mirroring `ui::toolbar::edit_menu`, so a menu entry and
//! the code it runs sit in matching files.

use eframe::egui;

use octa::ui;

use crate::app::state::OctaApp;

impl OctaApp {
    /// Run every Edit-menu action set on `action`.
    pub(super) fn dispatch_edit_menu(
        &mut self,
        ctx: &egui::Context,
        action: &ui::toolbar::ToolbarAction,
    ) {
        // The menu greys these out; this is the gate that holds regardless.
        let ro = self.is_readonly();
        if let Some(scope) = action.parse_in_new_tab {
            let tab = &self.tabs[self.active_tab];
            self.pending_parse_modal =
                crate::app::dialogs::parse_in_new_tab::build_modal_state(tab, scope);
        }
        if action.add_row && !ro {
            let insert_at = match self.tabs[self.active_tab].table_state.selected_cell {
                Some((row, _)) => row + 1,
                None => self.tabs[self.active_tab].table.row_count(),
            };
            self.tabs[self.active_tab].table.insert_row(insert_at);
            let sel_col = self.tabs[self.active_tab]
                .table_state
                .selected_cell
                .map(|(_, c)| c)
                .unwrap_or(0);
            self.tabs[self.active_tab].table_state.selected_cell = Some((insert_at, sel_col));
            self.tabs[self.active_tab].table_state.editing_cell = None;
            self.tabs[self.active_tab].filter_dirty = true;
        }
        if action.delete_row
            && !ro
            && let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell
        {
            self.tabs[self.active_tab].table.delete_row(row);
            self.tabs[self.active_tab].table_state.editing_cell = None;
            if self.tabs[self.active_tab].table.row_count() == 0 {
                self.tabs[self.active_tab].table_state.selected_cell = None;
            } else {
                let new_row = row.min(self.tabs[self.active_tab].table.row_count() - 1);
                self.tabs[self.active_tab].table_state.selected_cell = Some((new_row, col));
            }
            self.tabs[self.active_tab].filter_dirty = true;
        }
        if action.move_row_up
            && !ro
            && let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell
            && row > 0
        {
            self.tabs[self.active_tab].table.move_row(row, row - 1);
            self.tabs[self.active_tab].table_state.selected_cell = Some((row - 1, col));
            self.tabs[self.active_tab].filter_dirty = true;
        }
        if action.move_row_down
            && !ro
            && let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell
            && row + 1 < self.tabs[self.active_tab].table.row_count()
        {
            self.tabs[self.active_tab].table.move_row(row, row + 1);
            self.tabs[self.active_tab].table_state.selected_cell = Some((row + 1, col));
            self.tabs[self.active_tab].filter_dirty = true;
        }
        if let Some(col_idx) = action.sort_rows_asc_by {
            self.sort_tab_rows(self.active_tab, &[(col_idx, true)]);
            self.record_sort(col_idx, true);
        }
        if let Some(col_idx) = action.sort_rows_desc_by {
            self.sort_tab_rows(self.active_tab, &[(col_idx, false)]);
            self.record_sort(col_idx, false);
        }
        if action.discard_edits && !ro {
            self.tabs[self.active_tab].table.discard_edits();
        }
        if action.toggle_first_row_header && !ro {
            if octa::data::header_rows::has_real_header(&self.tabs[self.active_tab].table.columns) {
                self.apply_header_is_data();
            } else {
                self.apply_header_rows(0, 0);
            }
        }
        for (key, color) in action.set_marks.iter().cloned() {
            self.tabs[self.active_tab].table.set_mark(key, color);
        }
        for key in action.clear_marks.iter().cloned() {
            self.tabs[self.active_tab].table.clear_mark(key);
        }
        if action.clear_all_marks {
            self.tabs[self.active_tab].table.clear_all_marks();
        }
        if action.undo && !ro {
            self.do_undo();
        }
        if action.redo && !ro {
            self.do_redo();
        }
        if action.reopen_last_closed_tab {
            self.reopen_last_closed_tab(ctx);
        }
        if action.fit_all_columns {
            self.tabs[self.active_tab]
                .table_state
                .fit_all_columns_requested = true;
        }
        if action.fit_all_rows {
            self.fit_all_rows();
        }
        if action.copy_as_markdown {
            self.do_copy_markdown(ctx);
        }
        if action.copy_as_in_list {
            self.do_copy_in_list(ctx);
        }
    }
}
