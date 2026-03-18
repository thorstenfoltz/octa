mod data;
mod formats;
mod ui;

use data::DataTable;
use formats::FormatRegistry;
use ui::table_view::TableViewState;
use ui::theme::ThemeMode;

use eframe::egui;
use egui::RichText;

use std::sync::{Arc, Mutex};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([3840.0, 2160.0])
            .with_min_inner_size([800.0, 600.0])
            .with_maximized(true)
            .with_title("Rusty Viewer"),
        ..Default::default()
    };

    eframe::run_native(
        "Rusty Viewer",
        options,
        Box::new(|cc| {
            ui::theme::apply_theme(&cc.egui_ctx, ThemeMode::Dark);
            Ok(Box::new(RustyViewerApp::new()))
        }),
    )
}

struct RustyViewerApp {
    table: DataTable,
    registry: FormatRegistry,
    theme_mode: ThemeMode,
    table_state: TableViewState,
    search_text: String,
    filtered_rows: Vec<usize>,
    filter_dirty: bool,
    status_message: Option<(String, std::time::Instant)>,
    /// "Add Column" dialog state
    show_add_column_dialog: bool,
    new_col_name: String,
    new_col_type: String,
    /// Column index to insert at (None = append at end)
    insert_col_at: Option<usize>,
    /// "Delete Columns" dialog state
    show_delete_columns_dialog: bool,
    /// Checkbox state per column (true = marked for deletion)
    delete_col_selection: Vec<bool>,
    /// "Unsaved changes" dialog state
    show_close_confirm: bool,
    /// Whether we already decided to quit (skip further confirm)
    confirmed_close: bool,
    /// System clipboard handle (shared, lazily initialized)
    os_clipboard: Option<Arc<Mutex<arboard::Clipboard>>>,
}

const COLUMN_TYPES: &[&str] = &[
    "String",
    "Int64",
    "Float64",
    "Boolean",
    "Date32",
    "Timestamp(Microsecond, None)",
];

impl RustyViewerApp {
    fn new() -> Self {
        Self {
            table: DataTable::empty(),
            registry: FormatRegistry::new(),
            theme_mode: ThemeMode::Dark,
            table_state: TableViewState::default(),
            search_text: String::new(),
            filtered_rows: Vec::new(),
            filter_dirty: true,
            status_message: None,
            show_add_column_dialog: false,
            new_col_name: String::new(),
            new_col_type: "String".to_string(),
            insert_col_at: None,
            show_delete_columns_dialog: false,
            delete_col_selection: Vec::new(),
            show_close_confirm: false,
            confirmed_close: false,
            os_clipboard: arboard::Clipboard::new()
                .ok()
                .map(|c| Arc::new(Mutex::new(c))),
        }
    }

