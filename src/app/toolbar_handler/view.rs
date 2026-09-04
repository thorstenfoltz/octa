//! Dispatch for the View menu's [`ToolbarAction`] fields.
//!
//! One file per menu, mirroring `ui::toolbar::view_menu`, so a menu entry and
//! the code it runs sit in matching files.

use eframe::egui;

use octa::ui;

use crate::app::state::OctaApp;

impl OctaApp {
    /// Run every View-menu action set on `action`.
    pub(super) fn dispatch_view_menu(
        &mut self,
        ctx: &egui::Context,
        action: &ui::toolbar::ToolbarAction,
    ) {
        if action.zoom_in {
            self.zoom_percent = (self.zoom_percent + 5).min(500);
            self.apply_zoom(ctx);
            self.tabs[self.active_tab]
                .table_state
                .invalidate_row_heights();
        }
        if action.zoom_out {
            self.zoom_percent = self.zoom_percent.saturating_sub(5).max(25);
            self.apply_zoom(ctx);
            self.tabs[self.active_tab]
                .table_state
                .invalidate_row_heights();
        }
        if action.zoom_reset {
            self.zoom_percent = 100;
            self.apply_zoom(ctx);
            self.tabs[self.active_tab]
                .table_state
                .invalidate_row_heights();
        }
        if let Some(new_mode) = action.view_mode_changed {
            self.tabs[self.active_tab].view_mode = new_mode;
        }
        if action.toggle_readonly {
            self.toggle_readonly();
        }
        // Each entry owns one orientation: clicking it turns the split on in
        // that orientation, or off again when it is already the one showing.
        if action.toggle_split_view {
            let state = &mut self.tabs[self.active_tab].table_state;
            state.set_split(!(state.is_split() && !state.split_side_by_side), false);
        }
        if action.toggle_split_side_by_side {
            let state = &mut self.tabs[self.active_tab].table_state;
            state.set_split(!(state.is_split() && state.split_side_by_side), true);
        }
        if action.add_split_pane {
            self.change_split_panes(true);
        }
        if action.remove_split_pane {
            self.change_split_panes(false);
        }
        if action.compare_with {
            self.begin_compare_with();
        }
        if action.open_git_compare {
            self.open_git_compare_dialog();
        }
        if let Some(reader_name) = action.open_as {
            self.reopen_active_as(reader_name);
        }
    }

    /// One more band, or one fewer, within the 2..=`MAX_SPLIT_PANES` range.
    /// Silent when it cannot: the menu entry that asks for this is disabled
    /// with a hint saying why, and a key bound to it should not nag.
    pub(crate) fn change_split_panes(&mut self, add: bool) {
        let state = &mut self.tabs[self.active_tab].table_state;
        if add {
            state.add_split_pane();
        } else {
            state.remove_split_pane();
        }
    }
}
