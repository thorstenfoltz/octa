//! Per-cell / per-row / per-column operations triggered from the toolbar,
//! context menu, or keyboard shortcuts.

use octa::data;

use super::state::OctaApp;

impl OctaApp {
    /// Apply a text transformation to every selected cell. The target cells are
    /// the intersection of `selected_rows` × `selected_cols` when both are set;
    /// otherwise every cell in the selected rows or columns; otherwise the
    /// single selected cell. Non-string cells are skipped.
    pub(crate) fn transform_selected_cells(&mut self, transform: fn(&str) -> String) {
        let tab = &mut self.tabs[self.active_tab];
        if tab.table.col_count() == 0 {
            return;
        }
        let state = &tab.table_state;
        let row_count = tab.table.row_count();
        let col_count = tab.table.col_count();

        let targets: Vec<(usize, usize)> = match (
            !state.selected_rows.is_empty(),
            !state.selected_cols.is_empty(),
            state.selected_cell,
        ) {
            (true, true, _) => state
                .selected_rows
                .iter()
                .flat_map(|&r| state.selected_cols.iter().map(move |&c| (r, c)))
                .collect(),
            (true, false, _) => state
                .selected_rows
                .iter()
                .flat_map(|&r| (0..col_count).map(move |c| (r, c)))
                .collect(),
            (false, true, _) => state
                .selected_cols
                .iter()
                .flat_map(|&c| (0..row_count).map(move |r| (r, c)))
                .collect(),
            (false, false, Some(rc)) => vec![rc],
            _ => Vec::new(),
        };

        for (r, c) in targets {
            if r >= row_count || c >= col_count {
                continue;
            }
            if let Some(cv) = tab.table.get(r, c).cloned() {
                if let data::CellValue::String(s) = cv {
                    let new_val = transform(&s);
                    if new_val != s {
                        tab.table.set(r, c, data::CellValue::String(new_val));
                    }
                }
            }
        }
    }

    pub(crate) fn duplicate_selected_rows(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        if tab.table.col_count() == 0 {
            return;
        }
        let mut rows: Vec<usize> = if !tab.table_state.selected_rows.is_empty() {
            tab.table_state.selected_rows.iter().copied().collect()
        } else if let Some((r, _)) = tab.table_state.selected_cell {
            vec![r]
        } else {
            return;
        };
        // Insert from highest index to lowest so earlier insertions don't shift later targets.
        rows.sort_unstable_by(|a, b| b.cmp(a));
        let col_count = tab.table.col_count();
        for r in rows {
            if r >= tab.table.row_count() {
                continue;
            }
            let values: Vec<data::CellValue> = (0..col_count)
                .map(|c| {
                    tab.table
                        .get(r, c)
                        .cloned()
                        .unwrap_or(data::CellValue::Null)
                })
                .collect();
            tab.table.insert_row(r + 1);
            for (c, v) in values.into_iter().enumerate() {
                tab.table.set(r + 1, c, v);
            }
        }
        tab.filter_dirty = true;
    }

    pub(crate) fn delete_selected_rows(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        if tab.table.col_count() == 0 || tab.table.row_count() == 0 {
            return;
        }
        let mut rows: Vec<usize> = if !tab.table_state.selected_rows.is_empty() {
            tab.table_state.selected_rows.iter().copied().collect()
        } else if let Some((r, _)) = tab.table_state.selected_cell {
            vec![r]
        } else {
            return;
        };
        rows.sort_unstable_by(|a, b| b.cmp(a));
        for r in rows {
            if r < tab.table.row_count() {
                tab.table.delete_row(r);
            }
        }
        tab.table_state.selected_rows.clear();
        tab.table_state.selected_cell = None;
        tab.filter_dirty = true;
    }

    pub(crate) fn reload_active_file(&mut self) {
        let Some(path) = self.tabs[self.active_tab].table.source_path.clone() else {
            return;
        };
        let tab = &mut self.tabs[self.active_tab];
        tab.table.discard_edits();
        tab.table.clear_modified();
        tab.raw_content_modified = false;
        self.load_file(std::path::PathBuf::from(path));
    }

    /// Open the "Delete Columns" dialog, initializing checkboxes.
    pub(crate) fn open_delete_columns_dialog(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        tab.delete_col_selection = vec![false; tab.table.col_count()];
        if let Some((_, col)) = tab.table_state.selected_cell {
            if col < tab.delete_col_selection.len() {
                tab.delete_col_selection[col] = true;
            }
        }
        tab.show_delete_columns_dialog = true;
    }

    /// Sort columns alphabetically by name, ascending or descending.
    #[allow(dead_code)]
    pub(crate) fn sort_columns_alphabetically(&mut self, ascending: bool) {
        let tab = &mut self.tabs[self.active_tab];
        let col_count = tab.table.col_count();
        if col_count <= 1 {
            return;
        }

        let mut order: Vec<usize> = (0..col_count).collect();
        order.sort_by(|&a, &b| {
            let cmp = tab.table.columns[a]
                .name
                .to_lowercase()
                .cmp(&tab.table.columns[b].name.to_lowercase());
            if ascending { cmp } else { cmp.reverse() }
        });

        let old_widths = tab.table_state.col_widths.clone();
        tab.table_state.col_widths = order
            .iter()
            .map(|&orig| old_widths.get(orig).copied().unwrap_or(120.0))
            .collect();

        if let Some((row, col)) = tab.table_state.selected_cell {
            if let Some(new_col) = order.iter().position(|&orig| orig == col) {
                tab.table_state.selected_cell = Some((row, new_col));
            }
        }

        tab.table.reorder_columns(&order);
        tab.filter_dirty = true;
    }
}
