//! The GUI side of fixing the header row. One entry point per action,
//! shared by the row menus, the load banner and Edit > First row is header,
//! so all three check the same reasons, record the same Recipe step and
//! leave the view in the same state. The work is `octa::data::header_rows`.

use octa::data::header_rows;
use octa::data::recipe::{HeaderIsData, RecipeStep, UseHeaderRows};
use octa::i18n::t;

use crate::app::state::OctaApp;

impl OctaApp {
    /// Why the header actions are off on the active tab, or `None`.
    /// `removes_rows`: the action deletes rows from the top, which a filter
    /// may be hiding.
    pub(crate) fn header_row_block_reason(&self, removes_rows: bool) -> Option<String> {
        let tab = self.tabs.get(self.active_tab)?;
        if tab.db_origin.is_some() {
            return Some(t("context_menu.header_row_db"));
        }
        if self.is_readonly() {
            return Some(t("context_menu.header_row_readonly"));
        }
        if removes_rows && tab.filtered_rows.len() != tab.table.row_count() {
            return Some(t("context_menu.header_row_filtered"));
        }
        None
    }

    /// Use rows `first..=last` (0-based) of the active tab as its column names.
    pub(crate) fn apply_header_rows(&mut self, first: usize, last: usize) {
        if let Some(why) = self.header_row_block_reason(true) {
            self.status_message = Some((why, std::time::Instant::now()));
            return;
        }
        let dates = self.settings.detect_dates_on_load;
        let tab = &mut self.tabs[self.active_tab];
        match header_rows::use_header_rows(&mut tab.table, first, last, dates) {
            Ok(()) => {
                self.after_header_change();
                self.record_step(RecipeStep::UseHeaderRows(UseHeaderRows {
                    first: first + 1,
                    last: last + 1,
                    dates,
                }));
            }
            Err(e) => self.status_message = Some((format!("{e:#}"), std::time::Instant::now())),
        }
    }

    /// Turn the active tab's column names into its first row.
    pub(crate) fn apply_header_is_data(&mut self) {
        if let Some(why) = self.header_row_block_reason(false) {
            self.status_message = Some((why, std::time::Instant::now()));
            return;
        }
        let dates = self.settings.detect_dates_on_load;
        let tab = &mut self.tabs[self.active_tab];
        match header_rows::header_is_data(&mut tab.table, dates) {
            Ok(()) => {
                self.after_header_change();
                self.record_step(RecipeStep::HeaderIsData(HeaderIsData { dates }));
            }
            Err(e) => self.status_message = Some((format!("{e:#}"), std::time::Instant::now())),
        }
    }

    /// The view after the columns and top rows changed under it.
    fn after_header_change(&mut self) {
        // The load banners' Dismiss puts original values back by row index,
        // which the removed or added rows just shifted: keep what they did.
        let here = Some(self.active_tab);
        if self.pending_date_warning.as_ref().map(|w| w.tab_idx) == here {
            self.pending_date_warning = None;
        }
        if self.pending_date_parse_warning.as_ref().map(|w| w.tab_idx) == here {
            self.pending_date_parse_warning = None;
        }
        if self.pending_trim_warning.as_ref().map(|w| w.tab_idx) == here {
            self.pending_trim_warning = None;
        }
        if self.pending_number_warning.as_ref().map(|w| w.tab_idx) == here {
            self.pending_number_warning = None;
        }
        let tab = &mut self.tabs[self.active_tab];
        tab.header_suggestion = None;
        tab.filter_dirty = true;
        tab.table_state.widths_initialized = false;
        tab.table_state.editing_cell = None;
        tab.table_state.selected_rows.clear();
        tab.table_state.selected_cols.clear();
        tab.table_state.selected_cell =
            (tab.table.row_count() > 0 && tab.table.col_count() > 0).then_some((0, 0));
    }
}
