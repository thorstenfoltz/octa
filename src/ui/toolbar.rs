use egui::{Align, Layout, RichText, Ui};

use super::theme::{ThemeColors, ThemeMode};

pub struct ToolbarAction {
    pub open_file: bool,
    pub save_file: bool,
    pub save_file_as: bool,
    pub toggle_theme: bool,
    pub search_changed: bool,
    pub add_row: bool,
    pub delete_row: bool,
    pub add_column: bool,
    pub delete_column: bool,
    pub move_row_up: bool,
    pub move_row_down: bool,
    pub move_col_left: bool,
    pub move_col_right: bool,
    pub sort_rows_asc_by: Option<usize>,
    pub sort_rows_desc_by: Option<usize>,
    pub discard_edits: bool,
}

impl Default for ToolbarAction {
    fn default() -> Self {
        Self {
            open_file: false,
            save_file: false,
            save_file_as: false,
            toggle_theme: false,
            search_changed: false,
            add_row: false,
            delete_row: false,
            add_column: false,
            delete_column: false,
            move_row_up: false,
            move_row_down: false,
            move_col_left: false,
            move_col_right: false,
            sort_rows_asc_by: None,
            sort_rows_desc_by: None,
            discard_edits: false,
        }
    }
}

pub fn draw_toolbar(
    ui: &mut Ui,
    theme_mode: ThemeMode,
    search_text: &mut String,
    has_data: bool,
    has_edits: bool,
    has_source_path: bool,
    selected_cell: Option<(usize, usize)>,
    row_count: usize,
    col_count: usize,
) -> ToolbarAction {
    let mut action = ToolbarAction::default();
    let colors = ThemeColors::for_mode(theme_mode);
    let has_selected_cell = selected_cell.is_some();

    ui.horizontal(|ui| {
        ui.add_space(4.0);

        // App title
        ui.label(
            RichText::new("Rusty Viewer")
                .strong()
                .size(15.0)
                .color(colors.accent),
        );

        ui.add_space(8.0);

        // --- File menu ---
        ui.menu_button(RichText::new("File").color(colors.text_primary), |ui| {
            if ui.button("Open...").clicked() {
                action.open_file = true;
                ui.close_menu();
            }
            if has_data {
                ui.separator();
                if has_source_path {
                    if ui.button("Save").clicked() {
                        action.save_file = true;
                        ui.close_menu();
                    }
                }
                if ui.button("Save As...").clicked() {
                    action.save_file_as = true;
                    ui.close_menu();
                }
            }
        });

        // --- Edit menu ---
        if has_data {
            ui.menu_button(RichText::new("Edit").color(colors.text_primary), |ui| {
                // Row operations
                ui.label(
                    RichText::new("Rows")
                        .strong()
                        .size(11.0)
                        .color(colors.text_muted),
                );
                if ui.button("Insert Row").clicked() {
                    action.add_row = true;
                    ui.close_menu();
                }
                let del_row = ui.add_enabled(has_selected_cell, egui::Button::new("Delete Row"));
                if del_row.clicked() {
                    action.delete_row = true;
                    ui.close_menu();
                }

                let can_move_up = selected_cell.map_or(false, |(r, _)| r > 0);
                let can_move_down = selected_cell.map_or(false, |(r, _)| r + 1 < row_count);

                let up_btn = ui.add_enabled(can_move_up, egui::Button::new("Move Row Up"));
                if up_btn.clicked() {
                    action.move_row_up = true;
                    ui.close_menu();
                }
                let down_btn = ui.add_enabled(can_move_down, egui::Button::new("Move Row Down"));
                if down_btn.clicked() {
                    action.move_row_down = true;
                    ui.close_menu();
                }

                ui.separator();

                // Column operations
                ui.label(
                    RichText::new("Columns")
                        .strong()
                        .size(11.0)
                        .color(colors.text_muted),
                );
                if ui.button("Insert Column...").clicked() {
                    action.add_column = true;
                    ui.close_menu();
                }
                let del_col = ui.add_enabled(has_selected_cell, egui::Button::new("Delete Column"));
                if del_col.clicked() {
                    action.delete_column = true;
                    ui.close_menu();
                }

                let can_move_left = selected_cell.map_or(false, |(_, c)| c > 0);
                let can_move_right = selected_cell.map_or(false, |(_, c)| c + 1 < col_count);

                let left_btn = ui.add_enabled(can_move_left, egui::Button::new("Move Column Left"));
                if left_btn.clicked() {
                    action.move_col_left = true;
                    ui.close_menu();
                }
                let right_btn =
                    ui.add_enabled(can_move_right, egui::Button::new("Move Column Right"));
                if right_btn.clicked() {
                    action.move_col_right = true;
                    ui.close_menu();
                }

                ui.separator();
                ui.label(
                    RichText::new("Sort Rows")
                        .strong()
                        .size(11.0)
                        .color(colors.text_muted),
                );
                let can_sort = selected_cell.is_some();
                let sort_asc = ui.add_enabled(can_sort, egui::Button::new("Sort A -> Z"));
                if sort_asc.clicked() {
                    if let Some((_, col)) = selected_cell {
                        action.sort_rows_asc_by = Some(col);
                    }
                    ui.close_menu();
                }
                let sort_desc = ui.add_enabled(can_sort, egui::Button::new("Sort Z -> A"));
                if sort_desc.clicked() {
                    if let Some((_, col)) = selected_cell {
                        action.sort_rows_desc_by = Some(col);
                    }
                    ui.close_menu();
                }

                if has_edits {
                    ui.separator();
                    if ui.button("Discard All Edits").clicked() {
                        action.discard_edits = true;
                        ui.close_menu();
                    }
                }
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // Search box
            ui.label(RichText::new("Search:").color(colors.text_secondary));
            let response = ui.add(
                egui::TextEdit::singleline(search_text)
                    .desired_width(200.0)
                    .hint_text("Filter rows..."),
            );
            if response.changed() {
                action.search_changed = true;
            }
        }

        // Right-aligned: theme toggle
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(4.0);
            let toggle_text = format!(
                "{} {}",
                theme_mode.toggle().icon(),
                theme_mode.toggle().label()
            );
            if ui
                .button(RichText::new(toggle_text).color(colors.text_secondary))
                .clicked()
            {
                action.toggle_theme = true;
            }
        });
    });

    action
}
