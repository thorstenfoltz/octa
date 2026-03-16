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
    has_selected_cell: bool,
) -> ToolbarAction {
    let mut action = ToolbarAction::default();
    let colors = ThemeColors::for_mode(theme_mode);

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
                if ui.button("Insert Row").clicked() {
                    action.add_row = true;
                    ui.close_menu();
                }
                let delete_row_btn =
                    ui.add_enabled(has_selected_cell, egui::Button::new("Delete Row"));
                if delete_row_btn.clicked() {
                    action.delete_row = true;
                    ui.close_menu();
                }

                ui.separator();

                // Column operations
                if ui.button("Add Column...").clicked() {
                    action.add_column = true;
                    ui.close_menu();
                }
                let delete_col_btn =
                    ui.add_enabled(has_selected_cell, egui::Button::new("Delete Column"));
                if delete_col_btn.clicked() {
                    action.delete_column = true;
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

            // Search box (stays in the toolbar directly)
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
