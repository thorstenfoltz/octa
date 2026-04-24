//! "Delete Columns" modal dialog: checkbox list with All/None buttons.

use eframe::egui;

use super::super::state::OctaApp;

pub(crate) fn render_delete_columns_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.tabs[app.active_tab].show_delete_columns_dialog {
        return;
    }
    let mut open = true;
    let mut should_delete = false;
    // Keep the selection vec in sync when the table shape changes while the
    // dialog is open (rare, but possible via SQL mutations).
    let tab = &mut app.tabs[app.active_tab];
    if tab.delete_col_selection.len() != tab.table.col_count() {
        tab.delete_col_selection = vec![false; tab.table.col_count()];
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

            let tab = &mut app.tabs[app.active_tab];
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (idx, col) in tab.table.columns.iter().enumerate() {
                        let mut checked = tab.delete_col_selection[idx];
                        let label = format!("{} [{}]", col.name, col.data_type);
                        if ui.checkbox(&mut checked, label).changed() {
                            tab.delete_col_selection[idx] = checked;
                        }
                    }
                });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.small_button("All").clicked() {
                    for v in &mut tab.delete_col_selection {
                        *v = true;
                    }
                }
                if ui.small_button("None").clicked() {
                    for v in &mut tab.delete_col_selection {
                        *v = false;
                    }
                }
            });

            let selected_count = tab.delete_col_selection.iter().filter(|&&v| v).count();
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
                    tab.show_delete_columns_dialog = false;
                }
            });
        });

    if should_delete {
        let tab = &mut app.tabs[app.active_tab];
        // Delete in reverse order to keep indices valid
        let to_delete: Vec<usize> = tab
            .delete_col_selection
            .iter()
            .enumerate()
            .filter_map(|(i, &sel)| if sel { Some(i) } else { None })
            .rev()
            .collect();

        for col_idx in to_delete {
            tab.table.delete_column(col_idx);
        }

        tab.table_state.editing_cell = None;
        if tab.table.col_count() == 0 {
            tab.table_state.selected_cell = None;
        } else if let Some((row, col)) = tab.table_state.selected_cell {
            let new_col = col.min(tab.table.col_count() - 1);
            tab.table_state.selected_cell = Some((row, new_col));
        }
        tab.table_state.widths_initialized = false;
        tab.filter_dirty = true;
        tab.show_delete_columns_dialog = false;
    }

    if !open {
        app.tabs[app.active_tab].show_delete_columns_dialog = false;
    }
}
