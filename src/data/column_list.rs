//! Pure list model behind the column navigator panel: what the panel shows,
//! how its search box narrows it, and what a drag-reorder means as an index
//! permutation. No egui here; the panel is a renderer over this.

use crate::data::DataTable;
use std::collections::HashSet;

/// One row of the navigator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnEntry {
    pub index: usize,
    pub name: String,
    pub data_type: String,
    pub hidden: bool,
    pub frozen: bool,
}

/// Build the full list. `frozen_cols` is the count of leading frozen columns
/// the table view keeps, so entry `i` is frozen when `i < frozen_cols`.
pub fn column_entries(
    table: &DataTable,
    hidden: &HashSet<usize>,
    frozen_cols: usize,
) -> Vec<ColumnEntry> {
    table
        .columns
        .iter()
        .enumerate()
        .map(|(index, col)| ColumnEntry {
            index,
            name: col.name.clone(),
            data_type: col.data_type.clone(),
            hidden: hidden.contains(&index),
            frozen: index < frozen_cols,
        })
        .collect()
}

/// Narrow the list by a substring, case-insensitively. An empty query keeps
/// everything, so the caller never special-cases the unfiltered state.
pub fn filter_entries<'a>(entries: &'a [ColumnEntry], query: &str) -> Vec<&'a ColumnEntry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return entries.iter().collect();
    }
    entries
        .iter()
        .filter(|e| e.name.to_lowercase().contains(&q))
        .collect()
}

/// The index order after dragging `from` onto `to`. Out-of-range inputs and
/// no-op moves return the identity order rather than panicking, because a
/// drag that ends outside the list is a normal thing for a user to do.
pub fn move_column(order_len: usize, from: usize, to: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..order_len).collect();
    if from >= order_len || to >= order_len || from == to {
        return order;
    }
    let moved = order.remove(from);
    order.insert(to, moved);
    order
}

#[cfg(test)]
#[path = "column_list_tests.rs"]
mod tests;
