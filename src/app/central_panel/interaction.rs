//! Table interaction handling: clipboard routing, column rename, type
//! change, sort, context menu and lazy row loading.

use std::sync::{Arc, Mutex};

use eframe::egui;

use octa::data::{self, ViewMode};
use octa::formats;
use octa::ui;
use octa::ui::shortcuts::ShortcutAction as SA;

use crate::app::file_io::load_remaining_parquet_rows;
use crate::app::state::OctaApp;

impl OctaApp {
    /// Route `Event::Copy` / `Event::Cut` / `Event::Paste` and the remappable
    /// `ShortcutAction::Copy/Cut/Paste` triggers to the table-level clipboard
    /// ops - but only when no text is marked and no TextEdit has focus.
    ///
    /// Subtle invariant: egui's TextEdit reads `Event::Paste` etc. without
    /// removing them from `i.events`, AND `draw_table` later in the frame
    /// also has its own paste-event picker. So we ALWAYS drain those events
    /// here (so nothing else fires on them), but only act on them when no
    /// TextEdit is focused. When the SQL editor / search bar / any other
    /// TextEdit is focused, the events have already been consumed by that
    /// editor in an earlier panel and we just throw them away.
    pub(super) fn handle_table_clipboard(&mut self, ctx: &egui::Context) {
        // Marked text owns Ctrl+C, wherever it is: a chat bubble, tool output,
        // a message in a dialog, the focused text box. Stand down entirely -
        // the events stay in the queue for egui's own copy, and `do_copy`
        // never runs, so the cells cannot replace the marked text on the
        // clipboard. Clicking outside the text clears
        // the selection and hands the shortcut straight back to the table.
        if octa::ui::text_selection::has_active_selection(ctx) {
            return;
        }
        if self.tabs[self.active_tab].view_mode != ViewMode::Table {
            return;
        }
        if self.tabs[self.active_tab].table.col_count() == 0 {
            return;
        }

        let mut do_copy = false;
        let mut do_cut = false;
        let mut paste_text: Option<String> = None;
        let mut had_paste_event = false;

        ctx.input_mut(|i| {
            i.events.retain(|e| match e {
                egui::Event::Copy => {
                    do_copy = true;
                    false
                }
                egui::Event::Cut => {
                    do_cut = true;
                    false
                }
                egui::Event::Paste(t) => {
                    paste_text = Some(t.clone());
                    had_paste_event = true;
                    false
                }
                _ => true,
            });
        });

        // While the Shortcuts grid is recording, Ctrl+C / X / V are a binding
        // being typed, not a clipboard command. The events are still drained
        // above so nothing later in the frame acts on them either.
        if octa::ui::shortcuts::capture_mode() {
            return;
        }

        // If any TextEdit holds focus (SQL editor, raw editor, search bar,
        // inline cell editor, dialogs, status-bar nav...), the events above
        // were already handled by that editor when it rendered. Drop them
        // and don't react further on the table side.
        let text_edit_focused = ctx
            .memory(|m| m.focused())
            .and_then(|id| egui::TextEdit::load_state(ctx, id).map(|_| ()))
            .is_some()
            || ctx.egui_wants_keyboard_input();
        if text_edit_focused {
            return;
        }

        // Configurable shortcut path (e.g. user remapped Copy to Ctrl+Shift+C).
        let shortcuts = self.settings.shortcuts.clone();
        if ctx.input(|i| shortcuts.triggered(SA::Copy, i)) {
            do_copy = true;
        }
        if ctx.input(|i| shortcuts.triggered(SA::Cut, i)) {
            do_cut = true;
        }
        if ctx.input(|i| shortcuts.triggered(SA::Paste, i)) && !had_paste_event {
            paste_text = None;
            had_paste_event = true;
        }

        if do_copy {
            self.do_copy(ctx);
        }
        if do_cut {
            self.do_cut(ctx);
        }
        if had_paste_event {
            self.do_paste(ctx, paste_text);
        }
    }

