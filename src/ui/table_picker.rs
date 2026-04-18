use std::path::PathBuf;

use eframe::egui;

use crate::formats::TableInfo;

/// Modal state for picking which table to open from a multi-table source
/// (DuckDB / SQLite). When `Some`, the main app renders a blocking dialog.
pub struct TablePickerState {
    pub path: PathBuf,
    pub format_name: String,
    pub tables: Vec<TableInfo>,
    pub selected: usize,
}

/// What the user did with the picker on this frame.
#[derive(Debug, Clone)]
pub enum TablePickerAction {
    /// Still showing — leave state untouched.
    None,
    /// User confirmed; load `(path, table_name)`.
    Open(PathBuf, String),
    /// User cancelled.
    Cancel,
}

/// Render the modal picker. Returns the user's action for this frame.
pub fn render_table_picker(ctx: &egui::Context, state: &mut TablePickerState) -> TablePickerAction {
    let mut action = TablePickerAction::None;
    let mut open_flag = true;

    egui::Window::new(format!("Open table — {}", state.format_name))
        .collapsible(false)
        .resizable(true)
        .default_width(560.0)
        .default_height(380.0)
        .open(&mut open_flag)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!(
                "{} contains {} table{}. Pick one to open:",
                state
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| state.path.display().to_string()),
                state.tables.len(),
                if state.tables.len() == 1 { "" } else { "s" },
            ));
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(180.0, ui.available_height() - 40.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("table_picker_list")
                            .show(ui, |ui| {
                                for (idx, t) in state.tables.iter().enumerate() {
                                    let label = match t.row_count {
                                        Some(n) => format!("{}  ({})", t.name, n),
                                        None => t.name.clone(),
                                    };
                                    if ui.selectable_label(state.selected == idx, label).clicked() {
                                        state.selected = idx;
                                    }
                                }
                            });
                    },
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height() - 40.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        if let Some(t) = state.tables.get(state.selected) {
                            ui.heading(&t.name);
                            ui.add_space(4.0);
                            ui.label(format!("{} columns", t.columns.len()));
                            ui.add_space(4.0);
                            egui::ScrollArea::vertical()
                                .id_salt("table_picker_schema")
                                .show(ui, |ui| {
                                    egui::Grid::new("schema_grid")
                                        .striped(true)
                                        .spacing(egui::vec2(12.0, 4.0))
                                        .show(ui, |ui| {
                                            ui.strong("Column");
                                            ui.strong("Type");
                                            ui.end_row();
                                            for col in &t.columns {
                                                ui.label(&col.name);
                                                ui.label(&col.data_type);
                                                ui.end_row();
                                            }
                                        });
                                });
                        }
                    },
                );
            });

            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    action = TablePickerAction::Cancel;
                }
                let can_open = state.selected < state.tables.len();
                let open_resp = ui.add_enabled(can_open, egui::Button::new("Open table"));
                if open_resp.clicked() && can_open {
                    let name = state.tables[state.selected].name.clone();
                    action = TablePickerAction::Open(state.path.clone(), name);
                }
            });
        });

    if !open_flag {
        action = TablePickerAction::Cancel;
    }
    action
}
