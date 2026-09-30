//! Clipboard plumbing: copy current selection to tab-separated text, paste
//! tab-separated text back, and bridge to the OS clipboard.

use eframe::egui;

use octa::data;
use octa::ui;

use super::state::OctaApp;

impl OctaApp {
    /// Build a tab-separated string from the current selection.
    /// Priority: selected_rows > selected_cols > selected_cells > selected_cell.
    pub(crate) fn copy_selection_to_string(&self) -> Option<String> {
        let tab = &self.tabs[self.active_tab];
        let state = &tab.table_state;

        if !state.selected_rows.is_empty() {
            let mut rows: Vec<usize> = state.selected_rows.iter().copied().collect();
            rows.sort();
            let mut lines = Vec::new();
            for row in rows {
                let mut cells = Vec::new();
                for col in 0..tab.table.col_count() {
                    let text = tab
                        .table
                        .get(row, col)
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    cells.push(text);
                }
                lines.push(cells.join("\t"));
            }
            Some(lines.join("\n"))
        } else if !state.selected_cols.is_empty() {
            let mut cols: Vec<usize> = state.selected_cols.iter().copied().collect();
            cols.sort();
            let mut lines = Vec::new();
            for row in 0..tab.table.row_count() {
                let mut cells = Vec::new();
                for &col in &cols {
                    let text = tab
                        .table
                        .get(row, col)
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    cells.push(text);
                }
                lines.push(cells.join("\t"));
            }
            Some(lines.join("\n"))
        } else if !state.selected_cells.is_empty() {
            let cells: Vec<(usize, usize)> = state.selected_cells.iter().copied().collect();
            let min_row = cells.iter().map(|(r, _)| *r).min().unwrap();
            let max_row = cells.iter().map(|(r, _)| *r).max().unwrap();
            let min_col = cells.iter().map(|(_, c)| *c).min().unwrap();
            let max_col = cells.iter().map(|(_, c)| *c).max().unwrap();
            let mut lines = Vec::new();
            for row in min_row..=max_row {
                let mut row_cells = Vec::new();
                for col in min_col..=max_col {
                    let text = if state.selected_cells.contains(&(row, col)) {
                        tab.table
                            .get(row, col)
                            .map(|v| v.to_string())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    row_cells.push(text);
                }
                lines.push(row_cells.join("\t"));
            }
            Some(lines.join("\n"))
        } else if let Some((row, col)) = state.selected_cell {
            let text = tab
                .table
                .get(row, col)
                .map(|v| v.to_string())
                .unwrap_or_default();
            Some(text)
        } else {
            None
        }
    }

    /// Build a GitHub-flavoured Markdown table from the current selection,
    /// including the column header row. Same selection precedence as
    /// [`copy_selection_to_string`](Self::copy_selection_to_string). Returns
    /// `None` when nothing is selected.
    pub(crate) fn copy_selection_as_markdown(&self) -> Option<String> {
        let tab = &self.tabs[self.active_tab];
        let state = &tab.table_state;
        let table = &tab.table;

        // Resolve which columns and rows the selection covers.
        let (cols, rows): (Vec<usize>, Vec<usize>) = if !state.selected_rows.is_empty() {
            let mut rows: Vec<usize> = state.selected_rows.iter().copied().collect();
            rows.sort();
            ((0..table.col_count()).collect(), rows)
        } else if !state.selected_cols.is_empty() {
            let mut cols: Vec<usize> = state.selected_cols.iter().copied().collect();
            cols.sort();
            (cols, (0..table.row_count()).collect())
        } else if !state.selected_cells.is_empty() {
            let cells = &state.selected_cells;
            let min_row = cells.iter().map(|(r, _)| *r).min().unwrap();
            let max_row = cells.iter().map(|(r, _)| *r).max().unwrap();
            let min_col = cells.iter().map(|(_, c)| *c).min().unwrap();
            let max_col = cells.iter().map(|(_, c)| *c).max().unwrap();
            ((min_col..=max_col).collect(), (min_row..=max_row).collect())
        } else if let Some((row, col)) = state.selected_cell {
            (vec![col], vec![row])
        } else {
            return None;
        };
        if cols.is_empty() || rows.is_empty() {
            return None;
        }

        // Only with a disjoint multi-cell selection do out-of-set cells render
        // blank; otherwise every cell in the rectangle is included.
        let mask = !state.selected_cells.is_empty();
        let cell = |r: usize, c: usize| -> String {
            if mask && !state.selected_cells.contains(&(r, c)) {
                return String::new();
            }
            md_escape(&table.get(r, c).map(|v| v.to_string()).unwrap_or_default())
        };

        let mut out = String::new();
        // Header.
        let headers: Vec<String> = cols
            .iter()
            .map(|&c| {
                md_escape(
                    table
                        .columns
                        .get(c)
                        .map(|ci| ci.name.as_str())
                        .unwrap_or(""),
                )
            })
            .collect();
        out.push_str("| ");
        out.push_str(&headers.join(" | "));
        out.push_str(" |\n|");
        for _ in &cols {
            out.push_str(" --- |");
        }
        out.push('\n');
        // Body.
        for &r in &rows {
            out.push_str("| ");
            let row_cells: Vec<String> = cols.iter().map(|&c| cell(r, c)).collect();
            out.push_str(&row_cells.join(" | "));
            out.push_str(" |\n");
        }
        Some(out)
    }

    /// Copy the current selection to the OS clipboard as a Markdown table.
    pub(crate) fn do_copy_markdown(&self, ctx: &egui::Context) {
        if let Some(text) = self.copy_selection_as_markdown() {
            ctx.copy_text(text);
        }
    }

    /// Copy the selected cells as a SQL `IN` list. A whole column counts the
    /// rows the current filter shows, not the hidden ones, since those are
    /// what the user is looking at when they pick it.
    pub(crate) fn do_copy_in_list(&mut self, ctx: &egui::Context) {
        let tab = &self.tabs[self.active_tab];
        let state = &tab.table_state;
        let table = &tab.table;
        let cells: Vec<(usize, usize)> = if !state.selected_rows.is_empty() {
            let mut rows: Vec<usize> = state.selected_rows.iter().copied().collect();
            rows.sort();
            rows.iter()
                .flat_map(|&r| (0..table.col_count()).map(move |c| (r, c)))
                .collect()
        } else if !state.selected_cols.is_empty() {
            let mut cols: Vec<usize> = state.selected_cols.iter().copied().collect();
            cols.sort();
            tab.filtered_rows
                .iter()
                .flat_map(|&r| cols.iter().map(move |&c| (r, c)))
                .collect()
        } else if !state.selected_cells.is_empty() {
            let mut cells: Vec<(usize, usize)> = state.selected_cells.iter().copied().collect();
            cells.sort();
            cells
        } else if let Some(cell) = state.selected_cell {
            vec![cell]
        } else {
            return;
        };
        match data::in_list::sql_in_list(cells.iter().filter_map(|&(r, c)| table.get(r, c))) {
            Some(text) => {
                ctx.copy_text(text);
                self.status_message = Some((
                    octa::i18n::t("context_menu.copied_in_list"),
                    std::time::Instant::now(),
                ));
            }
            None => {
                self.status_message = Some((
                    octa::i18n::t("context_menu.copy_in_list_empty"),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    /// Paste tab-separated text into the table at the current selection.
    pub(crate) fn paste_text_into_table(&mut self, text: &str) {
        let parsed_rows: Vec<Vec<&str>> = text
            .lines()
            .map(|line| line.split('\t').collect())
            .collect();
        if parsed_rows.is_empty() {
            return;
        }

        let tab = &mut self.tabs[self.active_tab];
        let (start_row, start_col) = tab.table_state.selected_cell.unwrap_or((0, 0));

        for (ri, row_cells) in parsed_rows.iter().enumerate() {
            let target_row = start_row + ri;
            if target_row >= tab.table.row_count() {
                break;
            }
            for (ci, &cell_text) in row_cells.iter().enumerate() {
                let target_col = start_col + ci;
                if target_col >= tab.table.col_count() {
                    break;
                }
                if let Some(existing) = tab.table.get(target_row, target_col).cloned() {
                    let new_val = data::CellValue::parse_like(&existing, cell_text);
                    tab.table.set(target_row, target_col, new_val);
                }
            }
        }
        tab.filter_dirty = true;
    }

    /// Cut: copy selection then clear the underlying cells.
    pub(crate) fn do_cut(&mut self, ctx: &egui::Context) {
        self.do_copy(ctx);
        let tab = &mut self.tabs[self.active_tab];
        let row_count = tab.table.row_count();
        let col_count = tab.table.col_count();
        let state = &tab.table_state;
        let mut targets: Vec<(usize, usize)> = Vec::new();
        if !state.selected_rows.is_empty() {
            for &row in &state.selected_rows {
                for col in 0..col_count {
                    targets.push((row, col));
                }
            }
        } else if !state.selected_cols.is_empty() {
            for &col in &state.selected_cols {
                for row in 0..row_count {
                    targets.push((row, col));
                }
            }
        } else if !state.selected_cells.is_empty() {
            for &(row, col) in &state.selected_cells {
                targets.push((row, col));
            }
        } else if let Some((row, col)) = state.selected_cell {
            targets.push((row, col));
        }
        for (row, col) in targets {
            tab.table.set(row, col, data::CellValue::Null);
        }
        tab.filter_dirty = true;
    }

    /// Copy the selection to the OS clipboard.
    ///
    /// Every clipboard write goes through egui (`ctx.copy_text`), never a
    /// clipboard library of our own: egui's windowing layer talks Wayland on
    /// Wayland and X11 on X11, and it is the same clipboard every text box
    /// pastes from. A separate `arboard` handle used to write the X11
    /// clipboard even on Wayland, so a copied cell never reached the SQL
    /// editor.
    pub(crate) fn do_copy(&self, ctx: &egui::Context) {
        if let Some(text) = self.copy_selection_to_string() {
            ctx.copy_text(text);
        }
    }

    /// Paste `text` into the table. `None` (a menu click, a remapped Paste
    /// key) carries no text yet: ask the windowing layer for the clipboard,
    /// which comes back next frame as an ordinary `Event::Paste`.
    pub(crate) fn do_paste(&mut self, ctx: &egui::Context, text: Option<String>) {
        match text {
            Some(text) if !text.is_empty() => self.paste_text_into_table(&text),
            Some(_) => {}
            None => ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste),
        }
    }

    pub(crate) fn apply_zoom(&self, ctx: &egui::Context) {
        let base_font_size = self.settings.font_size;
        let effective_font_size = base_font_size * self.zoom_percent as f32 / 100.0;
        ui::theme::apply_theme(
            ctx,
            self.theme_mode,
            ui::theme::FontSettings {
                size: effective_font_size,
                body: self.settings.body_font,
                custom_path: Some(self.settings.custom_font_path.as_str()),
            },
        );
    }
}

/// Escape a cell value for a Markdown table cell: pipes break the column
/// structure and newlines break the row, so neutralise both.
fn md_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\r', "")
        .replace('\n', "<br>")
}