    pub(super) fn handle_table_interaction(
        &mut self,
        interaction: ui::table_view::TableInteraction,
        ctx: &egui::Context,
    ) {
        let tab = &mut self.tabs[self.active_tab];
        if let Some(col_idx) = interaction.header_col_clicked {
            tab.insert_col_at = Some(col_idx + 1);
            if let Some((row, _)) = tab.table_state.selected_cell {
                tab.table_state.selected_cell = Some((row, col_idx));
            }
        }

        if let Some((from, to)) = interaction.col_drag_move {
            tab.table.move_column(from, to);
            if let Some((row, col)) = tab.table_state.selected_cell {
                let new_col = if col == from {
                    to
                } else if from < to {
                    if col > from && col <= to {
                        col - 1
                    } else {
                        col
                    }
                } else if col >= to && col < from {
                    col + 1
                } else {
                    col
                };
                tab.table_state.selected_cell = Some((row, new_col));
            }
            if from < tab.table_state.col_widths.len() && to < tab.table_state.col_widths.len() {
                let w = tab.table_state.col_widths.remove(from);
                tab.table_state.col_widths.insert(to, w);
            }
            tab.filter_dirty = true;
        }

        let tab = &mut self.tabs[self.active_tab];
        if let Some((col_idx, new_name)) = interaction.rename_column
            && col_idx < tab.table.columns.len()
            && !new_name.is_empty()
            && tab.table.columns[col_idx].name != new_name
        {
            // Through `rename_column`, so the rename is undoable and the
            // recipe's undo mark means what it says.
            let old = tab.table.columns[col_idx].name.clone();
            tab.table.rename_column(col_idx, new_name.clone());
            tab.table_state.widths_initialized = false;
            self.record_step(octa::data::recipe::RecipeStep::Rename(
                octa::data::recipe::Rename {
                    renames: vec![octa::data::recipe::RenamePair {
                        from: old,
                        to: new_name,
                    }],
                },
            ));
        }

        if let Some((col_idx, target)) = interaction.change_col_type {
            // Converts on the spot when every value converts, and raises the
            // Change type dialog when one will not, rather than refusing the
            // whole column the way `convert_column` used to.
            self.retype_or_prompt(col_idx, target);
        }

        let tab = &mut self.tabs[self.active_tab];
        let large_sort = tab.large.is_some().then_some(()).and_then(|()| {
            interaction
                .sort_rows_asc_by
                .map(|c| (c, true))
                .or_else(|| interaction.sort_rows_desc_by.map(|c| (c, false)))
        });
        if large_sort.is_none() {
            let sorts = [
                interaction.sort_rows_asc_by.map(|c| (c, true)),
                interaction.sort_rows_desc_by.map(|c| (c, false)),
            ];
            for (col_idx, ascending) in sorts.into_iter().flatten() {
                let tab = &mut self.tabs[self.active_tab];
                tab.table.sort_rows_by_column(col_idx, ascending);
                tab.filter_dirty = true;
                self.record_sort(col_idx, ascending);
            }
        }
        let tab = &mut self.tabs[self.active_tab];

        // --- Context menu: row operations ---
        if interaction.ctx_insert_row {
            let insert_at = match tab.table_state.selected_cell {
                Some((row, _)) => row + 1,
                None => tab.table.row_count(),
            };
            tab.table.insert_row(insert_at);
            let sel_col = tab.table_state.selected_cell.map(|(_, c)| c).unwrap_or(0);
            tab.table_state.selected_cell = Some((insert_at, sel_col));
            tab.table_state.editing_cell = None;
            tab.filter_dirty = true;
        }
        if interaction.ctx_delete_row
            && let Some((row, col)) = tab.table_state.selected_cell
        {
            tab.table.delete_row(row);
            tab.table_state.editing_cell = None;
            if tab.table.row_count() == 0 {
                tab.table_state.selected_cell = None;
            } else {
                let new_row = row.min(tab.table.row_count() - 1);
                tab.table_state.selected_cell = Some((new_row, col));
            }
            tab.filter_dirty = true;
        }
        if interaction.ctx_move_row_up
            && let Some((row, col)) = tab.table_state.selected_cell
            && row > 0
        {
            tab.table.move_row(row, row - 1);
            tab.table_state.selected_cell = Some((row - 1, col));
            tab.filter_dirty = true;
        }
        if interaction.ctx_move_row_down
            && let Some((row, col)) = tab.table_state.selected_cell
            && row + 1 < tab.table.row_count()
        {
            tab.table.move_row(row, row + 1);
            tab.table_state.selected_cell = Some((row + 1, col));
            tab.filter_dirty = true;
        }

        // --- Context menu: column operations ---
        if interaction.ctx_insert_column {
            tab.show_add_column_dialog = true;
            tab.new_col_name.clear();
            tab.new_col_type = "String".to_string();
            tab.new_col_formula.clear();
            tab.insert_col_at = tab.table_state.selected_cell.map(|(_, c)| c + 1);
        }
        if interaction.ctx_delete_column && tab.table.col_count() > 0 {
            self.open_delete_columns_dialog();
        }
        if interaction.ctx_move_col_left {
            let tab = &mut self.tabs[self.active_tab];
            if let Some((row, col)) = tab.table_state.selected_cell
                && col > 0
            {
                tab.table.move_column(col, col - 1);
                tab.table_state.selected_cell = Some((row, col - 1));
                tab.table_state.widths_initialized = false;
            }
        }
        if interaction.ctx_move_col_right {
            let tab = &mut self.tabs[self.active_tab];
            if let Some((row, col)) = tab.table_state.selected_cell
                && col + 1 < tab.table.col_count()
            {
                tab.table.move_column(col, col + 1);
                tab.table_state.selected_cell = Some((row, col + 1));
                tab.table_state.widths_initialized = false;
            }
        }

        // --- Copy / Paste ---
        let tab = &mut self.tabs[self.active_tab];
        if interaction.ctx_copy_cell
            && let Some((row, col)) = tab.table_state.selected_cell
        {
            let text = tab
                .table
                .get(row, col)
                .map(|v| v.to_string())
                .unwrap_or_default();
            ctx.copy_text(text);
        }
        if interaction.ctx_copy {
            self.do_copy(ctx);
        }
        if interaction.ctx_copy_markdown {
            self.do_copy_markdown(ctx);
        }
        if interaction.ctx_copy_in_list {
            self.do_copy_in_list(ctx);
        }
        if interaction.ctx_cut {
            self.do_cut(ctx);
        }
        if interaction.ctx_paste {
            self.do_paste(ctx, interaction.paste_text);
        }
        // --- Add bookmark (cell right-click "Add bookmark...") ---
        // The context menu sets `selected_cell` to the clicked cell, so
        // `begin_add_bookmark` bookmarks that row + column.
        if interaction.ctx_add_bookmark {
            self.begin_add_bookmark();
        }

        // --- Parse in new tab (from cell right-click context menu) ---
        // Resolve scope into modal state *before* taking a `&mut tab`
        // borrow for the mark dispatch below - `build_modal_state`
        // reads the table immutably.
        if let Some(scope) = interaction.ctx_parse_in_new_tab {
            let tab_ref = &self.tabs[self.active_tab];
            self.pending_parse_modal =
                crate::app::dialogs::parse_in_new_tab::build_modal_state(tab_ref, scope);
        }

        // --- Cell history (cell right-click "Cell history...") ---
        if let Some((row, col)) = interaction.ctx_cell_history {
            self.open_cell_history(row, col);
        }

        // --- Facet popup applied or cleared (header funnel) ---
        // Writes exactly what the Column Filter modal writes, so the chip
        // row, the header dot, the filtered row count and every export of
        // the filtered view keep working with no second code path.
        if let Some(result) = interaction.facet_result.clone() {
            let tab = &mut self.tabs[self.active_tab];
            if result.cleared {
                tab.column_filters.remove(&result.col);
            } else {
                tab.column_filters.insert(result.col, result.allowed);
            }
            tab.filter_dirty = true;
        }

        // --- Hide column (right-click) ---
        if let Some(col_idx) = interaction.ctx_hide_column {
            self.tabs[self.active_tab].hidden_columns.insert(col_idx);
        }

        // --- Value frequency (right-click "Value frequency...") ---
        if let Some(col_idx) = interaction.ctx_value_frequency {
            let tab = &mut self.tabs[self.active_tab];
            tab.value_frequency_col = Some(col_idx);
            tab.value_frequency_size = octa::ui::settings::DialogSize::default();
        }

        // --- Number format (right-click "Number format...") ---
        if let Some(col_idx) = interaction.ctx_column_format {
            self.tabs[self.active_tab].open_column_format(col_idx);
        }

        // --- Color marks ---
        let tab = &mut self.tabs[self.active_tab];
        if let Some((keys, color)) = interaction.set_mark {
            for key in keys {
                tab.table.set_mark(key, color);
            }
        }
        if let Some(keys) = interaction.clear_mark {
            for key in keys {
                tab.table.clear_mark(key);
            }
        }

        // --- Large-file mode: the loaded window follows the scroll ---
        let tab = &self.tabs[self.active_tab];
        if tab.large.is_some() {
            // Dragging the (virtual) scrollbar addresses the file directly and
            // outranks the edge triggers below, which only ever move by a page.
            if let Some(row) = interaction.jump_to_row {
                self.large_jump_to_row(row);
                return;
            }
            // Sorting a file that is not in memory means asking for the page
            // again with an ORDER BY, not reordering the rows on screen.
            let step = if interaction.needs_more_rows {
                Some(1)
            } else if tab.table_state.scroll_y() <= 0.0
                && tab.large_page_key.as_ref().is_some_and(|k| k.offset > 0)
            {
                Some(-1)
            } else {
                None
            };
            // The search box is a WHERE clause here, not a row-vector filter.
            let order = large_sort.or_else(|| tab.large_page_key.as_ref().and_then(|k| k.order));
            // The search box becomes a WHERE clause, but only once the typing
            // stops: a `count(*)` per keystroke over a file this size would
            // lock the window.
            let want = crate::app::large_file::search_filter(tab, &tab.search_text.clone());
            let applied = tab
                .large_page_key
                .as_ref()
                .map(|k| k.filter.clone())
                .unwrap_or_default();
            const SETTLE: std::time::Duration = std::time::Duration::from_millis(400);
            let mut filter_ready = want == applied;
            if !filter_ready {
                match &self.large_filter_pending {
                    Some((pending, since)) if *pending == want => {
                        if since.elapsed() >= SETTLE {
                            filter_ready = true;
                        } else {
                            ctx.request_repaint_after(SETTLE);
                        }
                    }
                    _ => {
                        self.large_filter_pending = Some((want.clone(), std::time::Instant::now()));
                        ctx.request_repaint_after(SETTLE);
                    }
                }
            }
            if filter_ready {
                self.large_filter_pending = None;
                self.large_apply_view(order, want);
                if let Some(step) = step {
                    self.large_page_step(step);
                }
            }
            return;
        }

        // A row seam was double-clicked to fit rows to their content while
        // cells could not wrap; fitting means wrapping.
        if interaction.fit_rows_wants_wrap {
            self.enable_cell_line_breaks();
        }

        let tab = &mut self.tabs[self.active_tab];

        // --- Lazy loading: load more rows on demand ---
        if interaction.needs_more_rows
            && tab.bg_can_load_more
            && tab.bg_row_buffer.is_none()
            && tab.table.total_rows.is_some()
        {
            tab.bg_can_load_more = false;
            let buffer = Arc::new(Mutex::new(Vec::<Vec<data::CellValue>>::new()));
            let done_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let exhausted_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
            tab.bg_row_buffer = Some(buffer.clone());
            tab.bg_loading_done = done_flag.clone();
            tab.bg_file_exhausted = exhausted_flag.clone();

            let skip_rows = tab.table.row_offset + tab.table.row_count();
            // Background-load chunk size mirrors the first-load cap so the user's
            // Settings choice applies to both passes consistently.
            let max_chunk = formats::initial_load_rows();

            // A live-database tab has no `source_path`; it pages the server
            // instead, with the same buffer/flag protocol the file readers use.
            if tab.table.source_path.is_none()
                && let Some(origin) = tab.db_origin.clone()
            {
                let page_rows = self.settings.db_page_size();
                let conn = self
                    .settings
                    .db_connections
                    .iter()
                    .find(|c| c.id == origin.conn_id)
                    .cloned();
                if let Some(conn) = conn {
                    let secret =
                        octa::ui::settings::db_secrets::get_db_secret(&conn.id, &self.settings);
                    let ssh_secret =
                        octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &self.settings);
                    let cache = self.db_conn_cache.clone();
                    // Reuse the sidebar's own result channel rather than
                    // adding a second one: it is drained every frame and puts
                    // the message in the status bar. A page that failed
                    // silently would look exactly like the end of the table,
                    // which is the failure this whole path exists to remove.
                    let pending = self.db_browser.pending_open.clone();
                    let failure_label = format!("{} @ {}", origin.table, conn.name);
                    let repaint = ctx.clone();
                    let (load_finished, cancel_slot, cancelled) = self.begin_db_load(format!(
                        "{} {failure_label}",
                        octa::i18n::t("db.loading_more")
                    ));
                    // ponytail: LIMIT/OFFSET paging with no ORDER BY, so a
                    // server free to reorder between pages can repeat or skip
                    // a row. Same ceiling `fetch_batches` and every table copy
                    // already accept; an ORDER BY would force a full sort per
                    // page on exactly the warehouse tables this is for.
                    let sql = octa::db::paged_sql(
                        conn.engine,
                        &octa::db::select_all_sql(
                            conn.engine,
                            origin.catalog.as_deref(),
                            &origin.schema,
                            &origin.table,
                        ),
                        page_rows,
                        skip_rows,
                    );
                    std::thread::spawn(move || {
                        let _done = crate::app::flag_guard::FlagOnDrop::new(done_flag, true);
                        let _load = crate::app::flag_guard::FlagOnDrop::new(load_finished, true);
                        let read =
                            cache.with_conn(&conn, secret.as_deref(), ssh_secret.as_deref(), |c| {
                                // Same reason as the sidebar open: `with_conn`
                                // retries once, and after a Cancel that retry
                                // would re-run the statement just stopped.
                                if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                                    anyhow::bail!("{}", octa::i18n::t("db.load_cancelled"));
                                }
                                if let Ok(mut slot) = cancel_slot.lock() {
                                    *slot = c.cancel_handle();
                                }
                                c.query(&sql)
                            });
                        match read {
                            Ok(t) => {
                                // A short page is the end of the table.
                                if t.rows.len() < page_rows {
                                    exhausted_flag
                                        .store(true, std::sync::atomic::Ordering::Relaxed);
                                }
                                if let Ok(mut buf) = buffer.lock() {
                                    buf.extend(t.rows);
                                }
                            }
                            Err(e) => {
                                // Deliberately NOT `exhausted`: this page
                                // failed or was cancelled, which says nothing
                                // about whether the server still holds rows.
                                // Claiming otherwise would retire the tab's
                                // "+" and report a partial table as complete.
                                // `bg_can_load_more` is already false, so
                                // nothing retries on its own; reopen the table
                                // to try again.
                                if let Ok(mut p) = pending.lock() {
                                    p.push(crate::app::db_browser::DbOpenResult::Failed(format!(
                                        "{} {failure_label}: {e:#}",
                                        octa::i18n::t("db.page_failed")
                                    )));
                                }
                                repaint.request_repaint();
                            }
                        }
                    });
                } else {
                    // The connection was deleted while the tab was open.
                    exhausted_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    done_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            } else if let Some(ref source_path) = tab.table.source_path.clone() {
                let path = std::path::PathBuf::from(source_path);
                let format_name = tab.table.format_name.clone().unwrap_or_default();
                let num_cols = tab.table.col_count();
                let csv_delimiter = tab.csv_delimiter;

                if format_name == "Parquet" {
                    std::thread::spawn(move || {
                        // Bad bytes at row two million, or a USB stick pulled
                        // mid-scroll, must not leave the app repainting every
                        // frame with a spinner that never stops.
                        let _done =
                            crate::app::flag_guard::FlagOnDrop::new(done_flag.clone(), true);
                        if let Err(e) = load_remaining_parquet_rows(
                            &path,
                            skip_rows,
                            max_chunk,
                            buffer.clone(),
                            done_flag,
                            exhausted_flag,
                        ) {
                            eprintln!("Background loading error: {}", e);
                        }
                    });
                } else if format_name == "CSV" || format_name == "TSV" {
                    let delimiter = if format_name == "TSV" {
                        b'\t'
                    } else {
                        csv_delimiter
                    };
                    std::thread::spawn(move || {
                        let _done =
                            crate::app::flag_guard::FlagOnDrop::new(done_flag.clone(), true);
                        if let Err(e) = formats::csv_reader::load_csv_rows_chunk(
                            &path,
                            delimiter,
                            formats::csv_reader::ChunkSpec {
                                skip_rows,
                                max_rows: max_chunk,
                                num_cols,
                            },
                            formats::csv_reader::ChunkSink {
                                buffer,
                                done: done_flag,
                                exhausted: exhausted_flag,
                            },
                        ) {
                            eprintln!("Background CSV loading error: {}", e);
                        }
                    });
                }
            }
        }
    }
}
