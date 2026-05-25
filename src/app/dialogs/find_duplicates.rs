//! Find Duplicates dialog.
//!
//! Pick N key columns + an output mode, hit **Find**, and the dialog
//! either marks every duplicate row orange in place or opens a new
//! tab containing only the duplicates. The dedupe logic lives in
//! `octa::data::duplicates::find_duplicate_rows`; this file is only
//! the picker + dispatch.

use eframe::egui;
use egui::RichText;

use octa::data::duplicates::find_duplicate_rows;
use octa::data::{DataTable, MarkColor, MarkKey};

use super::super::state::{FindDuplicatesMode, OctaApp, TabState};

pub(crate) fn render_find_duplicates_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.tabs[app.active_tab].show_find_duplicates {
        return;
    }

    // Pull a snapshot of the column list and the modal state up front so
    // the inner closure doesn't need to borrow `app` twice.
    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let mut key_cols = app.tabs[app.active_tab].find_duplicates_key_cols.clone();
    let mut mode = app.tabs[app.active_tab].find_duplicates_mode;
    let mut close_requested = false;
    let mut run_requested = false;

    egui::Window::new("Find duplicates")
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .resizable(true)
        .collapsible(false)
        .default_width(420.0)
        .default_height(380.0)
        .min_width(320.0)
        .min_height(220.0)
        .show(ctx, |ui| {
            ui.label(RichText::new("Key columns").strong().size(13.0));
            ui.label(
                RichText::new(
                    "Rows are flagged as duplicates when every checked column \
                     has the same value as at least one other row.",
                )
                .size(10.0)
                .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if ui.small_button("All").clicked() {
                    key_cols = (0..col_names.len()).collect();
                }
                if ui.small_button("None").clicked() {
                    key_cols.clear();
                }
                ui.label(
                    RichText::new(format!("{} selected", key_cols.len()))
                        .size(10.0)
                        .color(ui.visuals().weak_text_color()),
                );
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    for (idx, name) in col_names.iter().enumerate() {
                        let mut on = key_cols.contains(&idx);
                        if ui.checkbox(&mut on, name).changed() {
                            if on {
                                key_cols.insert(idx);
                            } else {
                                key_cols.remove(&idx);
                            }
                        }
                    }
                });

            ui.separator();
            ui.label(
                RichText::new("What to do with duplicates")
                    .strong()
                    .size(13.0),
            );
            ui.radio_value(
                &mut mode,
                FindDuplicatesMode::Highlight,
                "Highlight rows in place (Orange mark)",
            );
            ui.radio_value(
                &mut mode,
                FindDuplicatesMode::NewTab,
                "Open duplicates in a new tab",
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can_run = !key_cols.is_empty();
                let run_btn = ui.add_enabled(can_run, egui::Button::new("Apply"));
                if run_btn.clicked() {
                    run_requested = true;
                }
                if !can_run {
                    ui.label(
                        RichText::new("Select at least one key column")
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cancel").clicked() {
                        close_requested = true;
                    }
                });
            });
        });

    // Stash UI state changes back on the tab regardless of which button
    // closed the dialog.
    {
        let tab = &mut app.tabs[app.active_tab];
        tab.find_duplicates_key_cols = key_cols.clone();
        tab.find_duplicates_mode = mode;
    }

    if close_requested {
        app.tabs[app.active_tab].show_find_duplicates = false;
        return;
    }

    if !run_requested {
        return;
    }

    // --- Execute ---
    let key_cols_vec: Vec<usize> = {
        let mut v: Vec<usize> = key_cols.iter().copied().collect();
        v.sort_unstable();
        v
    };
    let dup_rows: Vec<usize> = {
        let tab = &app.tabs[app.active_tab];
        find_duplicate_rows(&tab.table, &key_cols_vec)
    };

    if dup_rows.is_empty() {
        app.status_message = Some((
            "Find duplicates: no duplicate rows for that key".to_string(),
            std::time::Instant::now(),
        ));
        app.tabs[app.active_tab].show_find_duplicates = false;
        return;
    }

    match mode {
        FindDuplicatesMode::Highlight => {
            let dup_count = dup_rows.len();
            let tab = &mut app.tabs[app.active_tab];
            for row_idx in dup_rows {
                tab.table.set_mark(MarkKey::Row(row_idx), MarkColor::Orange);
            }
            app.status_message = Some((
                format!(
                    "Marked {} duplicate row(s) orange. \
                     Use Edit > Mark > Clear all marks to remove.",
                    dup_count
                ),
                std::time::Instant::now(),
            ));
        }
        FindDuplicatesMode::NewTab => {
            let dup_count = dup_rows.len();
            let key_summary: String = key_cols_vec
                .iter()
                .filter_map(|&c| col_names.get(c))
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            let new_table =
                build_duplicates_table(&app.tabs[app.active_tab].table, &dup_rows, &key_summary);
            // Mirror `apply_loaded_table`'s tab-creation pattern: spawn a
            // fresh tab and activate it.
            let mut new_tab = TabState::new(app.settings.default_search_mode);
            new_tab.table = new_table;
            new_tab.filter_dirty = true;
            if new_tab.table.row_count() > 0 && new_tab.table.col_count() > 0 {
                new_tab.table_state.selected_cell = Some((0, 0));
            }
            app.tabs.push(new_tab);
            app.active_tab = app.tabs.len() - 1;
            app.status_message = Some((
                format!(
                    "Opened {} duplicate row(s) in a new tab (key: {})",
                    dup_count, key_summary
                ),
                std::time::Instant::now(),
            ));
        }
    }

    app.tabs[app.active_tab].show_find_duplicates = false;
}

/// Clone the columns + the chosen rows out of `src` into a fresh
/// `DataTable`. The new table has no source path so Save prompts for
/// one — same convention as the Parse-in-new-tab and Smart-Paste
/// flows. The format-name string carries a hint about how the tab was
/// produced so the title is informative.
fn build_duplicates_table(src: &DataTable, rows: &[usize], key_summary: &str) -> DataTable {
    let mut copy = DataTable {
        columns: src.columns.clone(),
        rows: Vec::with_capacity(rows.len()),
        edits: std::collections::HashMap::new(),
        source_path: None,
        format_name: Some(format!("Duplicates by {}", key_summary)),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    };
    for &row_idx in rows {
        if let Some(row) = src.rows.get(row_idx) {
            copy.rows.push(row.clone());
        }
    }
    copy
}