    /// Build a tab-separated string from the current selection.
    /// Priority: selected_rows > selected_cols > selected_cell.
    fn copy_selection_to_string(&self) -> Option<String> {
        let state = &self.table_state;

        if !state.selected_rows.is_empty() {
            // Copy selected rows (all columns)
            let mut rows: Vec<usize> = state.selected_rows.iter().copied().collect();
            rows.sort();
            let mut lines = Vec::new();
            for row in rows {
                let mut cells = Vec::new();
                for col in 0..self.table.col_count() {
                    let text = self
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
            // Copy selected columns (all rows)
            let mut cols: Vec<usize> = state.selected_cols.iter().copied().collect();
            cols.sort();
            let mut lines = Vec::new();
            for row in 0..self.table.row_count() {
                let mut cells = Vec::new();
                for &col in &cols {
                    let text = self
                        .table
                        .get(row, col)
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    cells.push(text);
                }
                lines.push(cells.join("\t"));
            }
            Some(lines.join("\n"))
        } else if let Some((row, col)) = state.selected_cell {
            // Copy single cell
            let text = self
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
    fn paste_text_into_table(&mut self, text: &str) {
        let parsed_rows: Vec<Vec<&str>> = text
            .lines()
            .map(|line| line.split('\t').collect())
            .collect();
        if parsed_rows.is_empty() {
            return;
        }

        let (start_row, start_col) = self.table_state.selected_cell.unwrap_or((0, 0));

        for (ri, row_cells) in parsed_rows.iter().enumerate() {
            let target_row = start_row + ri;
            if target_row >= self.table.row_count() {
                break;
            }
            for (ci, &cell_text) in row_cells.iter().enumerate() {
                let target_col = start_col + ci;
                if target_col >= self.table.col_count() {
                    break;
                }
                if let Some(existing) = self.table.get(target_row, target_col).cloned() {
                    let new_val = data::CellValue::parse_like(&existing, cell_text);
                    self.table.set(target_row, target_col, new_val);
                }
            }
        }
        self.filter_dirty = true;
    }

    /// Copy selection to both internal and OS clipboard.
    fn do_copy(&mut self) {
        if let Some(text) = self.copy_selection_to_string() {
            self.table_state.clipboard = Some(text.clone());
            if let Some(ref cb) = self.os_clipboard {
                if let Ok(mut cb) = cb.lock() {
                    let _ = cb.set_text(&text);
                }
            }
        }
    }

    /// Paste from OS clipboard (preferred) or internal clipboard.
    fn do_paste(&mut self, paste_event_text: Option<String>) {
        // Priority: paste_event_text (from egui Paste event) > OS clipboard > internal clipboard
        let text = if let Some(t) = paste_event_text {
            Some(t)
        } else if let Some(ref cb) = self.os_clipboard {
            cb.lock().ok().and_then(|mut cb| cb.get_text().ok())
        } else {
            self.table_state.clipboard.clone()
        };

        if let Some(text) = text {
            if !text.is_empty() {
                self.paste_text_into_table(&text);
            }
        }
    }

    /// Check if the OS clipboard has text content.
    fn os_clipboard_has_text(&self) -> bool {
        if let Some(ref cb) = self.os_clipboard {
            if let Ok(mut cb) = cb.lock() {
                return cb.get_text().map(|t| !t.is_empty()).unwrap_or(false);
            }
        }
        false
    }

    fn open_file(&mut self) {
        let mut dialog = rfd::FileDialog::new();
        for (name, exts) in self.registry.format_descriptions() {
            let ext_refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter(&name, &ext_refs);
        }

        let file = dialog.pick_file();

        if let Some(path) = file {
            match self.registry.reader_for_path(&path) {
                Some(reader) => match reader.read_file(&path) {
                    Ok(table) => {
                        self.table = table;
                        self.table_state = TableViewState::default();
                        self.search_text.clear();
                        self.filter_dirty = true;
                        self.status_message = None;
                    }
                    Err(e) => {
                        self.status_message = Some((
                            format!("Error reading file: {}", e),
                            std::time::Instant::now(),
                        ));
                    }
                },
                None => {
                    self.status_message = Some((
                        format!(
                            "No reader available for: {}",
                            path.extension()
                                .map(|e| e.to_string_lossy().to_string())
                                .unwrap_or_default()
                        ),
                        std::time::Instant::now(),
                    ));
                }
            }
        }
    }

    fn save_file(&mut self) {
        if let Some(ref path) = self.table.source_path.clone() {
            let path = std::path::Path::new(path);
            self.do_save(path.to_path_buf());
        }
    }

    fn save_file_as(&mut self) {
        let mut dialog = rfd::FileDialog::new();
        for (name, exts) in self.registry.format_descriptions() {
            let ext_refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter(&name, &ext_refs);
        }
        if let Some(ref source) = self.table.source_path {
            if let Some(name) = std::path::Path::new(source).file_name() {
                dialog = dialog.set_file_name(name.to_string_lossy().to_string());
            }
        }

        if let Some(path) = dialog.save_file() {
            self.do_save(path);
        }
    }

    fn do_save(&mut self, path: std::path::PathBuf) {
        match self.registry.reader_for_path(&path) {
            Some(reader) => {
                if !reader.supports_write() {
                    self.status_message = Some((
                        format!("Writing is not supported for {} format", reader.name()),
                        std::time::Instant::now(),
                    ));
                    return;
                }
                self.table.apply_edits();
                match reader.write_file(&path, &self.table) {
                    Ok(()) => {
                        self.table.source_path = Some(path.to_string_lossy().to_string());
                        self.table.clear_modified();
                        self.status_message = Some((
                            format!("Saved to {}", path.display()),
                            std::time::Instant::now(),
                        ));
                    }
                    Err(e) => {
                        self.status_message = Some((
                            format!("Error saving file: {}", e),
                            std::time::Instant::now(),
                        ));
                    }
                }
            }
            None => {
                self.status_message = Some((
                    format!(
                        "No writer available for extension: {}",
                        path.extension()
                            .map(|e| e.to_string_lossy().to_string())
                            .unwrap_or_default()
                    ),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    fn recompute_filter(&mut self) {
        if self.search_text.is_empty() {
            self.filtered_rows = (0..self.table.row_count()).collect();
        } else {
            let query = self.search_text.to_lowercase();
            self.filtered_rows = (0..self.table.row_count())
                .filter(|&row_idx| {
                    (0..self.table.col_count()).any(|col_idx| {
                        self.table
                            .get(row_idx, col_idx)
                            .map(|v| v.to_string().to_lowercase().contains(&query))
                            .unwrap_or(false)
                    })
                })
                .collect();
        }
        self.filter_dirty = false;
    }

    /// Open the "Delete Columns" dialog, initializing checkboxes.
    fn open_delete_columns_dialog(&mut self) {
        self.delete_col_selection = vec![false; self.table.col_count()];
        // Pre-select the currently selected column if any
        if let Some((_, col)) = self.table_state.selected_cell {
            if col < self.delete_col_selection.len() {
                self.delete_col_selection[col] = true;
            }
        }
        self.show_delete_columns_dialog = true;
    }

    /// Sort columns alphabetically by name, ascending or descending.
    #[allow(dead_code)]
    fn sort_columns_alphabetically(&mut self, ascending: bool) {
        let col_count = self.table.col_count();
        if col_count <= 1 {
            return;
        }

        // order[new_pos] = old_pos
        let mut order: Vec<usize> = (0..col_count).collect();
        order.sort_by(|&a, &b| {
            let cmp = self.table.columns[a]
                .name
                .to_lowercase()
                .cmp(&self.table.columns[b].name.to_lowercase());
            if ascending {
                cmp
            } else {
                cmp.reverse()
            }
        });

        // Reorder column widths to match
        let old_widths = self.table_state.col_widths.clone();
        self.table_state.col_widths = order
            .iter()
            .map(|&orig| old_widths.get(orig).copied().unwrap_or(120.0))
            .collect();

        // Update selected cell column: build reverse map
        if let Some((row, col)) = self.table_state.selected_cell {
            if let Some(new_col) = order.iter().position(|&orig| orig == col) {
                self.table_state.selected_cell = Some((row, new_col));
            }
        }

        // Apply the reorder atomically
        self.table.reorder_columns(&order);
        self.filter_dirty = true;
    }
}

impl eframe::App for RustyViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Handle close request ---
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.table.is_modified() && !self.confirmed_close {
                // Block the close and show our dialog
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.show_close_confirm = true;
            }
            // If confirmed_close is true, we just let it close
        }

        // Recompute filter if needed
        if self.filter_dirty {
            self.recompute_filter();
        }

        let search_active = !self.search_text.is_empty();
        let filtered_count = self.filtered_rows.len();

        // Top toolbar
        egui::TopBottomPanel::top("toolbar")
            .exact_height(40.0)
            .show(ctx, |ui| {
                let action = ui::toolbar::draw_toolbar(
                    ui,
                    self.theme_mode,
                    &mut self.search_text,
                    self.table.col_count() > 0,
                    self.table.is_modified(),
                    self.table.source_path.is_some(),
                    self.table_state.selected_cell,
                    self.table.row_count(),
                    self.table.col_count(),
                );

                if action.open_file {
                    self.open_file();
                }
                if action.save_file {
                    self.save_file();
                }
                if action.save_file_as {
                    self.save_file_as();
                }
                if action.toggle_theme {
                    self.theme_mode = self.theme_mode.toggle();
                    ui::theme::apply_theme(ctx, self.theme_mode);
                }
                if action.search_changed {
                    self.filter_dirty = true;
                }

                // --- Row operations ---
                if action.add_row {
                    let insert_at = match self.table_state.selected_cell {
                        Some((row, _)) => row + 1,
                        None => self.table.row_count(),
                    };
                    self.table.insert_row(insert_at);
                    let sel_col = self.table_state.selected_cell.map(|(_, c)| c).unwrap_or(0);
                    self.table_state.selected_cell = Some((insert_at, sel_col));
                    self.table_state.editing_cell = None;
                    self.filter_dirty = true;
                }
                if action.delete_row {
                    if let Some((row, col)) = self.table_state.selected_cell {
                        self.table.delete_row(row);
                        self.table_state.editing_cell = None;
                        if self.table.row_count() == 0 {
                            self.table_state.selected_cell = None;
                        } else {
                            let new_row = row.min(self.table.row_count() - 1);
                            self.table_state.selected_cell = Some((new_row, col));
                        }
                        self.filter_dirty = true;
                    }
                }
                if action.move_row_up {
                    if let Some((row, col)) = self.table_state.selected_cell {
                        if row > 0 {
                            self.table.move_row(row, row - 1);
                            self.table_state.selected_cell = Some((row - 1, col));
                            self.filter_dirty = true;
                        }
                    }
                }
                if action.move_row_down {
                    if let Some((row, col)) = self.table_state.selected_cell {
                        if row + 1 < self.table.row_count() {
                            self.table.move_row(row, row + 1);
                            self.table_state.selected_cell = Some((row + 1, col));
                            self.filter_dirty = true;
                        }
                    }
                }

                // --- Column operations ---
                if action.add_column {
                    self.show_add_column_dialog = true;
                    self.new_col_name.clear();
                    self.new_col_type = "String".to_string();
                    // Insert after selected column, or at end
                    self.insert_col_at = self.table_state.selected_cell.map(|(_, c)| c + 1);
                }
                if action.delete_column {
                    if self.table.col_count() > 0 {
                        self.open_delete_columns_dialog();
                    }
                }
                if action.move_col_left {
                    if let Some((row, col)) = self.table_state.selected_cell {
                        if col > 0 {
                            self.table.move_column(col, col - 1);
                            self.table_state.selected_cell = Some((row, col - 1));
                            self.table_state.widths_initialized = false;
                        }
                    }
                }
                if action.move_col_right {
                    if let Some((row, col)) = self.table_state.selected_cell {
                        if col + 1 < self.table.col_count() {
                            self.table.move_column(col, col + 1);
                            self.table_state.selected_cell = Some((row, col + 1));
                            self.table_state.widths_initialized = false;
                        }
                    }
                }
                if let Some(col_idx) = action.sort_rows_asc_by {
                    self.table.sort_rows_by_column(col_idx, true);
                    self.filter_dirty = true;
                }
                if let Some(col_idx) = action.sort_rows_desc_by {
                    self.table.sort_rows_by_column(col_idx, false);
                    self.filter_dirty = true;
                }

                if action.discard_edits {
                    self.table.discard_edits();
                }
            });

        // --- Add Column dialog ---
        if self.show_add_column_dialog {
            let mut open = true;
            let mut should_add = false;
            egui::Window::new("Insert Column")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.new_col_name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Type:");
                        egui::ComboBox::from_id_salt("col_type_combo")
                            .selected_text(self.new_col_type.as_str())
                            .show_ui(ui, |ui| {
                                for t in COLUMN_TYPES {
                                    ui.selectable_value(&mut self.new_col_type, t.to_string(), *t);
                                }
                            });
                    });
                    ui.add_space(4.0);
                    // Show/edit insert position
                    ui.horizontal(|ui| {
                        ui.label("Insert at position:");
                        let col_count = self.table.col_count();
                        let mut pos_val = self.insert_col_at.unwrap_or(col_count) + 1;
                        let drag = egui::DragValue::new(&mut pos_val)
                            .range(1..=(col_count + 1))
                            .speed(1.0);
                        if ui.add(drag).changed() {
                            self.insert_col_at = Some((pos_val - 1).min(col_count));
                        }
                        ui.label(format!("/ {}", col_count + 1));
                    });
                    ui.label(
                        RichText::new("Tip: click a column header to set insert position")
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Add").clicked() && !self.new_col_name.is_empty() {
                            should_add = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_add_column_dialog = false;
                        }
                    });
                });
            if should_add {
                let idx = self.insert_col_at.unwrap_or(self.table.col_count());
                self.table
                    .insert_column(idx, self.new_col_name.clone(), self.new_col_type.clone());
                // Select the new column
                if let Some((row, _)) = self.table_state.selected_cell {
                    self.table_state.selected_cell = Some((row, idx));
                }
                self.table_state.widths_initialized = false;
                self.filter_dirty = true;
                self.show_add_column_dialog = false;
            }
            if !open {
                self.show_add_column_dialog = false;
            }
        }

        // --- Delete Columns dialog ---
        if self.show_delete_columns_dialog {
            let mut open = true;
            let mut should_delete = false;
            // Make sure selection vec is in sync (table may have changed)
            if self.delete_col_selection.len() != self.table.col_count() {
                self.delete_col_selection = vec![false; self.table.col_count()];
            }
            egui::Window::new("Delete Columns")
                .open(&mut open)
                .resizable(true)
                .collapsible(false)
                .min_width(280.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Select columns to delete:");
                    ui.add_space(6.0);

                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for (idx, col) in self.table.columns.iter().enumerate() {
                                let mut checked = self.delete_col_selection[idx];
                                let label = format!("{} [{}]", col.name, col.data_type);
                                if ui.checkbox(&mut checked, label).changed() {
                                    self.delete_col_selection[idx] = checked;
                                }
                            }
                        });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.small_button("All").clicked() {
                            for v in &mut self.delete_col_selection {
                                *v = true;
                            }
                        }
                        if ui.small_button("None").clicked() {
                            for v in &mut self.delete_col_selection {
                                *v = false;
                            }
                        }
                    });

                    let selected_count = self.delete_col_selection.iter().filter(|&&v| v).count();
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let delete_btn = ui.add_enabled(
                            selected_count > 0,
                            egui::Button::new(format!("Delete ({} selected)", selected_count)),
                        );
                        if delete_btn.clicked() {
                            should_delete = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_delete_columns_dialog = false;
                        }
                    });
                });

            if should_delete {
                // Delete in reverse order to keep indices valid
                let to_delete: Vec<usize> = self
                    .delete_col_selection
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &sel)| if sel { Some(i) } else { None })
                    .rev()
                    .collect();

                for col_idx in to_delete {
                    self.table.delete_column(col_idx);
                }

                self.table_state.editing_cell = None;
                if self.table.col_count() == 0 {
                    self.table_state.selected_cell = None;
                } else if let Some((row, col)) = self.table_state.selected_cell {
                    let new_col = col.min(self.table.col_count() - 1);
                    self.table_state.selected_cell = Some((row, new_col));
                }
                self.table_state.widths_initialized = false;
                self.filter_dirty = true;
                self.show_delete_columns_dialog = false;
            }

            if !open {
                self.show_delete_columns_dialog = false;
            }
        }

        // --- Unsaved changes confirmation dialog ---
        if self.show_close_confirm {
            egui::Window::new("Unsaved Changes")
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("You have unsaved changes. What would you like to do?");
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            self.show_close_confirm = false;
                            if self.table.source_path.is_some() {
                                self.save_file();
                            } else {
                                self.save_file_as();
                            }
                            // After save, close
                            self.confirmed_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Don't Save").clicked() {
                            self.show_close_confirm = false;
                            self.confirmed_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_close_confirm = false;
                        }
                    });
                });
        }

        // Bottom status bar
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(28.0)
            .show(ctx, |ui| {
                ui::status_bar::draw_status_bar(
                    ui,
                    &self.table,
                    &self.table_state,
                    self.theme_mode,
                    filtered_count,
                    search_active,
                );
            });

        // Central panel: table view
        egui::CentralPanel::default().show(ctx, |ui| {
            // Show status message if any
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

            // Recompute filter before drawing (in case it was dirtied by toolbar actions)
            if self.filter_dirty {
                self.recompute_filter();
            }

            let filtered = self.filtered_rows.clone();
            let os_has_clip = self.table_state.clipboard.is_some() || self.os_clipboard_has_text();
            let interaction = ui::table_view::draw_table(
                ui,
                &mut self.table,
                &mut self.table_state,
                self.theme_mode,
                &filtered,
                os_has_clip,
            );

            // Handle column header click: update insert position for "Add Column" dialog
            if let Some(col_idx) = interaction.header_col_clicked {
                self.insert_col_at = Some(col_idx + 1);
                if let Some((row, _)) = self.table_state.selected_cell {
                    self.table_state.selected_cell = Some((row, col_idx));
                }
            }

            // Handle drag-and-drop column move
            if let Some((from, to)) = interaction.col_drag_move {
                self.table.move_column(from, to);
                if let Some((row, col)) = self.table_state.selected_cell {
                    let new_col = if col == from {
                        to
                    } else if from < to {
                        if col > from && col <= to {
                            col - 1
                        } else {
                            col
                        }
                    } else {
                        if col >= to && col < from {
                            col + 1
                        } else {
                            col
                        }
                    };
                    self.table_state.selected_cell = Some((row, new_col));
                }
                if from < self.table_state.col_widths.len()
                    && to < self.table_state.col_widths.len()
                {
                    let w = self.table_state.col_widths.remove(from);
                    self.table_state.col_widths.insert(to, w);
                }
                self.filter_dirty = true;
            }

            // Sort rows by column (from table header arrows or context menu)
            if let Some(col_idx) = interaction.sort_rows_asc_by {
                self.table.sort_rows_by_column(col_idx, true);
                self.filter_dirty = true;
            }
            if let Some(col_idx) = interaction.sort_rows_desc_by {
                self.table.sort_rows_by_column(col_idx, false);
                self.filter_dirty = true;
            }

            // --- Context menu: row operations ---
            if interaction.ctx_insert_row {
                let insert_at = match self.table_state.selected_cell {
                    Some((row, _)) => row + 1,
                    None => self.table.row_count(),
                };
                self.table.insert_row(insert_at);
                let sel_col = self.table_state.selected_cell.map(|(_, c)| c).unwrap_or(0);
                self.table_state.selected_cell = Some((insert_at, sel_col));
                self.table_state.editing_cell = None;
                self.filter_dirty = true;
            }
            if interaction.ctx_delete_row {
                if let Some((row, col)) = self.table_state.selected_cell {
                    self.table.delete_row(row);
                    self.table_state.editing_cell = None;
                    if self.table.row_count() == 0 {
                        self.table_state.selected_cell = None;
                    } else {
                        let new_row = row.min(self.table.row_count() - 1);
                        self.table_state.selected_cell = Some((new_row, col));
                    }
                    self.filter_dirty = true;
                }
            }
            if interaction.ctx_move_row_up {
                if let Some((row, col)) = self.table_state.selected_cell {
                    if row > 0 {
                        self.table.move_row(row, row - 1);
                        self.table_state.selected_cell = Some((row - 1, col));
                        self.filter_dirty = true;
                    }
                }
            }
            if interaction.ctx_move_row_down {
                if let Some((row, col)) = self.table_state.selected_cell {
                    if row + 1 < self.table.row_count() {
                        self.table.move_row(row, row + 1);
                        self.table_state.selected_cell = Some((row + 1, col));
                        self.filter_dirty = true;
                    }
                }
            }

            // --- Context menu: column operations ---
            if interaction.ctx_insert_column {
                self.show_add_column_dialog = true;
                self.new_col_name.clear();
                self.new_col_type = "String".to_string();
                self.insert_col_at = self.table_state.selected_cell.map(|(_, c)| c + 1);
            }
            if interaction.ctx_delete_column {
                if self.table.col_count() > 0 {
                    self.open_delete_columns_dialog();
                }
            }
            if interaction.ctx_move_col_left {
                if let Some((row, col)) = self.table_state.selected_cell {
                    if col > 0 {
                        self.table.move_column(col, col - 1);
                        self.table_state.selected_cell = Some((row, col - 1));
                        self.table_state.widths_initialized = false;
                    }
                }
            }
            if interaction.ctx_move_col_right {
                if let Some((row, col)) = self.table_state.selected_cell {
                    if col + 1 < self.table.col_count() {
                        self.table.move_column(col, col + 1);
                        self.table_state.selected_cell = Some((row, col + 1));
                        self.table_state.widths_initialized = false;
                    }
                }
            }

            // --- Copy / Paste ---
            if interaction.ctx_copy {
                self.do_copy();
            }
            if interaction.ctx_paste {
                self.do_paste(interaction.paste_text);
            }
        });
    }
}
