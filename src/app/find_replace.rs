//! Search / filter recomputation and Replace-next / Replace-all.

use octa::data;
use octa::data::search::RowMatcher;

use super::state::OctaApp;

impl OctaApp {
    pub(crate) fn recompute_filter(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        if tab.search_text.is_empty() {
            tab.filtered_rows = (0..tab.table.row_count()).collect();
        } else {
            let matcher = RowMatcher::new(&tab.search_text, tab.search_mode);
            tab.filtered_rows = (0..tab.table.row_count())
                .filter(|&row_idx| {
                    (0..tab.table.col_count()).any(|col_idx| {
                        tab.table
                            .get(row_idx, col_idx)
                            .map(|v| matcher.matches(&v.to_string()))
                            .unwrap_or(false)
                    })
                })
                .collect();
        }
        tab.filter_dirty = false;
        tab.table_state.invalidate_row_heights();
    }

    /// Replace the next matching cell value (starting after the current selection).
    pub(crate) fn replace_next_match(&mut self) {
        let tab = &self.tabs[self.active_tab];
        if tab.search_text.is_empty() {
            return;
        }
        let matcher = RowMatcher::new(&tab.search_text, tab.search_mode);
        let row_count = tab.table.row_count();
        let col_count = tab.table.col_count();
        if row_count == 0 || col_count == 0 {
            return;
        }

        let (start_row, start_col) = match tab.table_state.selected_cell {
            Some((r, c)) => {
                if c + 1 < col_count {
                    (r, c + 1)
                } else if r + 1 < row_count {
                    (r + 1, 0)
                } else {
                    (0, 0) // wrap around
                }
            }
            None => (0, 0),
        };

        let replace_text = tab.replace_text.clone();

        let total_cells = row_count * col_count;
        let start_idx = start_row * col_count + start_col;
        for offset in 0..total_cells {
            let idx = (start_idx + offset) % total_cells;
            let row = idx / col_count;
            let col = idx % col_count;
            if let Some(val) = self.tabs[self.active_tab].table.get(row, col) {
                let text = val.to_string();
                if matcher.matches(&text) {
                    let new_text = matcher.replace(&text, &replace_text);
                    let new_val = data::CellValue::parse_like(val, &new_text);
                    if new_val != *val {
                        self.tabs[self.active_tab].table.set(row, col, new_val);
                    }
                    self.tabs[self.active_tab].table_state.selected_cell = Some((row, col));
                    self.tabs[self.active_tab].table_state.selected_rows.clear();
                    self.tabs[self.active_tab].table_state.selected_cols.clear();
                    self.tabs[self.active_tab].filter_dirty = true;
                    self.status_message = Some((
                        format!("Replaced at row {}, col {}", row + 1, col + 1),
                        std::time::Instant::now(),
                    ));
                    return;
                }
            }
        }
        self.status_message = Some(("No match found".to_string(), std::time::Instant::now()));
    }

    /// Replace all matching cell values.
    pub(crate) fn replace_all_matches(&mut self) {
        let tab = &self.tabs[self.active_tab];
        if tab.search_text.is_empty() {
            return;
        }
        let matcher = RowMatcher::new(&tab.search_text, tab.search_mode);
        let replace_text = tab.replace_text.clone();
        let row_count = tab.table.row_count();
        let col_count = tab.table.col_count();
        let mut count = 0usize;
        for row in 0..row_count {
            for col in 0..col_count {
                if let Some(val) = self.tabs[self.active_tab].table.get(row, col).cloned() {
                    let text = val.to_string();
                    if matcher.matches(&text) {
                        let new_text = matcher.replace(&text, &replace_text);
                        let new_val = data::CellValue::parse_like(&val, &new_text);
                        if new_val != val {
                            self.tabs[self.active_tab].table.set(row, col, new_val);
                            count += 1;
                        }
                    }
                }
            }
        }
        self.tabs[self.active_tab].filter_dirty = true;
        self.status_message = Some((
            format!("Replaced {} cell(s)", count),
            std::time::Instant::now(),
        ));
    }
}
