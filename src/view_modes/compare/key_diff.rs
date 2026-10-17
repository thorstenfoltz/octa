//! Ordered / Join renderer for the Compare view. Unlike the hash-based
//! `row_diff`, these route through the shared pure logic in
//! `octa::data::compare` (the same code the CLI `--diff` and the MCP
//! `diff_tables` tool use), so the GUI, CLI, and MCP report identical results.
//!
//! - **Ordered** - positional row-by-row cell comparison (`compare_ordered`):
//!   row N on the left versus row N on the right. Trailing rows on the longer
//!   side are reported as added / removed.
//! - **Join** - rows matched by user-picked key column(s) (`compare_join`),
//!   reporting which rows were added, removed, or changed and exactly which
//!   columns differ in each changed pair.
//!
//! Both flatten their `CompareResult` through `build_compare_table` into a
//! single annotated table (leading `status` + `changed_columns` columns) which
//! is rendered as a plain scrollable grid here.

use eframe::egui;
use egui::{RichText, ScrollArea};

use octa::data::compare::{
    CompareOptions, CompareResult, build_compare_table, compare_join, compare_ordered,
};
use octa::data::{CellValue, DataTable};
use octa::i18n::t;

use crate::app::state::TabState;
use crate::ui;
use ui::theme::ThemeMode;

/// Cap on rendered result rows. A diff of two huge tables can produce a vast
/// result; 2000 keeps the grid responsive. The summary line always reports the
/// true totals so the cap never hides the scale.
const RESULT_DISPLAY_CAP: usize = 2000;

/// What the last comparison was computed from. The left table counts as
/// unchanged while its undo, redo and edit counts and its shape stay the
/// same, the key the timeline view uses too.
/// ponytail: an edit undone and replaced by another of the same size between
/// two frames keeps the counts; recomputing every frame is what this cache
/// exists to stop, and a content hash would cost as much as the comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyDiffKey {
    join: bool,
    keys: Vec<String>,
    options: CompareOptions,
    // The right-hand table is not in the key: every site that changes it drops
    // `compare_cache` (`app/file_io/compare.rs`, `central_panel/views.rs`).
    left: Generation,
}

type Generation = (usize, usize, usize, usize, usize, usize);

/// A finished comparison and its flattened table.
pub struct KeyDiffCache {
    key: KeyDiffKey,
    /// egui pass that last used this cache.
    pass: u64,
    outcome: Result<(CompareResult, DataTable), String>,
}

fn generation(t: &DataTable) -> Generation {
    (
        t.undo_stack.len(),
        t.redo_stack.len(),
        t.edits.len(),
        t.row_count(),
        t.col_count(),
        // A replaced table is a new allocation, so this catches every
        // `tab.table = ..` site without each one dropping the cache.
        t.rows.as_ptr() as usize,
    )
}

/// A cache only counts if it was used on the previous pass: while this view is
/// not drawn the table may be freed and a new one reuse its address.
fn reusable(cache: &KeyDiffCache, key: &KeyDiffKey, pass: u64) -> bool {
    cache.key == *key && pass <= cache.pass.saturating_add(1)
}

pub fn render(ui: &mut egui::Ui, tab: &mut TabState, theme_mode: ThemeMode, join: bool) {
    let colors = ui::theme::ThemeColors::for_mode(theme_mode);
    if tab.compare_right_table.is_none() {
        ui.label(RichText::new(t("cmptol.no_table")).color(colors.text_muted));
        return;
    }
    let left_names: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();

    if join {
        // The key picker reuses `compare_columns_left` as the chosen keys.
        ui.collapsing(t("cmptol.key_columns"), |ui| {
            draw_key_picker(ui, &tab.table, &mut tab.compare_columns_left, &colors);
            ui.add_space(4.0);
            ui.label(
                RichText::new(t("cmptol.key_columns_hint"))
                    .color(colors.text_muted)
                    .size(11.0),
            );
        });
    }
    super::tolerance::tolerance_controls(
        ui,
        "compare_tolerance",
        &mut tab.compare_tolerance,
        &left_names,
    );
    ui.separator();

    let key_names: Vec<String> = tab
        .compare_columns_left
        .iter()
        .filter_map(|&i| left_names.get(i).cloned())
        .collect();
    if join && key_names.is_empty() {
        ui.label(RichText::new(t("cmptol.pick_keys")).color(colors.text_muted));
        return;
    }

    let options = tab.compare_tolerance.options(&left_names);
    let key = KeyDiffKey {
        join,
        keys: key_names.clone(),
        options: options.clone(),
        left: generation(&tab.table),
    };
    let pass = ui.ctx().cumulative_pass_nr();
    if !tab
        .compare_cache
        .as_ref()
        .is_some_and(|c| reusable(c, &key, pass))
    {
        let right = tab
            .compare_right_table
            .as_deref()
            .expect("checked at the top");
        let left = &tab.table;
        let outcome = if join {
            compare_join(left, right, &key_names, &options)
        } else {
            Ok(compare_ordered(left, right, &options))
        }
        .map(|r| {
            let table = build_compare_table(left, right, &r);
            (r, table)
        })
        .map_err(|e| format!("{e:#}"));
        tab.compare_cache = Some(KeyDiffCache { key, pass, outcome });
    }
    let Some(cache) = tab.compare_cache.as_mut() else {
        return;
    };
    cache.pass = pass;
    let (result, table) = match &cache.outcome {
        Ok((result, table)) => (result, table),
        Err(err) => {
            ui.label(RichText::new(err).color(colors.warning));
            return;
        }
    };

    // Summary line: the true totals up front.
    octa::ui::control_row::control_line(ui, |ui| {
        ui.label(
            RichText::new(
                t("cmptol.summary")
                    .replace("{changed}", &result.changed.len().to_string())
                    .replace("{unchanged}", &result.unchanged.to_string())
                    .replace("{left}", &result.only_in_a.len().to_string())
                    .replace("{right}", &result.only_in_b.len().to_string()),
            )
            .color(colors.text_secondary)
            .size(12.0),
        );
        if result.within_tolerance > 0 {
            ui.label(
                RichText::new(
                    t("cmptol.within").replace("{n}", &result.within_tolerance.to_string()),
                )
                .color(colors.text_muted)
                .size(12.0),
            );
        }
    });
    ui.add_space(6.0);
    draw_result_table(ui, table, &colors);
}

