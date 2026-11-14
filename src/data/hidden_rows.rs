//! Rows the user has hidden from view.
//!
//! Hiding is display only, like hiding a column: the row stays in the table,
//! a plain Save writes it, and nothing here is saved into a file or undone.
//! The set lives on [`DataTable`] rather than beside the view's filters
//! because rows move inside the table (insert, delete, sort, undo), and a
//! hidden row has to stay the same row through all of it.

use super::DataTable;

impl DataTable {
    /// Hide these rows. Indices past the end are ignored.
    pub fn hide_rows(&mut self, rows: impl IntoIterator<Item = usize>) {
        let count = self.rows.len();
        self.hidden_rows
            .extend(rows.into_iter().filter(|&r| r < count));
    }

    /// Show every hidden row again.
    pub fn show_hidden_rows(&mut self) {
        self.hidden_rows.clear();
    }

    /// A row was inserted at `index`: the hidden rows from there on move down.
    pub(super) fn hidden_rows_after_insert(&mut self, index: usize) {
        self.remap_hidden_rows(|r| Some(if r >= index { r + 1 } else { r }));
    }

    /// The row at `index` was removed, and with it its place in the set.
    pub(super) fn hidden_rows_after_remove(&mut self, index: usize) {
        self.remap_hidden_rows(|r| match r.cmp(&index) {
            std::cmp::Ordering::Less => Some(r),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(r - 1),
        });
    }

    /// The rows were reordered: row `r` now sits at `old_to_new[r]`.
    pub(super) fn hidden_rows_after_reorder(&mut self, old_to_new: &[usize]) {
        self.remap_hidden_rows(|r| old_to_new.get(r).copied());
    }

    /// The first `count` rows were dropped from the front.
    pub(super) fn hidden_rows_after_evict(&mut self, count: usize) {
        self.remap_hidden_rows(|r| r.checked_sub(count));
    }

    fn remap_hidden_rows(&mut self, to: impl Fn(usize) -> Option<usize>) {
        if !self.hidden_rows.is_empty() {
            self.hidden_rows = self.hidden_rows.iter().filter_map(|&r| to(r)).collect();
        }
    }
}

#[cfg(test)]
#[path = "hidden_rows_tests.rs"]
mod tests;
