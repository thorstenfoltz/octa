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

use octa::data::compare::{CompareResult, build_compare_table, compare_join, compare_ordered};
use octa::data::{CellValue, DataTable};

use crate::app::state::TabState;
use crate::ui;
use ui::theme::ThemeMode;

/// Cap on rendered result rows. A diff of two huge tables can produce a vast
/// result; 2000 keeps the grid responsive. The summary line always reports the
/// true totals so the cap never hides the scale.
const RESULT_DISPLAY_CAP: usize = 2000;

pub fn render(ui: &mut egui::Ui, tab: &mut TabState, theme_mode: ThemeMode, join: bool) {
    let colors = ui::theme::ThemeColors::for_mode(theme_mode);
    let Some(ref right_box) = tab.compare_right_table else {
        ui.label(
            RichText::new(
                "Right side has no tabular data. Switch to Text Diff or\n\
                 re-open the compare with a tabular format on the right.",
            )
            .color(colors.text_muted),
        );
        return;
    };
    let left = &tab.table;
    let right: &DataTable = right_box.as_ref();

    // Compute the result. Join needs key columns; on a missing key column or an
    // empty selection it returns an error / prompt instead of a table.
    let result: Result<CompareResult, String> = if join {
        // Key-column picker reuses `compare_columns_left` as the chosen keys.
        ui.collapsing("Key columns", |ui| {
            draw_key_picker(ui, left, &mut tab.compare_columns_left, &colors);
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Rows are matched by these column(s) on both sides.\n\
                     The same column names must exist on the right side too.",
                )
                .color(colors.text_muted)
                .size(11.0),
            );
        });
        ui.separator();

        let key_names: Vec<String> = tab
            .compare_columns_left
            .iter()
            .filter_map(|&i| left.columns.get(i).map(|c| c.name.clone()))
            .collect();
        if key_names.is_empty() {
            ui.label(
                RichText::new("Pick at least one key column above to run the join.")
                    .color(colors.text_muted),
            );
            return;
        }
        compare_join(left, right, &key_names).map_err(|e| e.to_string())
    } else {
        Ok(compare_ordered(left, right))
    };

    let result = match result {
        Ok(r) => r,
        Err(err) => {
            ui.label(RichText::new(format!("⚠ {err}")).color(colors.warning));
            return;
        }
    };

    // Summary line - true totals upfront.
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "Changed: {}   Unchanged: {}   Only in left: {}   Only in right: {}",
                result.changed.len(),
                result.unchanged,
                result.only_in_a.len(),
                result.only_in_b.len(),
            ))
            .color(colors.text_secondary)
            .size(12.0),
        );
    });
    ui.add_space(6.0);

    // Flatten to the shared annotated table and render it as a grid.
    let table = build_compare_table(left, right, &result);
    draw_result_table(ui, &table, &colors);
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
            RichText::new("(no columns)")
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
        ui.label(RichText::new("No differences - the tables match.").color(colors.success));
        return;
    }
    let mono = egui::FontId::new(12.0, egui::FontFamily::Monospace);
    let ncols = table.col_count();
    let shown = table.row_count().min(RESULT_DISPLAY_CAP);

    ScrollArea::both()
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
                    RichText::new(format!(
                        "... {} more rows not shown",
                        table.row_count() - RESULT_DISPLAY_CAP
                    ))
                    .color(colors.text_muted),
                );
            }
        });
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
