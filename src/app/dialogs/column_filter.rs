//! Excel-style per-column value-set filter dialog.
//!
//! Renders when `tab.show_column_filter` is true. The user picks a column,
//! the dialog computes its unique cell values, and a scrollable checkbox
//! list controls which values pass the filter. Filters AND with each other
//! and with the toolbar text-search via `recompute_filter`.

use std::collections::{BTreeSet, HashSet};

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{DialogSize, draw_window_controls};

use super::super::state::OctaApp;

/// Cap on rendered checkbox rows per frame. Columns with more unique values
/// truncate the visible list; the user has to narrow with the search box.
const MAX_VISIBLE_VALUES: usize = 5000;

pub(crate) fn render_column_filter_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.tabs[app.active_tab].show_column_filter {
        return;
    }

    // --- Resolve the picked column. Bail if the table changed under us
    // (e.g. user deleted columns while the dialog was open). ---
    let col_idx = match app.tabs[app.active_tab].column_filter_picker_col {
        Some(c) if c < app.tabs[app.active_tab].table.col_count() => c,
        _ => {
            app.tabs[app.active_tab].show_column_filter = false;
            return;
        }
    };

    // --- Gather unique values for the picked column. BTreeSet sorts
    // lexicographically so the checkbox list is stable across renders. ---
    let unique_values: Vec<String> = {
        let tab = &app.tabs[app.active_tab];
        let mut set: BTreeSet<String> = BTreeSet::new();
        for row in 0..tab.table.row_count() {
            if let Some(v) = tab.table.get(row, col_idx) {
                set.insert(v.to_string());
            }
        }
        set.into_iter().collect()
    };
    let total_unique = unique_values.len();

    // --- Seed an "all checked" draft on first-open when no saved filter
    // exists. Driven by the one-shot `column_filter_needs_seed` flag set
    // by `open_column_filter_dialog` / column-switch. Without the flag,
    // an empty draft would be indistinguishable from a user-cleared
    // "Select none" state and we'd re-seed every frame. ---
    {
        let tab = &mut app.tabs[app.active_tab];
        if tab.column_filter_needs_seed {
            tab.column_filter_draft_allowed = unique_values.iter().cloned().collect();
            tab.column_filter_needs_seed = false;
        }
    }

    // --- Stage local copies of all dialog state. Writing the window body
    // through a closure forces an extended &mut borrow on `app`, so we work
    // on locals and persist back after the closure returns. ---
    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let col_name = col_names[col_idx].clone();
    let mut size = app.tabs[app.active_tab].column_filter_size;
    let mut value_search = std::mem::take(&mut app.tabs[app.active_tab].column_filter_value_search);
    let mut draft: HashSet<String> =
        std::mem::take(&mut app.tabs[app.active_tab].column_filter_draft_allowed);
    let mut close_requested = false;
    let mut apply_requested = false;
    let mut clear_requested = false;
    let mut switch_col: Option<usize> = None;

    let mut window = egui::Window::new("Column Filter")
        .title_bar(false)
        .collapsible(false);
    window = match size {
        DialogSize::Maximized => window.fixed_rect(ctx.content_rect().shrink(8.0)),
        DialogSize::Minimized => window.resizable(false),
        DialogSize::Normal => window
            .resizable(true)
            .default_width(460.0)
            .default_height(540.0)
            .min_width(320.0)
            .min_height(240.0),
    };
    let minimized = size == DialogSize::Minimized;

    window.show(ctx, |ui| {
        egui::Panel::top("column_filter_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Column Filter").strong().size(16.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close_requested = true;
                        }
                    });
                });
            });

        if minimized {
            return;
        }

        egui::Panel::bottom("column_filter_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Clear filter on this column").clicked() {
                        clear_requested = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Apply").clicked() {
                            apply_requested = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_requested = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show_inside(ui, |ui| {
                // Column picker.
                ui.horizontal(|ui| {
                    ui.label("Column:");
                    egui::ComboBox::from_id_salt("column_filter_combo")
                        .selected_text(&col_name)
                        .show_ui(ui, |ui| {
                            for (c, name) in col_names.iter().enumerate() {
                                let is_current = c == col_idx;
                                if ui.selectable_label(is_current, name).clicked() && !is_current {
                                    switch_col = Some(c);
                                }
                            }
                        });
                });

                ui.separator();

                // Value-list type-filter.
                ui.horizontal(|ui| {
                    ui.label("Find:");
                    ui.add(
                        egui::TextEdit::singleline(&mut value_search)
                            .hint_text("(filter the list below)"),
                    );
                });
                let needle = value_search.to_lowercase();
                let matches_search = |v: &String| -> bool {
                    needle.is_empty() || v.to_lowercase().contains(&needle)
                };
                let visible: Vec<&String> = unique_values
                    .iter()
                    .filter(|v| matches_search(v))
                    .take(MAX_VISIBLE_VALUES)
                    .collect();
                let total_matching = unique_values.iter().filter(|v| matches_search(v)).count();
                let hidden = total_matching.saturating_sub(visible.len());

                ui.horizontal(|ui| {
                    if ui.small_button("Select all").clicked() {
                        for v in &visible {
                            draft.insert((*v).clone());
                        }
                    }
                    if ui.small_button("Select none").clicked() {
                        for v in &visible {
                            draft.remove(*v);
                        }
                    }
                    let checked = unique_values.iter().filter(|v| draft.contains(*v)).count();
                    ui.label(
                        RichText::new(format!("{} of {} checked", checked, total_unique))
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                });

                ui.separator();

                // Scrollable checkbox list.
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for v in &visible {
                            let mut checked = draft.contains(*v);
                            let display = if v.is_empty() {
                                "(empty)".to_string()
                            } else {
                                (*v).clone()
                            };
                            if ui.checkbox(&mut checked, display).changed() {
                                if checked {
                                    draft.insert((*v).clone());
                                } else {
                                    draft.remove(*v);
                                }
                            }
                        }
                        if hidden > 0 {
                            ui.label(
                                RichText::new(format!(
                                    "({} more values not shown - narrow with the search box)",
                                    hidden
                                ))
                                .size(10.0)
                                .color(ui.visuals().weak_text_color()),
                            );
                        }
                    });
            });
    });

    // --- Persist back. The order of branches matters: apply/clear/switch
    // mutate `column_filters`; close discards; the fallthrough just keeps
    // the intermediate dialog state alive for the next frame. ---
    let tab = &mut app.tabs[app.active_tab];
    tab.column_filter_size = size;

    if apply_requested {
        // "All checked" = no filter active (every value passes, equivalent
        // to no filter). "None checked" = filter that allows no values =
        // zero visible rows, which is what the user asked for via Select
        // none. Anything in between is a partial filter.
        if draft.len() == total_unique {
            tab.column_filters.remove(&col_idx);
        } else {
            tab.column_filters.insert(col_idx, draft);
        }
        tab.column_filter_value_search.clear();
        tab.column_filter_draft_allowed.clear();
        tab.filter_dirty = true;
        tab.show_column_filter = false;
    } else if clear_requested {
        tab.column_filters.remove(&col_idx);
        tab.column_filter_value_search.clear();
        tab.column_filter_draft_allowed.clear();
        tab.filter_dirty = true;
        tab.show_column_filter = false;
    } else if close_requested {
        tab.column_filter_value_search.clear();
        tab.column_filter_draft_allowed.clear();
        tab.show_column_filter = false;
    } else if let Some(next) = switch_col {
        // Commit the current column's draft before swapping so in-progress
        // edits aren't lost.
        if draft.len() == total_unique {
            tab.column_filters.remove(&col_idx);
        } else {
            tab.column_filters.insert(col_idx, draft);
        }
        tab.filter_dirty = true;
        tab.column_filter_picker_col = Some(next);
        tab.column_filter_value_search.clear();
        // Seed the next column's draft from any saved filter; if none, arm
        // the seed flag so the next frame re-seeds with "all checked".
        match tab.column_filters.get(&next) {
            Some(set) => {
                tab.column_filter_draft_allowed = set.clone();
                tab.column_filter_needs_seed = false;
            }
            None => {
                tab.column_filter_draft_allowed.clear();
                tab.column_filter_needs_seed = true;
            }
        }
    } else {
        // Steady state: keep intermediate draft alive for the next frame.
        tab.column_filter_value_search = value_search;
        tab.column_filter_draft_allowed = draft;
    }
}
