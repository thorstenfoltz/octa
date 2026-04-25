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
    pub(crate) fn do_cut(&mut self) {
        self.do_copy();
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

    /// Copy selection to both internal and OS clipboard.
    pub(crate) fn do_copy(&mut self) {
        if let Some(text) = self.copy_selection_to_string() {
            self.tabs[self.active_tab].table_state.clipboard = Some(text.clone());
            if let Some(ref cb) = self.os_clipboard {
                if let Ok(mut cb) = cb.lock() {
                    let _ = cb.set_text(&text);
                }
            }
        }
    }

    /// Paste from OS clipboard (preferred) or internal clipboard.
    pub(crate) fn do_paste(&mut self, paste_event_text: Option<String>) {
        let text = if let Some(t) = paste_event_text {
            Some(t)
        } else if let Some(ref cb) = self.os_clipboard {
            cb.lock().ok().and_then(|mut cb| cb.get_text().ok())
        } else {
            self.tabs[self.active_tab].table_state.clipboard.clone()
        };

        if let Some(text) = text {
            if !text.is_empty() {
                self.paste_text_into_table(&text);
            }
        }
    }

    /// Check if the OS clipboard has text content.
    pub(crate) fn os_clipboard_has_text(&self) -> bool {
        if let Some(ref cb) = self.os_clipboard {
            if let Ok(mut cb) = cb.lock() {
                return cb.get_text().map(|t| !t.is_empty()).unwrap_or(false);
            }
        }
        false
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
