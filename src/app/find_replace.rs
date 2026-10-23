//! Search / filter recomputation and the Find & Replace box: its drawing,
//! Replace-next and Replace-all.

use eframe::egui;
use octa::data;

use super::state::{OctaApp, TabState};
use crate::view_modes::find_box;

/// The cells the Find & Replace box finds, in display order: the matches in
/// the rows on screen (the filtered set), in every column but the hidden
/// ones, which the user cannot see change.
fn box_cells(tab: &TabState, matcher: &data::search::RowMatcher) -> Vec<(usize, usize)> {
    octa::ui::search_highlight::cell_matches(
        &tab.table,
        matcher,
        &tab.filtered_rows,
        tab.table.col_count(),
        None,
    )
    .into_iter()
    .filter(|(_, c)| !tab.hidden_columns.contains(c))
    .collect()
}

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
        tab.column_filter_shape_draft = None;
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
        // A large-file tab filters in SQL, against the file, not with a row
        // vector: `filtered_rows` over forty million rows is the allocation the
        // mode exists to avoid. It still needs one row per row of the *page*,
        // though - that is what the view renders from, and clearing it here is
        // why a large tab used to show its column headers over nothing.
        if self.tabs[self.active_tab].large.is_some() {
            let tab = &mut self.tabs[self.active_tab];
            tab.filtered_rows = (0..tab.table.row_count()).collect();
            tab.filter_dirty = false;
            tab.search_cell_matches.clear();
            tab.validation_violations.clear();
            return;
        }
        // A tab sorted and filtered on the server already holds only the rows
        // its filters keep. Filtering them again here would only add the
        // differences between the two matchers, so only the parts the server
        // could not apply exactly still filter.
        let server_left = {
            let tab = &self.tabs[self.active_tab];
            super::db_view::server_leftovers(
                tab,
                self.settings.db_pushdown,
                &self.settings.db_connections,
                self.search_result_mode,
            )
        };
        let mode = self.search_result_mode;
        // Build the matcher under an immutable borrow that ends before the
        // mutable one below (the matcher is owned, so no borrow lingers).
        let matcher = {
            let tab = &self.tabs[self.active_tab];
            (!tab.search_text.is_empty()).then(|| tab.search_matcher())
        };
        // The Find & Replace box highlights its own text and never filters.
        let box_matcher = self.tabs[self.active_tab]
            .find_box_active()
            .map(find_box::matcher);
        let mark_mode = self.settings.mark_filter_cell_mode;
        let tab = &mut self.tabs[self.active_tab];
        let has_column_filters = server_left.is_none() && !tab.column_filters.is_empty();
        let local_predicates: Vec<octa::data::predicate_filter::PredicateFilter> =
            match &server_left {
                None => tab.predicate_filters.clone(),
                Some(left) => left
                    .predicates
                    .iter()
                    .filter_map(|&i| tab.predicate_filters.get(i).cloned())
                    .collect(),
            };
        let has_predicates = !local_predicates.is_empty();
        // "Filter to marked": when active, keep only marked rows (union with
        // cell-derived rows per the mode). An empty row set means the marks
        // constrain columns only, so all rows are kept. ANDs with the text /
        // column filters below, consistent with every other filter.
        let mark_keep = tab
            .mark_filter_active
            .then(|| octa::data::mark_filter::mark_keep_set(&tab.table.marks, mark_mode));
        let mark_hides_rows = mark_keep.as_ref().is_some_and(|k| !k.rows.is_empty());
        // In highlight mode the text search no longer hides rows; it only paints
        // matches. Excel-style column filters still hide rows in both modes.
        let highlight = super::state::effective_highlight(tab.view_mode, mode);
        let text_hides_rows =
            matcher.is_some() && !highlight && server_left.as_ref().is_none_or(|l| l.search);
        // Column range the text search scans (one column, or all).
        let col_count = tab.table.col_count();
        let (scope_lo, scope_hi) = match tab.search_scope_col {
            Some(c) if c < col_count => (c, c + 1),
            _ => (0, col_count),
        };

        if !text_hides_rows && !has_column_filters && !has_predicates && !mark_hides_rows {
            tab.filtered_rows = (0..tab.table.row_count()).collect();
        } else {
            tab.filtered_rows = (0..tab.table.row_count())
                .filter(|&row_idx| {
                    // 0. Filter to marked (row set): keep only marked rows when
                    //    the mark set constrains rows.
                    if let Some(k) = mark_keep.as_ref()
                        && !k.rows.is_empty()
                        && !k.rows.contains(&row_idx)
                    {
                        return false;
                    }
                    // 1. Text search (filter mode only): any in-scope cell must match.
                    let text_ok = !text_hides_rows
                        || matcher.as_ref().is_none_or(|m| {
                            (scope_lo..scope_hi).any(|col_idx| {
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
                    let values_ok = !has_column_filters
                        || tab.column_filters.iter().all(|(&col, allowed)| {
                            tab.table
                                .get(row_idx, col)
                                .map(|v| allowed.contains(&v.to_string()))
                                .unwrap_or(false)
                        });
                    if !values_ok {
                        return false;
                    }
                    // 3. Comparison filters (`amount > 1000`), which a value
                    //    allow-set cannot express. Also ANDed.
                    octa::data::predicate_filter::row_passes(&local_predicates, &tab.table, row_idx)
                })
                .collect();
        }

        // 4. Duplicate filter: keep only the rows that repeat on the key
        //    columns, or only those that do not. ANDs with everything above,
        //    and runs last because it is the only step that has to look at the
        //    whole table rather than at one row.
        if let Some(df) = tab.duplicate_filter.clone() {
            let stamp: super::state::DupStamp = (
                tab.table.row_count(),
                tab.table.edits.len(),
                tab.table.undo_stack.len(),
            );
            if tab
                .duplicate_filter_cache
                .as_ref()
                .is_none_or(|(cached, _)| *cached != stamp)
            {
                // ponytail: a full key-hash pass over every row whenever the
                // stamp moves. Incremental maintenance if a very large table
                // ever makes the pause visible.
                let set: std::collections::HashSet<usize> =
                    octa::data::duplicates::find_duplicate_rows(&tab.table, &df.key_cols)
                        .into_iter()
                        .collect();
                tab.duplicate_filter_cache = Some((stamp, set));
            }
            if let Some((_, set)) = tab.duplicate_filter_cache.as_ref() {
                tab.filtered_rows
                    .retain(|row_idx| set.contains(row_idx) == df.keep_duplicates);
            }
        }

        // Highlight matches are a table-view concern; text/tree views compute
        // their own match count each frame, so only touch the nav bookkeeping
        // when the table is the active view.
        tab.search_cell_matches.clear();
        if tab.view_mode == octa::data::ViewMode::Table {
            if let Some(m) = box_matcher.as_ref() {
                tab.search_cell_matches = box_cells(tab, m);
            } else if highlight && let Some(m) = matcher.as_ref() {
                tab.search_cell_matches = octa::ui::search_highlight::cell_matches(
                    &tab.table,
                    m,
                    &tab.filtered_rows,
                    tab.table.col_count(),
                    tab.search_scope_col,
                );
            }
            tab.search_nav.match_count = tab.search_cell_matches.len();
            if tab.search_nav.current >= tab.search_nav.match_count {
                tab.search_nav.current = 0;
            }
        }

        // Validation: cache the cells failing a rule so the renderer can paint
        // them without re-scanning the table every frame. Cheap no-op when no
        // rules are set.
        tab.validation_violations = if tab.validation_rules.is_empty() {
            std::collections::HashSet::new()
        } else {
            octa::data::validation::violations(&tab.table, &tab.validation_rules)
        };

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

    /// The Find & Replace box's text and replacement, while it is active.
    fn box_find(&self) -> Option<(data::search::RowMatcher, String)> {
        let f = self.tabs[self.active_tab].find_box_active()?;
        Some((find_box::matcher(f), f.replace.clone()))
    }

    /// Replace next: the selected match (or the first one after the selection),
    /// then select the match after it. A text view replaces in its text.
    pub(crate) fn replace_next_match(&mut self) {
        if self.is_readonly() {
            return;
        }
        let Some((matcher, replace_text)) = self.box_find() else {
            return;
        };
        if self.tabs[self.active_tab].shows_text() {
            self.replace_in_text(&matcher, &replace_text, false);
            return;
        }
        let tab = &mut self.tabs[self.active_tab];
        let cells = box_cells(tab, &matcher);
        let display: std::collections::HashMap<usize, usize> = tab
            .filtered_rows
            .iter()
            .enumerate()
            .map(|(i, &r)| (r, i))
            .collect();
        let order = |(r, c): (usize, usize)| (display.get(&r).copied().unwrap_or(usize::MAX), c);
        let Some((target, then)) =
            data::search::replace_next_target(&cells, tab.table_state.selected_cell, order)
        else {
            self.status_message = Some((
                octa::i18n::t("search.no_matches"),
                std::time::Instant::now(),
            ));
            return;
        };
        data::search::replace_in_cells(&mut tab.table, &[target], &matcher, &replace_text);
        let show = then.unwrap_or(target);
        tab.table_state.selected_cell = Some(show);
        tab.table_state.selected_rows.clear();
        tab.table_state.selected_cols.clear();
        if let Some(&i) = display.get(&show.0) {
            tab.table_state.scroll_to_row(i);
        }
        tab.filter_dirty = true;
        self.status_message = Some((
            octa::i18n::t("search.replaced_at")
                .replace("{row}", &(target.0 + 1).to_string())
                .replace("{col}", &(target.1 + 1).to_string()),
            std::time::Instant::now(),
        ));
    }

    /// Replace every match in the cells on screen, as one undo step. A text
    /// view replaces in its text.
    pub(crate) fn replace_all_matches(&mut self) {
        if self.is_readonly() {
            return;
        }
        let Some((matcher, replace_text)) = self.box_find() else {
            return;
        };
        if self.tabs[self.active_tab].shows_text() {
            self.replace_in_text(&matcher, &replace_text, true);
            return;
        }
        let tab = &mut self.tabs[self.active_tab];
        let cells = box_cells(tab, &matcher);
        let undo_len = tab.table.undo_stack.len();
        let count = data::search::replace_in_cells(&mut tab.table, &cells, &matcher, &replace_text);
        tab.table.coalesce_undo_since(undo_len);
        tab.filter_dirty = true;
        self.status_message = Some((
            octa::i18n::t("search.replaced_cells").replace("{n}", &count.to_string()),
            std::time::Instant::now(),
        ));
    }

    /// Replace in the text a text view shows (Raw, Markdown, the trees, the
    /// text diff): that text is what the user sees and what Save writes, so
    /// replacing in the table behind it changed nothing visible. `all` replaces
    /// every match, otherwise the current one, which the next match then takes
    /// the place of in the count.
    fn replace_in_text(
        &mut self,
        matcher: &data::search::RowMatcher,
        replace_text: &str,
        all: bool,
    ) {
        let tab = &mut self.tabs[self.active_tab];
        let Some(text) = tab.raw_content.clone() else {
            return;
        };
        let ranges = matcher.find_ranges(&text);
        if ranges.is_empty() {
            self.status_message = Some((
                octa::i18n::t("search.no_matches"),
                std::time::Instant::now(),
            ));
            return;
        }
        let (new_text, count) = if all {
            (matcher.replace_all(&text, replace_text), ranges.len())
        } else {
            let at = tab.search_nav.current.min(ranges.len() - 1);
            (
                matcher.replace_range(&text, ranges[at].clone(), replace_text),
                1,
            )
        };
        if new_text == text {
            return;
        }
        tab.raw_content = Some(new_text);
        // A tree shows the parsed text: it has to read back, or nothing changes.
        if let Err(e) = tab.refresh_tree_from_text() {
            tab.raw_content = Some(text);
            self.status_message = Some((
                octa::i18n::t("search.replace_breaks_text").replace("{error}", &e),
                std::time::Instant::now(),
            ));
            return;
        }
        tab.raw_content_modified = true;
        tab.markdown_render_cache = None;
        self.status_message = Some((
            octa::i18n::t("search.replaced_text").replace("{n}", &count.to_string()),
            std::time::Instant::now(),
        ));
    }
}

impl OctaApp {
    /// Open the Find & Replace box on the active tab, replace row showing and
    /// the keyboard in its find field (Ctrl+H, Search -> Find & Replace).
    pub(crate) fn open_find_box(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let f = tab.find_box.get_or_insert_with(Default::default);
        f.replacing = true;
        f.focus = true;
        tab.filter_dirty = true;
    }

    /// Close the box; the toolbar search's highlight comes back.
    pub(crate) fn close_find_box(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        tab.find_box = None;
        tab.filter_dirty = true;
    }

    /// Draw the box over the view just drawn, when it is open and the view
    /// shows what it searches. `corner` is the view's top right.
    pub(crate) fn draw_find_box(&mut self, ui: &mut egui::Ui, corner: egui::Pos2) {
        use super::state::NavDir;
        use octa::i18n::t;

        let readonly = self.is_readonly();
        let tab = &mut self.tabs[self.active_tab];
        if !tab.find_box_fits() {
            return;
        }
        let Some(mut input) = tab.find_box.take() else {
            return;
        };
        let nav = if input.query.is_empty() {
            (0, 0)
        } else {
            (tab.search_nav.match_count, tab.search_nav.current)
        };
        let block = readonly.then(|| t("context_menu.header_row_readonly"));
        let texts = find_box::FindBoxTexts {
            placeholder: t("search_menu.find"),
            find_hint: t("search.find_box_hint"),
            replace_all_hint: t("search.replace_all_hint"),
        };
        let out = find_box::draw(
            ui,
            corner,
            egui::Id::new("tab_find_box"),
            &mut input,
            nav,
            block.as_deref(),
            &texts,
        );
        if out.changed {
            tab.search_nav.current = 0;
            tab.filter_dirty = true;
        }
        if out.prev {
            tab.search_nav.pending_jump = Some(NavDir::Prev);
        } else if out.next {
            tab.search_nav.pending_jump = Some(NavDir::Next);
        }
        if out.close {
            self.close_find_box();
            return;
        }
        tab.find_box = Some(input);
        if out.replace_one {
            self.replace_next_match();
        }
        if out.replace_all {
            self.replace_all_matches();
        }
    }
}

#[cfg(test)]
#[path = "find_replace_tests.rs"]
mod tests;
