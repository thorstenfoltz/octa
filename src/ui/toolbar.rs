use std::collections::HashSet;

use egui::{RichText, Ui};

use super::theme::{ThemeColors, ThemeMode};
use crate::data::{DataTable, MarkColor, MarkKey, SearchMode, ViewMode};

#[derive(Default)]
pub struct ToolbarAction {
    pub new_file: bool,
    pub open_file: bool,
    pub open_directory: bool,
    pub close_directory: bool,
    pub open_recent: Option<String>,
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
    pub view_mode_changed: Option<ViewMode>,
    pub show_settings: bool,
    pub show_about: bool,
    pub check_for_updates: bool,
    pub replace_next: bool,
    pub replace_all: bool,
    pub toggle_replace_bar: bool,
    pub search_focus: bool,
    pub show_documentation: bool,
    pub exit: bool,
    pub zoom_in: bool,
    pub zoom_out: bool,
    pub zoom_reset: bool,
    pub toggle_sql_panel: bool,
    /// Toggle "first row is header" for the active table.
    pub toggle_first_row_header: bool,
    /// Apply a color mark to a set of keys (cell/row/column).
    pub set_marks: Vec<(MarkKey, MarkColor)>,
    /// Clear color marks from a set of keys.
    pub clear_marks: Vec<MarkKey>,
}

