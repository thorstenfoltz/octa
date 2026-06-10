//! Search / filter recomputation and Replace-next / Replace-all.

use octa::data;
use octa::data::search::RowMatcher;

use super::state::OctaApp;

impl OctaApp {
    /// Open the Column Filter dialog for the active tab, optionally
    /// preselecting a column. Seeds the draft set from any existing applied
    /// filter on the picked column so the dialog opens with the current
    /// checkbox state, not all-unchecked.
    pub(crate) fn open_column_filter_dialog(&mut self, preselect: Option<usize>) {
        let tab = &mut self.tabs[self.active_tab];
        if tab.table.col_count() == 0 {
            return;
        }
        let col = preselect
            .filter(|&c| c < tab.table.col_count())
            .or(tab.column_filter_picker_col)
            .filter(|&c| c < tab.table.col_count())
            .unwrap_or(0);
        tab.column_filter_picker_col = Some(col);
        tab.column_filter_value_search.clear();
        // Seed the draft with the saved set. If nothing is saved, leave it
        // empty and arm the one-shot seed flag so the dialog's first render
        // populates "all values" exactly once. Without the explicit flag, an
        // empty draft is indistinguishable from a user-cleared "Select none"
        // state and we'd re-seed every frame.
        match tab.column_filters.get(&col) {
            Some(set) => {
                tab.column_filter_draft_allowed = set.clone();
                tab.column_filter_needs_seed = false;
            }
            None => {
                tab.column_filter_draft_allowed.clear();
                tab.column_filter_needs_seed = true;
            }
        }
        tab.show_column_filter = true;
    }

    pub(crate) fn recompute_filter(&mut self) {
        let mode = self.search_result_mode;
        let tab = &mut self.tabs[self.active_tab];
        let matcher = (!tab.search_text.is_empty())
            .then(|| RowMatcher::new(&tab.search_text, tab.search_mode));
        let has_column_filters = !tab.column_filters.is_empty();
        // In highlight mode the text search no longer hides rows; it only paints
        // matches. Excel-style column filters still hide rows in both modes.
        let highlight = super::state::effective_highlight(tab.view_mode, mode);
        let text_hides_rows = matcher.is_some() && !highlight;

        if !text_hides_rows && !has_column_filters {
            tab.filtered_rows = (0..tab.table.row_count()).collect();
        } else {
            tab.filtered_rows = (0..tab.table.row_count())
                .filter(|&row_idx| {
                    // 1. Text search (filter mode only): any cell must match.
                    let text_ok = !text_hides_rows
                        || matcher.as_ref().is_none_or(|m| {
                            (0..tab.table.col_count()).any(|col_idx| {
                                tab.table
                                    .get(row_idx, col_idx)
                                    .map(|v| m.matches(&v.to_string()))
                                    .unwrap_or(false)
                            })
                        });
                    if !text_ok {
                        return false;
                    }
                    // 2. Excel-style column filters: every filtered column's
                    //    cell must appear in its allow-set. Filters AND with
                    //    each other and with the text search above.
                    tab.column_filters.iter().all(|(&col, allowed)| {
                        tab.table
                            .get(row_idx, col)
                            .map(|v| allowed.contains(&v.to_string()))
                            .unwrap_or(false)
                    })
                })
                .collect();
        }

        // Highlight matches are a table-view concern; text/tree views compute
        // their own match count each frame, so only touch the nav bookkeeping
        // when the table is the active view.
        tab.search_cell_matches.clear();
        if tab.view_mode == octa::data::ViewMode::Table {
            if highlight && let Some(m) = matcher.as_ref() {
                tab.search_cell_matches = octa::ui::search_highlight::cell_matches(
                    &tab.table,
                    m,
                    &tab.filtered_rows,
                    tab.table.col_count(),
                );
            }
            tab.search_nav.match_count = tab.search_cell_matches.len();
            if tab.search_nav.current >= tab.search_nav.match_count {
                tab.search_nav.current = 0;
            }
        }

        tab.filter_dirty = false;
        tab.table_state.invalidate_row_heights();
    }

    /// Consume a pending highlight-search jump for the table view: advance the
    /// current match (wrapping), select that cell and scroll it into view.
    /// No-op when there is no pending jump or no matches. Mirrors the cell-jump
    /// path used by the status-bar navigation box.
    pub(crate) fn apply_table_search_jump(&mut self) {
        let row_height =
            (self.settings.font_size * self.zoom_percent as f32 / 100.0 * 2.0).max(26.0);
        let tab = &mut self.tabs[self.active_tab];
        let Some(dir) = tab.search_nav.pending_jump.take() else {
            return;
        };
        let count = tab.search_cell_matches.len();
        if count == 0 {
            return;
        }
        tab.search_nav.current = match dir {
            super::state::NavDir::Next => (tab.search_nav.current + 1) % count,
            super::state::NavDir::Prev => (tab.search_nav.current + count - 1) % count,
        };
        let (row, col) = tab.search_cell_matches[tab.search_nav.current];
        tab.table_state.selected_cell = Some((row, col));
        tab.table_state.selected_rows.clear();
        tab.table_state.selected_cols.clear();
        tab.table_state.set_scroll_y(row as f32 * row_height);
        let col_left: f32 = tab.table_state.col_widths[..col].iter().sum();
        tab.table_state.set_scroll_x(col_left);
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
