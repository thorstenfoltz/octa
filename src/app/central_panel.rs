//! Central panel: status banner, view-mode dispatch (PDF/Notebook/Markdown/
//! Raw/JsonTree), the table renderer, and the table interaction handling
//! (column rename, type change, sort, context menu, lazy row loading).

use std::sync::{Arc, Mutex};

use eframe::egui;

use octa::data::{self, ViewMode};
use octa::formats;
use octa::ui;
use octa::ui::shortcuts::ShortcutAction as SA;

use super::file_io::load_remaining_parquet_rows;
use super::state::OctaApp;
use crate::view_modes;

impl OctaApp {
    pub(crate) fn render_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Status message — auto-fades after 10s.
            if let Some((ref msg, instant)) = self.status_message {
                if instant.elapsed().as_secs() < 10 {
                    let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
                    let color = if msg.starts_with("Saved") {
                        colors.success
                    } else {
                        colors.error
                    };
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(msg).color(color).size(12.0));
                    });
                    ui.add_space(4.0);
                }
            }

            // Recompute filter before drawing (toolbar actions earlier in the
            // frame may have dirtied it).
            if self.tabs[self.active_tab].filter_dirty {
                self.recompute_filter();
            }

            // Non-table view modes render and return early.
            if self.tabs[self.active_tab].view_mode == ViewMode::Pdf {
                view_modes::render_pdf_view(
                    ctx,
                    ui,
                    &mut self.tabs[self.active_tab],
                    self.theme_mode,
                );
                return;
            }
            if self.tabs[self.active_tab].view_mode == ViewMode::Notebook {
                view_modes::render_notebook_view(
                    ctx,
                    ui,
                    &self.tabs[self.active_tab],
                    self.theme_mode,
                    self.settings.notebook_output_layout,
                );
                return;
            }
            if self.tabs[self.active_tab].view_mode == ViewMode::Markdown {
                view_modes::render_markdown_view(ui, &mut self.tabs[self.active_tab]);
                return;
            }
            if self.tabs[self.active_tab].view_mode == ViewMode::Raw {
                let raw_action = view_modes::render_raw_view(
                    ui,
                    &mut self.tabs[self.active_tab],
                    self.theme_mode,
                    self.settings.color_aligned_columns,
                    self.settings.tab_size,
                    self.settings.warn_raw_align_reload,
                );
                if raw_action.confirm_unalign {
                    self.show_unalign_confirm = true;
                }
                return;
            }
            if self.tabs[self.active_tab].view_mode == ViewMode::JsonTree {
                view_modes::render_json_tree_view(
                    ui,
                    &mut self.tabs[self.active_tab],
                    self.theme_mode,
                );
                return;
            }

            // --- Table view ---
            // Drain pending Copy/Cut/Paste events (and remappable
            // ShortcutAction triggers) here, AFTER all earlier panels (SQL
            // editor, toolbar search, status bar nav, etc.) have had a chance
            // to consume them. This keeps clipboard interactions in TextEdits
            // local to those editors and only routes the leftover events to
            // the table.
            self.handle_table_clipboard(ctx);

            let os_has_clipboard = self.os_clipboard_has_text();
            let tab = &mut self.tabs[self.active_tab];
            let filtered = tab.filtered_rows.clone();
            let os_has_clip = tab.table_state.clipboard.is_some() || os_has_clipboard;
            let interaction = ui::table_view::draw_table(
                ui,
                &mut tab.table,
                &mut tab.table_state,
                self.theme_mode,
                &filtered,
                os_has_clip,
                self.settings.show_row_numbers,
                self.settings.alternating_row_colors,
                self.settings.negative_numbers_red,
                self.settings.highlight_edits,
                self.settings.font_size * self.zoom_percent as f32 / 100.0,
                self.settings.cell_line_breaks,
                self.settings.binary_display_mode,
                self.welcome_logo_texture.as_ref(),
                &self.settings.shortcuts,
            );

            self.handle_table_interaction(interaction);
        });
    }

    /// Route `Event::Copy` / `Event::Cut` / `Event::Paste` and the remappable
    /// `ShortcutAction::Copy/Cut/Paste` triggers to the table-level clipboard
    /// ops — but only when no TextEdit has keyboard focus.
    ///
    /// Subtle invariant: egui's TextEdit reads `Event::Paste` etc. without
    /// removing them from `i.events`, AND `draw_table` later in the frame
    /// also has its own paste-event picker. So we ALWAYS drain those events
    /// here (so nothing else fires on them), but only act on them when no
    /// TextEdit is focused. When the SQL editor / search bar / any other
    /// TextEdit is focused, the events have already been consumed by that
    /// editor in an earlier panel and we just throw them away.
    fn handle_table_clipboard(&mut self, ctx: &egui::Context) {
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

        // If any TextEdit holds focus (SQL editor, raw editor, search bar,
        // inline cell editor, dialogs, status-bar nav...), the events above
        // were already handled by that editor when it rendered. Drop them
        // and don't react further on the table side.
        let text_edit_focused = ctx
            .memory(|m| m.focused())
            .and_then(|id| egui::TextEdit::load_state(ctx, id).map(|_| ()))
            .is_some()
            || ctx.wants_keyboard_input();
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
            self.do_copy();
        }
        if do_cut {
            self.do_cut();
        }
        if had_paste_event {
            self.do_paste(paste_text);
        }
    }

    fn handle_table_interaction(&mut self, interaction: ui::table_view::TableInteraction) {
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
        if let Some((col_idx, new_name)) = interaction.rename_column {
            if col_idx < tab.table.columns.len() && !new_name.is_empty() {
                tab.table.columns[col_idx].name = new_name;
                tab.table.structural_changes = true;
                tab.table_state.widths_initialized = false;
            }
        }

        if let Some((col_idx, new_type)) = interaction.change_col_type {
            if !tab.table.convert_column(col_idx, &new_type) {
                self.status_message = Some((
                    format!("Cannot convert column to {new_type}: some values are incompatible"),
                    std::time::Instant::now(),
                ));
            }
        }

        let tab = &mut self.tabs[self.active_tab];
        if let Some(col_idx) = interaction.sort_rows_asc_by {
            tab.table.sort_rows_by_column(col_idx, true);
            tab.filter_dirty = true;
        }
        if let Some(col_idx) = interaction.sort_rows_desc_by {
            tab.table.sort_rows_by_column(col_idx, false);
            tab.filter_dirty = true;
        }

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
        if interaction.ctx_delete_row {
            if let Some((row, col)) = tab.table_state.selected_cell {
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
        }
        if interaction.ctx_move_row_up {
            if let Some((row, col)) = tab.table_state.selected_cell {
                if row > 0 {
                    tab.table.move_row(row, row - 1);
                    tab.table_state.selected_cell = Some((row - 1, col));
                    tab.filter_dirty = true;
                }
            }
        }
        if interaction.ctx_move_row_down {
            if let Some((row, col)) = tab.table_state.selected_cell {
                if row + 1 < tab.table.row_count() {
                    tab.table.move_row(row, row + 1);
                    tab.table_state.selected_cell = Some((row + 1, col));
                    tab.filter_dirty = true;
                }
            }
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
            if let Some((row, col)) = tab.table_state.selected_cell {
                if col > 0 {
                    tab.table.move_column(col, col - 1);
                    tab.table_state.selected_cell = Some((row, col - 1));
                    tab.table_state.widths_initialized = false;
                }
            }
        }
        if interaction.ctx_move_col_right {
            let tab = &mut self.tabs[self.active_tab];
            if let Some((row, col)) = tab.table_state.selected_cell {
                if col + 1 < tab.table.col_count() {
                    tab.table.move_column(col, col + 1);
                    tab.table_state.selected_cell = Some((row, col + 1));
                    tab.table_state.widths_initialized = false;
                }
            }
        }

        // --- Copy / Paste ---
        let tab = &mut self.tabs[self.active_tab];
        if interaction.ctx_copy_cell {
            if let Some((row, col)) = tab.table_state.selected_cell {
                let text = tab
                    .table
                    .get(row, col)
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                tab.table_state.clipboard = Some(text.clone());
                if let Some(ref cb) = self.os_clipboard {
                    if let Ok(mut cb) = cb.lock() {
                        let _ = cb.set_text(&text);
                    }
                }
            }
        }
        if interaction.ctx_copy {
            self.do_copy();
        }
        if interaction.ctx_cut {
            self.do_cut();
        }
        if interaction.ctx_paste {
            self.do_paste(interaction.paste_text);
        }

        // --- Undo / Redo ---
        let tab = &mut self.tabs[self.active_tab];
        if interaction.undo {
            tab.table.undo();
            tab.filter_dirty = true;
            tab.table_state.widths_initialized = false;
        }
        if interaction.redo {
            tab.table.redo();
            tab.filter_dirty = true;
            tab.table_state.widths_initialized = false;
        }

        // --- Color marks ---
        if let Some((key, color)) = interaction.set_mark {
            tab.table.set_mark(key, color);
        }
        if let Some(key) = interaction.clear_mark {
            tab.table.clear_mark(key);
        }

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
            let max_chunk = 1_000_000usize;

            if let Some(ref source_path) = tab.table.source_path.clone() {
                let path = std::path::PathBuf::from(source_path);
                let format_name = tab.table.format_name.clone().unwrap_or_default();
                let num_cols = tab.table.col_count();
                let csv_delimiter = tab.csv_delimiter;

                if format_name == "Parquet" {
                    std::thread::spawn(move || {
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
                        if let Err(e) = formats::csv_reader::load_csv_rows_chunk(
                            &path,
                            delimiter,
                            skip_rows,
                            max_chunk,
                            num_cols,
                            buffer,
                            done_flag,
                            exhausted_flag,
                        ) {
                            eprintln!("Background CSV loading error: {}", e);
                        }
                    });
                }
            }
        }
    }
}
