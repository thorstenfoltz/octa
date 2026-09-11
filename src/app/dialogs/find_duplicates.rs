//! Find Duplicates dialog.
//!
//! Pick N key columns + an output mode, hit **Apply**, and the dialog
//! marks every duplicate row orange in place, opens a new tab containing
//! only the duplicates, filters the tab to (non-)repeats, or drops the
//! repeats keeping the first or last occurrence. The engines live in
//! `octa::data::duplicates::find_duplicate_rows` and
//! `octa::data::dedupe::dedupe_dropped_indices`; this file is only the
//! picker + dispatch.

use eframe::egui;
use egui::RichText;

use octa::data::dedupe::{KeepWhich, dedupe_dropped_indices};
use octa::data::duplicates::find_duplicate_rows;
use octa::data::{DataTable, MarkColor, MarkKey};
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

use super::super::state::{DuplicateFilter, FindDuplicatesMode, OctaApp, TabState};

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
    let is_large = app.tabs[app.active_tab].large.is_some();
    let readonly = app.is_readonly();
    // A mode picked on an ordinary tab must not stay selected when the dialog
    // is next opened on a large or read-only one, or Apply would silently do
    // nothing.
    if (is_large
        && matches!(
            mode,
            FindDuplicatesMode::FilterDuplicates | FindDuplicatesMode::FilterUnique
        ))
        || (readonly && matches!(mode, FindDuplicatesMode::Drop(_)))
    {
        mode = FindDuplicatesMode::Highlight;
    }
    let mut close_requested = false;
    let mut run_requested = false;

    let dialog_id = egui::Id::new("octa_find_duplicates_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(420.0, 380.0));
    let window = egui::Window::new("octa_find_duplicates")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(420.0)
            .default_height(380.0)
            .min_width(320.0)
            .min_height(220.0)
            .default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("find_duplicates_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.fd_title"))
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

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("dialog.fd_key_columns"))
                    .strong()
                    .size(13.0),
            );
            ui.label(
                RichText::new(octa::i18n::t("dialog.fd_key_desc"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if ui.small_button(octa::i18n::t("dialog.sel_all")).clicked() {
                    key_cols = (0..col_names.len()).collect();
                }
                if ui.small_button(octa::i18n::t("dialog.sel_none")).clicked() {
                    key_cols.clear();
                }
                ui.label(
                    RichText::new(format!(
                        "{} {}",
                        key_cols.len(),
                        octa::i18n::t("dialog.selected")
                    ))
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
                RichText::new(octa::i18n::t("dialog.fd_what_to_do"))
                    .strong()
                    .size(13.0),
            );
            ui.radio_value(
                &mut mode,
                FindDuplicatesMode::Highlight,
                octa::i18n::t("dialog.fd_highlight"),
            );
            ui.radio_value(
                &mut mode,
                FindDuplicatesMode::NewTab,
                octa::i18n::t("dialog.fd_new_tab"),
            );
            // A large-file tab filters against the file in SQL and never
            // builds the row vector these two narrow, so they cannot work
            // there. Greyed with the reason rather than hidden.
            ui.add_enabled_ui(!is_large, |ui| {
                ui.radio_value(
                    &mut mode,
                    FindDuplicatesMode::FilterDuplicates,
                    octa::i18n::t("dialog.fd_filter_dups"),
                )
                .on_hover_text(octa::i18n::t("dialog.fd_filter_dups_hint"))
                .on_disabled_hover_text(octa::i18n::t("dialog.fd_filter_large"));
                ui.radio_value(
                    &mut mode,
                    FindDuplicatesMode::FilterUnique,
                    octa::i18n::t("dialog.fd_filter_unique"),
                )
                .on_hover_text(octa::i18n::t("dialog.fd_filter_unique_hint"))
                .on_disabled_hover_text(octa::i18n::t("dialog.fd_filter_large"));
            });
            // Drop is the one mode that edits the table, so it follows the
            // read-only chokepoint like every other edit path.
            ui.add_enabled_ui(!readonly, |ui| {
                let dropping = matches!(mode, FindDuplicatesMode::Drop(_));
                if ui
                    .radio(dropping, octa::i18n::t("dedupe.title"))
                    .on_hover_text(octa::i18n::t("dedupe.menu_hint"))
                    .on_disabled_hover_text(octa::i18n::t("transform.readonly"))
                    .clicked()
                    && !dropping
                {
                    mode = FindDuplicatesMode::Drop(KeepWhich::First);
                }
                if dropping {
                    ui.indent("fd_keep", |ui| {
                        ui.label(octa::i18n::t("dedupe.keep_label"));
                        ui.radio_value(
                            &mut mode,
                            FindDuplicatesMode::Drop(KeepWhich::First),
                            octa::i18n::t("dedupe.keep_first"),
                        );
                        ui.radio_value(
                            &mut mode,
                            FindDuplicatesMode::Drop(KeepWhich::Last),
                            octa::i18n::t("dedupe.keep_last"),
                        );
                    });
                }
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can_run = !key_cols.is_empty();
                let run_btn =
                    ui.add_enabled(can_run, egui::Button::new(octa::i18n::t("common.apply")));
                if run_btn.clicked() {
                    run_requested = true;
                }
                if !can_run {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.fd_select_one"))
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(octa::i18n::t("common.cancel")).clicked() {
                        close_requested = true;
                    }
                });
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if close_requested || run_requested {
                DialogSize::Normal
            } else {
                size
            },
        )
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
            octa::i18n::t("dialog.fd_no_dups"),
            std::time::Instant::now(),
        ));
        app.tabs[app.active_tab].show_find_duplicates = false;
        return;
    }

    match mode {
        FindDuplicatesMode::Highlight => {
            let dup_count = dup_rows.len();
            let tab = &mut app.tabs[app.active_tab];
            // Marking is a replace, not an accumulate: without this, a second
            // run on different key columns leaves the first run's orange rows
            // behind and the colour stops meaning "duplicate". Goes through
            // `clear_mark` so the whole run is one undo step.
            let stale: Vec<MarkKey> = tab
                .table
                .marks
                .iter()
                .filter(|(k, c)| matches!(k, MarkKey::Row(_)) && **c == MarkColor::Orange)
                .map(|(k, _)| k.clone())
                .collect();
            for key in stale {
                tab.table.clear_mark(key);
            }
            for row_idx in dup_rows {
                tab.table.set_mark(MarkKey::Row(row_idx), MarkColor::Orange);
            }
            app.status_message = Some((
                format!(
                    "{} {} {} {}",
                    octa::i18n::t("dialog.fd_marked"),
                    dup_count,
                    octa::i18n::t("dialog.fd_dup_rows_orange"),
                    octa::i18n::t("dialog.fd_marked_suffix")
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
                    "{} {} {} ({}: {})",
                    octa::i18n::t("dialog.fd_opened"),
                    dup_count,
                    octa::i18n::t("dialog.fd_in_new_tab"),
                    octa::i18n::t("dialog.fd_key_label"),
                    key_summary
                ),
                std::time::Instant::now(),
            ));
        }
        FindDuplicatesMode::FilterDuplicates | FindDuplicatesMode::FilterUnique => {
            let keep_duplicates = mode == FindDuplicatesMode::FilterDuplicates;
            let dup_count = dup_rows.len();
            let tab = &mut app.tabs[app.active_tab];
            let kept = if keep_duplicates {
                dup_count
            } else {
                tab.table.row_count().saturating_sub(dup_count)
            };
            tab.duplicate_filter = Some(DuplicateFilter {
                key_cols: key_cols_vec.clone(),
                keep_duplicates,
            });
            tab.duplicate_filter_cache = None;
            tab.filter_dirty = true;
            app.status_message = Some((
                octa::i18n::t(if keep_duplicates {
                    "dialog.fd_filtered_dups"
                } else {
                    "dialog.fd_filtered_unique"
                })
                .replace("{n}", &kept.to_string()),
                std::time::Instant::now(),
            ));
        }
        FindDuplicatesMode::Drop(keep) => drop_duplicates(app, &key_cols_vec, keep),
    }

    app.tabs[app.active_tab].show_find_duplicates = false;
}

