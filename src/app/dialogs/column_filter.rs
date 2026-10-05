//! Excel-style per-column value-set filter dialog (**Columns -> Filter by
//! value or shape...**).
//!
//! Renders when `tab.show_column_filter` is true. The user picks any column,
//! the dialog computes its unique cell values, and a scrollable checkbox
//! list controls which values pass the filter. A Values / Shapes switch lists
//! the column's shapes instead (`A-99999`, see `octa::data::shapes`), exactly
//! like the header funnel; ticked shapes are turned into the same value set
//! on Apply, so there is still only one filter. Filters AND with each other
//! and with the toolbar text-search via `recompute_filter`.

use std::collections::{BTreeSet, HashSet};

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

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
    // lexicographically so the checkbox list is stable across renders. A
    // partial database tab lists the server's values instead (the most
    // common `FILTER_WINDOW_TOP_N`, plus the blank value when there are
    // missing cells, as the loaded-row list has one); the window's own
    // search then narrows that list as before. ---
    let on_server = crate::app::db_view::server_conn(
        &app.tabs[app.active_tab],
        app.settings.db_pushdown,
        &app.settings.db_connections,
    )
    .is_some();
    if on_server {
        app.want_values(
            crate::app::db_view::ValuesSlot::Window,
            (col_idx, String::new()),
            crate::app::db_view::FILTER_WINDOW_TOP_N,
            ctx,
        );
    }
    let (unique_values, total_unique, server_loading, server_error, server_more) = {
        let tab = &app.tabs[app.active_tab];
        let server = on_server.then(|| {
            tab.filter_window_values
                .as_ref()
                .filter(|v| v.key.0 == col_idx)
                .and_then(|v| v.result.as_ref())
        });
        match server {
            None => {
                let mut set: BTreeSet<String> = BTreeSet::new();
                for row in 0..tab.table.row_count() {
                    if let Some(v) = tab.table.get(row, col_idx) {
                        set.insert(v.to_string());
                    }
                }
                let v: Vec<String> = set.into_iter().collect();
                let n = v.len();
                (v, n, false, None, None)
            }
            Some(None) => (Vec::new(), 0, true, None, None),
            Some(Some(Err(e))) => (Vec::new(), 0, false, Some(e.clone()), None),
            Some(Some(Ok(vf))) => {
                let mut v: Vec<String> = vf.rows.iter().map(|r| r.label.clone()).collect();
                let mut total = vf.unique_count;
                if vf.nulls > 0 {
                    v.push(String::new());
                    total += 1;
                }
                v.sort();
                let more = (vf.unique_count > vf.rows.len())
                    .then(|| (vf.unique_count - vf.rows.len(), vf.rows.len()));
                (v, total, false, None, more)
            }
        }
    };
    // Without the list, "everything ticked" cannot be told from a filter:
    // Apply (and a column switch) would drop the column's filter.
    let list_ready = !server_loading && server_error.is_none();
    let app_rows_loaded = app.tabs[app.active_tab].table.row_count();

    // --- Shapes mode: the column's shapes, and a shape draft derived from
    // the value draft whenever it is unset (open, column switch, switching
    // into Shapes). ---
    let shapes_mode = app.tabs[app.active_tab].column_filter_shapes_mode;
    let shape_rows: Vec<(String, usize, String)> = if shapes_mode {
        octa::data::shapes::shape_frequency(&app.tabs[app.active_tab].table, col_idx)
            .shapes
            .into_iter()
            .map(|s| (s.shape, s.count, s.example))
            .collect()
    } else {
        Vec::new()
    };

    // --- Seed an "all checked" draft on first-open when no saved filter
    // exists. Driven by the one-shot `column_filter_needs_seed` flag set
    // by `open_column_filter_dialog` / column-switch. Without the flag,
    // an empty draft would be indistinguishable from a user-cleared
    // "Select none" state and we'd re-seed every frame. ---
    {
        let tab = &mut app.tabs[app.active_tab];
        if tab.column_filter_needs_seed && list_ready {
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
    let mut shape_draft: HashSet<String> = if shapes_mode {
        app.tabs[app.active_tab]
            .column_filter_shape_draft
            .take()
            .unwrap_or_else(|| {
                draft
                    .iter()
                    .map(|v| octa::data::shapes::shape_of(v))
                    .collect()
            })
    } else {
        HashSet::new()
    };
    let mut switch_mode: Option<bool> = None;
    let mut close_requested = false;
    let mut apply_requested = false;
    let mut clear_requested = false;
    let mut switch_col: Option<usize> = None;

    let dialog_id = egui::Id::new("octa_column_filter_dialog");
    let build_size = size;
    let window = egui::Window::new("Column Filter")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, build_size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(540.0)
            .min_width(320.0)
            .min_height(240.0)
    });
    let minimized = size == DialogSize::Minimized;

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("column_filter_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("columns_menu.filter_title"))
                            .strong()
                            .size(16.0),
                    );
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
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button(octa::i18n::t("dialog.cf_clear"))
                        .on_hover_text(octa::i18n::t("columns_menu.filter_clear_hint"))
                        .clicked()
                    {
                        clear_requested = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                list_ready,
                                egui::Button::new(octa::i18n::t("common.apply")),
                            )
                            .on_disabled_hover_text(octa::i18n::t(if server_loading {
                                "dbview.facet_counting"
                            } else {
                                "dbview.facet_failed"
                            }))
                            .clicked()
                        {
                            apply_requested = true;
                        }
                        if ui.button(octa::i18n::t("common.cancel")).clicked() {
                            close_requested = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show(ui, |ui| {
                // Column picker.
                ui.horizontal(|ui| {
                    ui.label(octa::i18n::t("dialog.cf_column"));
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

                // Values / Shapes, the same switch as the header funnel.
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(!shapes_mode, octa::i18n::t("facet.mode_values"))
                        .on_hover_text(octa::i18n::t("facet.mode_values_hint"))
                        .clicked()
                        && shapes_mode
                    {
                        switch_mode = Some(false);
                    }
                    if ui
                        .selectable_label(shapes_mode, octa::i18n::t("facet.mode_shapes"))
                        .on_hover_text(octa::i18n::t("facet.mode_shapes_hint"))
                        .clicked()
                        && !shapes_mode
                    {
                        switch_mode = Some(true);
                    }
                });

                ui.separator();

                // Value-list type-filter.
                ui.horizontal(|ui| {
                    ui.label(octa::i18n::t("dialog.cf_find"));
                    ui.add(
                        egui::TextEdit::singleline(&mut value_search)
                            .hint_text(octa::i18n::t("dialog.cf_find_hint")),
                    );
                });
                let needle = value_search.to_lowercase();
                if shapes_mode {
                    if on_server {
                        ui.weak(octa::i18n::t("dbview.facet_shapes_loaded").replace(
                            "{loaded}",
                            &octa::ui::status_bar::format_number(app_rows_loaded),
                        ));
                    }
                    let visible: Vec<&(String, usize, String)> = shape_rows
                        .iter()
                        .filter(|(shape, _, example)| {
                            needle.is_empty()
                                || shape.to_lowercase().contains(&needle)
                                || example.to_lowercase().contains(&needle)
                        })
                        .collect();
                    ui.horizontal(|ui| {
                        if ui
                            .small_button(octa::i18n::t("dialog.select_all"))
                            .clicked()
                        {
                            for (shape, _, _) in &visible {
                                shape_draft.insert(shape.clone());
                            }
                        }
                        if ui
                            .small_button(octa::i18n::t("dialog.select_none"))
                            .clicked()
                        {
                            for (shape, _, _) in &visible {
                                shape_draft.remove(shape);
                            }
                        }
                        let checked = shape_rows
                            .iter()
                            .filter(|(shape, _, _)| shape_draft.contains(shape))
                            .count();
                        ui.label(
                            RichText::new(format!(
                                "{}/{} {}",
                                checked,
                                shape_rows.len(),
                                octa::i18n::t("dialog.cf_checked")
                            ))
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                        );
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (shape, count, example) in &visible {
                                let mut checked = shape_draft.contains(shape);
                                let label = format!(
                                    "{shape}  ({count})  {} {example}",
                                    octa::i18n::t("facet.shape_example")
                                );
                                if ui
                                    .checkbox(&mut checked, label)
                                    .on_hover_text(octa::i18n::t("facet.shape_row_hint"))
                                    .changed()
                                {
                                    if checked {
                                        shape_draft.insert(shape.clone());
                                    } else {
                                        shape_draft.remove(shape);
                                    }
                                }
                            }
                        });
                    return;
                }
                if server_loading {
                    octa::ui::control_row::control_row(ui, |ui| {
                        ui.spinner();
                        ui.weak(octa::i18n::t("dbview.facet_counting"));
                    });
                    return;
                }
                if let Some(e) = &server_error {
                    octa::ui::message::selectable_message(
                        ui,
                        ui.visuals().error_fg_color,
                        &format!("{} {e}", octa::i18n::t("dbview.facet_failed")),
                    );
                    return;
                }
                if let Some((more, shown)) = server_more {
                    ui.weak(
                        octa::i18n::t("dbview.window_more")
                            .replace("{count}", &octa::ui::status_bar::format_number(more))
                            .replace("{shown}", &octa::ui::status_bar::format_number(shown)),
                    );
                }
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
                    if ui
                        .small_button(octa::i18n::t("dialog.select_all"))
                        .clicked()
                    {
                        for v in &visible {
                            draft.insert((*v).clone());
                        }
                    }
                    if ui
                        .small_button(octa::i18n::t("dialog.select_none"))
                        .clicked()
                    {
                        for v in &visible {
                            draft.remove(*v);
                        }
                    }
                    let checked = unique_values.iter().filter(|v| draft.contains(*v)).count();
                    ui.label(
                        RichText::new(format!(
                            "{}/{} {}",
                            checked,
                            total_unique,
                            octa::i18n::t("dialog.cf_checked")
                        ))
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
                                octa::i18n::t("dialog.cf_empty")
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
                                    "({} {})",
                                    hidden,
                                    octa::i18n::t("dialog.cf_more_hidden")
                                ))
                                .size(10.0)
                                .color(ui.visuals().weak_text_color()),
                            );
                        }
                    });
            });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, build_size, inner.response.rect);
    }

    // --- Persist back. The order of branches matters: apply/clear/switch
    // mutate `column_filters`; close discards; the fallthrough just keeps
    // the intermediate dialog state alive for the next frame. ---
    //
    // What the draft means as a filter: `None` = no filter (everything
    // passes). In Shapes mode every shape ticked is no filter; otherwise the
    // ticked shapes become the values that have them, the same set the
    // header funnel writes. "None checked" stays a filter that allows no
    // values, which is what Select none asked for.
    let shapes_all_ticked = shape_rows.iter().all(|(s, _, _)| shape_draft.contains(s));
    let effective = |draft: &HashSet<String>, table: &octa::data::DataTable| {
        if shapes_mode {
            (!shapes_all_ticked)
                .then(|| octa::data::shapes::values_with_shapes(table, col_idx, &shape_draft))
        } else {
            (draft.len() != total_unique).then(|| draft.clone())
        }
    };
    let tab = &mut app.tabs[app.active_tab];
    tab.column_filter_size = size;

    if apply_requested {
        match effective(&draft, &tab.table) {
            None => tab.column_filters.remove(&col_idx),
            Some(allowed) => tab.column_filters.insert(col_idx, allowed),
        };
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
        // edits aren't lost (not before the server's list came: nothing was
        // edited, and the empty draft would read as "no filter").
        if list_ready {
            match effective(&draft, &tab.table) {
                None => tab.column_filters.remove(&col_idx),
                Some(allowed) => tab.column_filters.insert(col_idx, allowed),
            };
            tab.filter_dirty = true;
        }
        tab.column_filter_picker_col = Some(next);
        tab.column_filter_value_search.clear();
        tab.column_filter_shape_draft = None;
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
    } else if let Some(to_shapes) = switch_mode {
        // Carry the ticks across: shapes from the ticked values, or back to
        // the values that have the ticked shapes.
        if !to_shapes {
            draft = match effective(&draft, &tab.table) {
                None => unique_values.iter().cloned().collect(),
                Some(allowed) => allowed,
            };
        }
        tab.column_filter_shapes_mode = to_shapes;
        tab.column_filter_shape_draft = None;
        tab.column_filter_value_search = value_search;
        tab.column_filter_draft_allowed = draft;
    } else {
        // Steady state: keep intermediate drafts alive for the next frame.
        tab.column_filter_value_search = value_search;
        tab.column_filter_draft_allowed = draft;
        if shapes_mode {
            tab.column_filter_shape_draft = Some(shape_draft);
        }
    }
}