/// Checkbox list of the left table's columns, used to pick join keys.
fn draw_key_picker(
    ui: &mut egui::Ui,
    table: &DataTable,
    selected: &mut Vec<usize>,
    colors: &ui::theme::ThemeColors,
) {
    let total = table.col_count();
    if total == 0 {
        ui.label(
            RichText::new(t("cmptol.no_columns"))
                .color(colors.text_muted)
                .size(11.0),
        );
        return;
    }
    for col_idx in 0..total {
        let name = &table.columns[col_idx].name;
        let mut picked = selected.contains(&col_idx);
        if ui.checkbox(&mut picked, name).changed() {
            if picked {
                if !selected.contains(&col_idx) {
                    selected.push(col_idx);
                }
            } else {
                selected.retain(|c| *c != col_idx);
            }
        }
    }
}

/// Render the annotated compare table (status + changed_columns + data cols)
/// as a scrollable striped grid, colouring the status cell by kind.
fn draw_result_table(ui: &mut egui::Ui, table: &DataTable, colors: &ui::theme::ThemeColors) {
    if table.row_count() == 0 {
        ui.label(RichText::new(t("cmptol.no_differences")).color(colors.success));
        return;
    }
    let mono = egui::FontId::new(12.0, egui::FontFamily::Monospace);
    let ncols = table.col_count();
    let shown = table.row_count().min(RESULT_DISPLAY_CAP);

    // Build plain-text copy payload: header + shown rows, tab-separated.
    let copy_text = {
        let header: Vec<&str> = (0..ncols)
            .map(|c| {
                table
                    .columns
                    .get(c)
                    .map(|ci| ci.name.as_str())
                    .unwrap_or("?")
            })
            .collect();
        let mut lines = vec![header.join("\t")];
        for r in 0..shown {
            let row: Vec<String> = (0..ncols)
                .map(|c| match table.get(r, c) {
                    Some(CellValue::Null) => "-".to_string(),
                    Some(v) => v.to_string(),
                    None => String::new(),
                })
                .collect();
            lines.push(row.join("\t"));
        }
        lines.join("\n")
    };

    let scroll = ScrollArea::both()
        .id_salt("compare_key_diff_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("compare_key_diff_grid")
                .num_columns(ncols)
                .spacing([12.0, 2.0])
                .striped(true)
                .show(ui, |ui| {
                    for c in 0..ncols {
                        let name = table
                            .columns
                            .get(c)
                            .map(|ci| ci.name.as_str())
                            .unwrap_or("?");
                        ui.label(
                            RichText::new(name)
                                .font(mono.clone())
                                .color(colors.text_muted),
                        );
                    }
                    ui.end_row();
                    for r in 0..shown {
                        for c in 0..ncols {
                            let text = match table.get(r, c) {
                                Some(CellValue::Null) => "-".to_string(),
                                Some(v) => v.to_string(),
                                None => String::new(),
                            };
                            // Colour the leading `status` cell by kind.
                            let color = if c == 0 {
                                status_color(&text, colors)
                            } else {
                                colors.text_primary
                            };
                            ui.label(RichText::new(text).font(mono.clone()).color(color));
                        }
                        ui.end_row();
                    }
                });
            if table.row_count() > RESULT_DISPLAY_CAP {
                ui.label(
                    RichText::new(
                        t("cmptol.more_rows")
                            .replace("{n}", &(table.row_count() - RESULT_DISPLAY_CAP).to_string()),
                    )
                    .color(colors.text_muted),
                );
            }
        });

    let area_resp = ui.interact(
        scroll.inner_rect,
        ui.make_persistent_id("compare_key_diff_menu"),
        egui::Sense::click(),
    );
    area_resp.context_menu(|ui| {
        if ui.button(octa::i18n::t("compare.copy_table")).clicked() {
            ui.ctx().copy_text(copy_text.clone());
            ui.close();
        }
    });
    if area_resp.hovered() && octa::ui::text_selection::copy_pressed(ui.ctx()) {
        ui.ctx().copy_text(copy_text);
    }
}

/// Map a `status` cell value to a colour.
fn status_color(status: &str, colors: &ui::theme::ThemeColors) -> egui::Color32 {
    match status {
        "only_in_a" => colors.warning,
        "only_in_b" => colors.accent,
        "changed_a" | "changed_b" => colors.success,
        _ => colors.text_primary,
    }
}

#[cfg(test)]
#[path = "key_diff_tests.rs"]
mod tests;