/// Delete every repeat on `key_cols` (empty = whole row) except the `keep`
/// occurrence, as one undo step, and report the count in the status bar.
/// Shared with the clean-up panel's "duplicate rows" fix.
pub(crate) fn drop_duplicates(app: &mut OctaApp, key_cols: &[usize], keep: KeepWhich) {
    let active = app.active_tab;

    // Merge pending cell edits so dedupe sees the visible values.
    app.tabs[active].table.apply_edits();

    // The engine tells us exactly which original rows to drop, in descending
    // order. Delete each (highest first, so indices don't shift) and coalesce
    // the per-row DeleteRow undo actions into one Batch, so a single Ctrl+Z
    // restores the full table.
    let dropped = dedupe_dropped_indices(&app.tabs[active].table, key_cols, keep);
    let removed = dropped.len();

    if removed > 0 {
        let undo_start = app.tabs[active].table.undo_stack.len();
        for row_idx in dropped {
            app.tabs[active].table.delete_row(row_idx);
        }
        app.tabs[active].table.coalesce_undo_since(undo_start);
        app.tabs[active].table.structural_changes = true;
        app.tabs[active].filter_dirty = true;
        app.tabs[active].table_state.widths_initialized = false;
    }

    app.status_message = Some((
        format!("{removed} {}", octa::i18n::t("dedupe.removed_status")),
        std::time::Instant::now(),
    ));
}

/// Clone the columns + the chosen rows out of `src` into a fresh
/// `DataTable`. The new table has no source path so Save prompts for
/// one - same convention as the Parse-in-new-tab and Smart-Paste
/// flows. The format-name string carries a hint about how the tab was
/// produced so the title is informative.
fn build_duplicates_table(src: &DataTable, rows: &[usize], key_summary: &str) -> DataTable {
    let mut copy = DataTable {
        columns: src.columns.clone(),
        rows: Vec::with_capacity(rows.len()),
        edits: std::collections::HashMap::new(),
        source_path: None,
        format_name: Some(format!(
            "{} {}",
            octa::i18n::t("dialog.fd_dup_by"),
            key_summary
        )),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
        formulas: std::collections::HashMap::new(),
    };
    for &row_idx in rows {
        if let Some(row) = src.rows.get(row_idx) {
            copy.rows.push(row.clone());
        }
    }
    copy
}
