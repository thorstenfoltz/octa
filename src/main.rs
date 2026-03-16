mod data;
mod formats;
mod ui;

use data::DataTable;
use formats::FormatRegistry;
use ui::table_view::TableViewState;
use ui::theme::ThemeMode;

use eframe::egui;

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
            // Set initial theme
            ui::theme::apply_theme(&cc.egui_ctx, ThemeMode::Dark);
            Ok(Box::new(RustyViewerApp::new()))
        }),
    )
}

struct RustyViewerApp {
    /// The loaded data table
    table: DataTable,
    /// Format registry for file I/O
    registry: FormatRegistry,
    /// Current theme mode
    theme_mode: ThemeMode,
    /// Table view state (selection, editing, col widths)
    table_state: TableViewState,
    /// Search / filter text
    search_text: String,
    /// Indices of rows matching the current filter
    filtered_rows: Vec<usize>,
    /// Whether the filter needs recomputation
    filter_dirty: bool,
    /// Status message (e.g., errors)
    status_message: Option<(String, std::time::Instant)>,
    /// Whether the "Add Column" dialog is open
    show_add_column_dialog: bool,
    /// New column name buffer
    new_col_name: String,
    /// New column type selection
    new_col_type: String,
}

const COLUMN_TYPES: &[&str] = &[
    "Utf8",
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
            new_col_type: "Utf8".to_string(),
        }
    }

    fn open_file(&mut self) {
        let mut dialog = rfd::FileDialog::new();
        // Add per-format filters first (e.g. "Parquet (*.parquet)")
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
        // Set default filename from source path
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
                // Apply edits to get a clean table for writing
                self.table.apply_edits();
                match reader.write_file(&path, &self.table) {
                    Ok(()) => {
                        self.table.source_path = Some(path.to_string_lossy().to_string());
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
}

impl eframe::App for RustyViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Recompute filter if needed
        if self.filter_dirty {
            self.recompute_filter();
        }

        // Keyboard shortcuts
        ctx.input(|i| {
            if i.key_pressed(egui::Key::O) && i.modifiers.command {
                // Will be handled below since we can't borrow mutably here
            }
        });

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
                    self.table.has_edits(),
                    self.table.source_path.is_some(),
                    self.table_state.selected_cell.is_some(),
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
                if action.add_row {
                    // Insert after the selected row, or at the end if nothing selected
                    let insert_at = match self.table_state.selected_cell {
                        Some((row, _)) => row + 1,
                        None => self.table.row_count(),
                    };
                    self.table.insert_row(insert_at);
                    // Select the new row
                    let sel_col = self.table_state.selected_cell.map(|(_, c)| c).unwrap_or(0);
                    self.table_state.selected_cell = Some((insert_at, sel_col));
                    self.table_state.editing_cell = None;
                    self.filter_dirty = true;
                }
                if action.delete_row {
                    if let Some((row, col)) = self.table_state.selected_cell {
                        self.table.delete_row(row);
                        self.table_state.editing_cell = None;
                        // Keep selection on the same position, or move up if at end
                        if self.table.row_count() == 0 {
                            self.table_state.selected_cell = None;
                        } else {
                            let new_row = row.min(self.table.row_count() - 1);
                            self.table_state.selected_cell = Some((new_row, col));
                        }
                        self.filter_dirty = true;
                    }
                }
                if action.add_column {
                    self.show_add_column_dialog = true;
                    self.new_col_name.clear();
                    self.new_col_type = "Utf8".to_string();
                }
                if action.delete_column {
                    if let Some((_, col)) = self.table_state.selected_cell {
                        self.table.delete_column(col);
                        self.table_state.selected_cell = None;
                        self.table_state.editing_cell = None;
                        self.table_state.widths_initialized = false;
                        self.filter_dirty = true;
                    }
                }
                if action.discard_edits {
                    self.table.discard_edits();
                }
            });

        // Add Column dialog
        if self.show_add_column_dialog {
            let mut open = true;
            let mut should_add = false;
            egui::Window::new("Add Column")
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
                self.table
                    .add_column(self.new_col_name.clone(), self.new_col_type.clone());
                self.table_state.widths_initialized = false;
                self.filter_dirty = true;
                self.show_add_column_dialog = false;
            }
            if !open {
                self.show_add_column_dialog = false;
            }
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
            // Show error message if any
            if let Some((ref msg, instant)) = self.status_message {
                if instant.elapsed().as_secs() < 10 {
                    let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(msg).color(colors.error).size(12.0));
                    });
                    ui.add_space(4.0);
                }
            }

            // Recompute filter before drawing (in case it was dirtied by toolbar actions)
            if self.filter_dirty {
                self.recompute_filter();
            }

            let filtered = self.filtered_rows.clone();
            ui::table_view::draw_table(
                ui,
                &mut self.table,
                &mut self.table_state,
                self.theme_mode,
                &filtered,
            );
        });
    }
}