#[allow(clippy::too_many_arguments)]
pub fn draw_toolbar(
    ui: &mut Ui,
    theme_mode: ThemeMode,
    search_text: &mut String,
    search_mode: &mut SearchMode,
    search_focus_requested: bool,
    show_replace_bar: bool,
    replace_text: &mut String,
    has_data: bool,
    has_edits: bool,
    has_source_path: bool,
    selected_cell: Option<(usize, usize)>,
    selected_rows: &HashSet<usize>,
    selected_cols: &HashSet<usize>,
    row_count: usize,
    col_count: usize,
    current_view_mode: ViewMode,
    has_raw_content: bool,
    has_pdf_pages: bool,
    has_markdown: bool,
    has_notebook: bool,
    has_json: bool,
    sql_panel_open: bool,
    zoom_percent: u32,
    logo_texture: Option<&egui::TextureHandle>,
    recent_files: &[String],
    directory_tree_open: bool,
    first_row_is_header: bool,
    table: &DataTable,
) -> ToolbarAction {
    let mut action = ToolbarAction::default();
    let colors = ThemeColors::for_mode(theme_mode);
    let has_selected_cell = selected_cell.is_some();

    ui.horizontal(|ui| {
        ui.add_space(4.0);

        // App logo + title
        if let Some(tex) = logo_texture {
            ui.image(egui::load::SizedTexture::new(tex.id(), [20.0, 20.0]));
        }
        ui.label(
            RichText::new("Octa")
                .strong()
                .size(15.0)
                .color(colors.accent),
        );

        ui.add_space(8.0);

        // --- File menu ---
        ui.menu_button(RichText::new("File").color(colors.text_primary), |ui| {
            ui.set_min_width(180.0);
            if ui.button("New File").clicked() {
                action.new_file = true;
                ui.close_menu();
            }
            if ui.button("Open...").clicked() {
                action.open_file = true;
                ui.close_menu();
            }
            if ui.button("Open Directory...").clicked() {
                action.open_directory = true;
                ui.close_menu();
            }
            if directory_tree_open && ui.button("Close Directory").clicked() {
                action.close_directory = true;
                ui.close_menu();
            }
            if has_data {
                ui.separator();
                if has_source_path && ui.button("Save").clicked() {
                    action.save_file = true;
                    ui.close_menu();
                }
                if ui.button("Save As...").clicked() {
                    action.save_file_as = true;
                    ui.close_menu();
                }
            }
            ui.separator();
            ui.menu_button("Recent Files", |ui| {
                ui.set_min_width(250.0);
                if recent_files.is_empty() {
                    ui.add_enabled(false, egui::Button::new("(none)"));
                } else {
                    for path in recent_files {
                        let filename = std::path::Path::new(path)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.clone());
                        if ui.button(&filename).on_hover_text(path).clicked() {
                            action.open_recent = Some(path.clone());
                            ui.close_menu();
                        }
                    }
                }
            });
            ui.separator();
            if ui.button("Exit").clicked() {
                action.exit = true;
                ui.close_menu();
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

                let can_move_up = selected_cell.is_some_and(|(r, _)| r > 0);
                let can_move_down = selected_cell.is_some_and(|(r, _)| r + 1 < row_count);

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

                let can_move_left = selected_cell.is_some_and(|(_, c)| c > 0);
                let can_move_right = selected_cell.is_some_and(|(_, c)| c + 1 < col_count);

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

                ui.separator();

                // Mark submenu — surfaces the same colors as the right-click
                // context menu, scoped to the current selection.
                let mark_keys: Vec<MarkKey> = if !selected_rows.is_empty() {
                    let mut rs: Vec<usize> = selected_rows.iter().copied().collect();
                    rs.sort();
                    rs.into_iter().map(MarkKey::Row).collect()
                } else if !selected_cols.is_empty() {
                    let mut cs: Vec<usize> = selected_cols.iter().copied().collect();
                    cs.sort();
                    cs.into_iter().map(MarkKey::Column).collect()
                } else if let Some((r, c)) = selected_cell {
                    vec![MarkKey::Cell(r, c)]
                } else {
                    Vec::new()
                };
                let has_marks_keys = !mark_keys.is_empty();
                let any_currently_marked = mark_keys.iter().any(|k| table.marks.contains_key(k));
                ui.add_enabled_ui(has_marks_keys, |ui| {
                    ui.menu_button("Mark", |ui| {
                        for &color in MarkColor::ALL {
                            let swatch = ThemeColors::mark_swatch(color);
                            let label = color.label();
                            let btn = egui::Button::new(RichText::new(label).color(swatch));
                            if ui.add(btn).clicked() {
                                for k in &mark_keys {
                                    action.set_marks.push((k.clone(), color));
                                }
                                ui.close_menu();
                            }
                        }
                        if any_currently_marked {
                            ui.separator();
                            if ui.button("Clear").clicked() {
                                for k in &mark_keys {
                                    action.clear_marks.push(k.clone());
                                }
                                ui.close_menu();
                            }
                        }
                    });
                });

                ui.separator();
                let mut header_flag = first_row_is_header;
                if ui
                    .checkbox(&mut header_flag, "First row is header")
                    .changed()
                {
                    action.toggle_first_row_header = true;
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

            // --- View menu ---
            ui.menu_button(RichText::new("View").color(colors.text_primary), |ui| {
                let is_table = current_view_mode == ViewMode::Table;
                let is_raw = current_view_mode == ViewMode::Raw;
                let is_pdf = current_view_mode == ViewMode::Pdf;

                // Disable table view for notebook files (notebook view is the primary view)
                let table_enabled = !has_notebook;
                let table_btn = ui.add_enabled(
                    table_enabled,
                    egui::RadioButton::new(is_table, "Table View"),
                );
                if table_btn.clicked() {
                    action.view_mode_changed = Some(ViewMode::Table);
                    ui.close_menu();
                }
                let raw_btn =
                    ui.add_enabled(has_raw_content, egui::RadioButton::new(is_raw, "Raw Text"));
                if raw_btn.clicked() {
                    action.view_mode_changed = Some(ViewMode::Raw);
                    ui.close_menu();
                }
                if has_markdown {
                    let is_md = current_view_mode == ViewMode::Markdown;
                    let md_btn = ui.radio(is_md, "Markdown View");
                    if md_btn.clicked() {
                        action.view_mode_changed = Some(ViewMode::Markdown);
                        ui.close_menu();
                    }
                }
                if has_notebook {
                    let is_nb = current_view_mode == ViewMode::Notebook;
                    let nb_btn = ui.radio(is_nb, "Notebook View");
                    if nb_btn.clicked() {
                        action.view_mode_changed = Some(ViewMode::Notebook);
                        ui.close_menu();
                    }
                }
                if has_pdf_pages {
                    let pdf_btn = ui.radio(is_pdf, "PDF View");
                    if pdf_btn.clicked() {
                        action.view_mode_changed = Some(ViewMode::Pdf);
                        ui.close_menu();
                    }
                }
                if has_json {
                    let is_json_tree = current_view_mode == ViewMode::JsonTree;
                    let json_btn = ui.radio(is_json_tree, "JSON Tree");
                    if json_btn.clicked() {
                        action.view_mode_changed = Some(ViewMode::JsonTree);
                        ui.close_menu();
                    }
                }

                ui.separator();
                ui.label(
                    RichText::new("Zoom")
                        .strong()
                        .size(11.0)
                        .color(colors.text_muted),
                );
                ui.horizontal(|ui| {
                    if ui.button("-").clicked() {
                        action.zoom_out = true;
                    }
                    ui.label(format!("{}%", zoom_percent));
                    if ui.button("+").clicked() {
                        action.zoom_in = true;
                    }
                });
                if zoom_percent != 100 && ui.button("Reset (100%)").clicked() {
                    action.zoom_reset = true;
                    ui.close_menu();
                }
            });

            // --- Search menu ---
            ui.menu_button(RichText::new("Search").color(colors.text_primary), |ui| {
                ui.set_min_width(180.0);
                if ui.button("Find").clicked() {
                    action.search_focus = true;
                    ui.close_menu();
                }
                if ui.button("Find & Replace").clicked() {
                    action.toggle_replace_bar = true;
                    ui.close_menu();
                }
            });

            // --- SQL panel toggle (only for tabular data) ---
            // Styled like the menu buttons. Highlights with the accent color
            // while the panel is open.
            if current_view_mode == ViewMode::Table {
                let label_color = if sql_panel_open {
                    colors.accent
                } else {
                    colors.text_primary
                };
                let sql_btn = ui
                    .button(RichText::new("SQL").color(label_color))
                    .on_hover_text("Toggle SQL editor panel");
                if sql_btn.clicked() {
                    action.toggle_sql_panel = true;
                }
            }
        }

        // --- Help menu (always visible, next to Search) ---
        ui.menu_button(RichText::new("Help").color(colors.text_primary), |ui| {
            ui.set_min_width(180.0);
            if ui.button("Documentation...").clicked() {
                action.show_documentation = true;
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Settings...").clicked() {
                action.show_settings = true;
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Check for Updates...").clicked() {
                action.check_for_updates = true;
                ui.close_menu();
            }
            ui.separator();
            if ui.button("About").clicked() {
                action.show_about = true;
                ui.close_menu();
            }
        });

        if has_data {
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // Search box with mode selector
            ui.label(RichText::new("Search:").color(colors.text_secondary));
            let old_mode = *search_mode;
            egui::ComboBox::from_id_salt("search_mode")
                .width(75.0)
                .selected_text(search_mode.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(search_mode, SearchMode::Plain, "Plain");
                    ui.selectable_value(search_mode, SearchMode::Wildcard, "Wildcard");
                    ui.selectable_value(search_mode, SearchMode::Regex, "Regex");
                });
            if *search_mode != old_mode {
                action.search_changed = true;
            }
            let hint = match *search_mode {
                SearchMode::Plain => "Filter rows...",
                SearchMode::Wildcard => "e.g. foo*bar, item?",
                SearchMode::Regex => "e.g. ^\\d{3}-",
            };
            let search_id = ui.id().with("toolbar_search");
            let response = ui.add(
                egui::TextEdit::singleline(search_text)
                    .id(search_id)
                    .desired_width(200.0)
                    .hint_text(hint),
            );
            if response.changed() {
                action.search_changed = true;
            }
            if search_focus_requested {
                response.request_focus();
            }

            if show_replace_bar {
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(RichText::new("Replace:").color(colors.text_secondary));
                ui.add(
                    egui::TextEdit::singleline(replace_text)
                        .desired_width(160.0)
                        .hint_text("Replace with..."),
                );
                let has_search = !search_text.is_empty();
                if ui
                    .add_enabled(has_search, egui::Button::new("Next"))
                    .clicked()
                {
                    action.replace_next = true;
                }
                if ui
                    .add_enabled(has_search, egui::Button::new("All"))
                    .clicked()
                {
                    action.replace_all = true;
                }
            }
        }
    });

    action
}
