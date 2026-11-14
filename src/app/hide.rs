//! Hiding columns and rows from the menus.
//!
//! Hiding is display only: the data stays in the table and a plain Save
//! writes it. Hidden columns live on the tab (`TabState.hidden_columns`),
//! hidden rows on the table (`DataTable.hidden_rows`), because rows move.
//!
//! What gets hidden is the selection: whole columns or rows when some are
//! selected, else the columns or rows of the selected cells.

use std::collections::BTreeSet;

use super::state::TabState;
use octa::i18n::t;

impl TabState {
    /// The columns **Hide selected columns** would hide.
    pub(crate) fn hide_column_targets(&self) -> BTreeSet<usize> {
        let state = &self.table_state;
        let picked: BTreeSet<usize> = if !state.selected_cols.is_empty() {
            state.selected_cols.iter().copied().collect()
        } else {
            let cells = state.selected_cells.iter().map(|&(_, c)| c);
            cells.chain(state.selected_cell.map(|(_, c)| c)).collect()
        };
        let known = self.table.col_count();
        picked
            .into_iter()
            .filter(|c| *c < known && !self.hidden_columns.contains(c))
            .collect()
    }

    /// Why **Hide selected columns** is greyed, `None` when it can run.
    pub(crate) fn hide_columns_block(&self) -> Option<String> {
        if self.table.col_count() == 0 {
            return Some(t("greyed.need_table"));
        }
        let targets = self.hide_column_targets();
        if targets.is_empty() {
            return Some(t("greyed.need_selection"));
        }
        let visible = self.table.col_count() - self.hidden_columns.len();
        (targets.len() >= visible).then(|| t("greyed.last_visible_col"))
    }

    /// Hide `clicked` and, when it is part of the column selection, the rest
    /// of that selection with it. The last visible column stays.
    pub(crate) fn hide_columns_from(&mut self, clicked: Option<usize>) {
        let targets: BTreeSet<usize> = match clicked {
            Some(c) if !self.table_state.selected_cols.contains(&c) => [c].into(),
            _ => self.hide_column_targets(),
        };
        let visible = self
            .table
            .col_count()
            .saturating_sub(self.hidden_columns.len());
        let new = targets
            .iter()
            .filter(|c| !self.hidden_columns.contains(c))
            .count();
        if new == 0 || new >= visible {
            return;
        }
        self.hidden_columns.extend(targets);
        self.table_state.selected_cols.clear();
    }

    /// The rows **Hide selected rows** would hide.
    pub(crate) fn hide_row_targets(&self) -> BTreeSet<usize> {
        let state = &self.table_state;
        if !state.selected_rows.is_empty() {
            return state.selected_rows.iter().copied().collect();
        }
        let cells = state.selected_cells.iter().map(|&(r, _)| r);
        cells.chain(state.selected_cell.map(|(r, _)| r)).collect()
    }

    /// Why hiding rows is greyed whatever is selected, `None` when it can run.
    /// A large-file tab holds one page of its file, and row numbers there
    /// mean a place on the page, not a row.
    pub(crate) fn hide_rows_unavailable(&self) -> Option<String> {
        if self.table.col_count() == 0 {
            return Some(t("greyed.need_table"));
        }
        self.large.is_some().then(|| t("greyed.hide_rows_paged"))
    }

    /// Why **Hide selected rows** is greyed, `None` when it can run.
    pub(crate) fn hide_rows_block(&self) -> Option<String> {
        self.hide_rows_unavailable().or_else(|| {
            self.hide_row_targets()
                .is_empty()
                .then(|| t("greyed.need_selection"))
        })
    }

    /// Hide `clicked` and, when it is part of the row selection, the rest of
    /// that selection with it. `None` hides the selection.
    pub(crate) fn hide_rows_from(&mut self, clicked: Option<usize>) {
        if self.hide_rows_unavailable().is_some() {
            return;
        }
        let targets: BTreeSet<usize> = match clicked {
            Some(r) if !self.table_state.selected_rows.contains(&r) => [r].into(),
            _ => self.hide_row_targets(),
        };
        if targets.is_empty() {
            return;
        }
        self.table.hide_rows(targets);
        self.table_state.selected_rows.clear();
        self.table_state.selected_cells.clear();
        self.table_state.selected_cell = None;
        self.filter_dirty = true;
    }

    pub(crate) fn show_hidden_rows(&mut self) {
        self.table.show_hidden_rows();
        self.filter_dirty = true;
    }

    /// The last step of the row filter: hidden rows are never shown.
    pub(crate) fn drop_hidden_rows(&mut self) {
        if !self.table.hidden_rows.is_empty() {
            let hidden = &self.table.hidden_rows;
            self.filtered_rows.retain(|r| !hidden.contains(r));
        }
    }
}

#[cfg(test)]
#[path = "hide_tests.rs"]
mod tests;
