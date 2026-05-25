//! Row-hash-diff renderer for the Compare view. Uses BLAKE3 over the user-
//! picked columns to *match* rows; the UI shows the actual row content so
//! the user can see which records are unique to each side or shared.
//!
//! - **Left only** — rows whose hash is present in left but missing in right.
//! - **Right only** — rows whose hash is present in right but missing in left.
//! - **Shared (in both files)** — hash present in both. Each bucket can hold
//!   multiple physical rows (duplicates within one file collapse to the same
//!   hash); all of them are listed so the user spots the cardinality.
//!
//! Cross-format works because hashing sees only `CellValue::to_string`
//! output — a CSV row and a Parquet row with the same logical content
//! produce identical digests.

use std::collections::HashMap;

use eframe::egui;
use egui::{RichText, ScrollArea};

use octa::data::{CellValue, DataTable};

use super::hash::{hash_row, short_hex};
use crate::app::state::TabState;
use crate::ui;
use ui::theme::ThemeMode;

/// Maximum rows displayed per bucket before showing the "N more not shown"
/// summary. 200 keeps long bucket lists scannable; users can re-narrow the
/// column pick to surface relevant rows.
const BUCKET_DISPLAY_CAP: usize = 200;

pub fn render(ui: &mut egui::Ui, tab: &mut TabState, theme_mode: ThemeMode) {
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

    // Column-pick panel.
    ui.collapsing("Columns to hash", |ui| {
        ui.horizontal_top(|ui| {
            draw_column_picker(ui, "Left", left, &mut tab.compare_columns_left, &colors);
            ui.add_space(16.0);
            draw_column_picker(ui, "Right", right, &mut tab.compare_columns_right, &colors);
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Empty selection means \"hash every column\".\n\
                 Column ordering matters — pick the columns in the same\n\
                 order on both sides for matching semantics.",
            )
            .color(colors.text_muted)
            .size(11.0),
        );
    });
    ui.separator();

    // Build hash → row-index buckets per side. Storing indices (not just
    // counts) lets the result panes show the underlying row content.
    let left_hashes = collect_hash_rows(left, &tab.compare_columns_left);
    let right_hashes = collect_hash_rows(right, &tab.compare_columns_right);

    let mut left_only: Vec<(&[u8; 32], &Vec<usize>)> = Vec::new();
    let mut right_only: Vec<(&[u8; 32], &Vec<usize>)> = Vec::new();
    let mut both: Vec<(&[u8; 32], &Vec<usize>, &Vec<usize>)> = Vec::new();

    for (h, lrows) in &left_hashes {
        match right_hashes.get(h) {
            Some(rrows) => both.push((h, lrows, rrows)),
            None => left_only.push((h, lrows)),
        }
    }
    for (h, rrows) in &right_hashes {
        if !left_hashes.contains_key(h) {
            right_only.push((h, rrows));
        }
    }
    left_only.sort_by_key(|a| short_hex(a.0));
    right_only.sort_by_key(|a| short_hex(a.0));
    both.sort_by_key(|a| short_hex(a.0));

    // Summary line — totals upfront so the user knows the scale before
    // scrolling.
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "Left rows: {}   Right rows: {}   Shared hashes: {}   Left-only: {}   Right-only: {}",
                left.row_count(),
                right.row_count(),
                both.len(),
                left_only.len(),
                right_only.len(),
            ))
            .color(colors.text_secondary)
            .size(12.0),
        );
    });
    ui.add_space(6.0);

    ScrollArea::vertical()
        .id_salt("compare_row_diff_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            draw_single_side_bucket(
                ui,
                "Left only",
                colors.warning,
                left,
                &left_only,
                &tab.compare_columns_left,
                &colors,
            );
            ui.add_space(8.0);
            draw_single_side_bucket(
                ui,
                "Right only",
                colors.accent,
                right,
                &right_only,
                &tab.compare_columns_right,
                &colors,
            );
            ui.add_space(8.0);
            draw_shared_bucket(
                ui,
                left,
                right,
                &both,
                &tab.compare_columns_left,
                &tab.compare_columns_right,
                &colors,
            );
        });
}

/// One hash → all row indices that produced it. Duplicates in one file
/// collapse to the same hash and accumulate into the same Vec.
fn collect_hash_rows(table: &DataTable, cols: &[usize]) -> HashMap<[u8; 32], Vec<usize>> {
    let mut out: HashMap<[u8; 32], Vec<usize>> = HashMap::with_capacity(table.row_count());
    for row_idx in 0..table.row_count() {
        let h = hash_row(table, row_idx, cols);
        out.entry(h).or_default().push(row_idx);
    }
    out
}

fn draw_column_picker(
    ui: &mut egui::Ui,
    label: &str,
    table: &DataTable,
    selected: &mut Vec<usize>,
    colors: &ui::theme::ThemeColors,
) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).strong());
        let total = table.col_count();
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
        if total == 0 {
            ui.label(
                RichText::new("(no columns)")
                    .color(colors.text_muted)
                    .size(11.0),
            );
        }
    });
}

/// Render a Left-only / Right-only bucket. Shows actual row content so the
/// user sees which records are unique. Each bucket entry expands into a
/// per-hash collapsible carrying all rows that produced that hash.
fn draw_single_side_bucket(
    ui: &mut egui::Ui,
    title: &str,
    title_color: egui::Color32,
    table: &DataTable,
    buckets: &[(&[u8; 32], &Vec<usize>)],
    selected_cols: &[usize],
    colors: &ui::theme::ThemeColors,
) {
    let total_rows: usize = buckets.iter().map(|(_, rows)| rows.len()).sum();
    ui.label(
        RichText::new(format!(
            "{}  ({} hashes, {} rows)",
            title,
            buckets.len(),
            total_rows,
        ))
        .strong()
        .color(title_color),
    );
    let display_cols = effective_columns(table, selected_cols);
    if buckets.is_empty() {
        ui.label(RichText::new("(none)").color(colors.text_muted).size(11.0));
        return;
    }
    let mut shown = 0usize;
    for (digest, rows) in buckets {
        // Header collapsible: hash digest + duplicate-count badge.
        let header = format!("{}     × {}", short_hex(digest), rows.len(),);
        egui::CollapsingHeader::new(
            RichText::new(header)
                .font(egui::FontId::new(12.0, egui::FontFamily::Monospace))
                .color(colors.text_primary),
        )
        .id_salt(format!("bucket_single_{title}_{}", short_hex(digest)))
        .default_open(false)
        .show(ui, |ui| {
            draw_rows_inline(ui, table, &display_cols, rows, colors);
        });
        shown += 1;
        if shown >= BUCKET_DISPLAY_CAP {
            break;
        }
    }
    if buckets.len() > BUCKET_DISPLAY_CAP {
        ui.label(
            RichText::new(format!(
                "… {} more hash buckets not shown",
                buckets.len() - BUCKET_DISPLAY_CAP
            ))
            .color(colors.text_muted),
        );
    }
}

/// Render the Shared bucket. Each entry shows the left rows AND the right
/// rows that hashed to the same digest, side by side so the user can spot
/// content divergence in unhashed columns.
fn draw_shared_bucket(
    ui: &mut egui::Ui,
    left: &DataTable,
    right: &DataTable,
    buckets: &[(&[u8; 32], &Vec<usize>, &Vec<usize>)],
    left_cols: &[usize],
    right_cols: &[usize],
    colors: &ui::theme::ThemeColors,
) {
    let total_left: usize = buckets.iter().map(|(_, l, _)| l.len()).sum();
    let total_right: usize = buckets.iter().map(|(_, _, r)| r.len()).sum();
    ui.label(
        RichText::new(format!(
            "Shared (in both files)  ({} hashes, L {} rows, R {} rows)",
            buckets.len(),
            total_left,
            total_right,
        ))
        .strong()
        .color(colors.success),
    );
    if buckets.is_empty() {
        ui.label(RichText::new("(none)").color(colors.text_muted).size(11.0));
        return;
    }
    let left_display_cols = effective_columns(left, left_cols);
    let right_display_cols = effective_columns(right, right_cols);
    let mut shown = 0usize;
    for (digest, lrows, rrows) in buckets {
        let header = format!(
            "{}     L × {}   R × {}",
            short_hex(digest),
            lrows.len(),
            rrows.len(),
        );
        egui::CollapsingHeader::new(
            RichText::new(header)
                .font(egui::FontId::new(12.0, egui::FontFamily::Monospace))
                .color(colors.text_primary),
        )
        .id_salt(format!("bucket_shared_{}", short_hex(digest)))
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Left").color(colors.text_muted).size(11.0));
                    draw_rows_inline(ui, left, &left_display_cols, lrows, colors);
                });
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Right").color(colors.text_muted).size(11.0));
                    draw_rows_inline(ui, right, &right_display_cols, rrows, colors);
                });
            });
        });
        shown += 1;
        if shown >= BUCKET_DISPLAY_CAP {
            break;
        }
    }
    if buckets.len() > BUCKET_DISPLAY_CAP {
        ui.label(
            RichText::new(format!(
                "… {} more hash buckets not shown",
                buckets.len() - BUCKET_DISPLAY_CAP
            ))
            .color(colors.text_muted),
        );
    }
}

/// Resolve which columns to render in the result rows. When the user
/// hasn't picked any columns we'd otherwise show *all* of them, which can
/// be unwieldy — fall back to the first 8 so the table stays readable.
fn effective_columns(table: &DataTable, picked: &[usize]) -> Vec<usize> {
    if !picked.is_empty() {
        return picked
            .iter()
            .copied()
            .filter(|i| *i < table.col_count())
            .collect();
    }
    (0..table.col_count().min(8)).collect()
}

/// Render a small inline grid of row content for the given indices and
/// columns. Used inside the per-hash CollapsingHeader bodies so the user
/// sees actual cell values, not just hex digests.
fn draw_rows_inline(
    ui: &mut egui::Ui,
    table: &DataTable,
    cols: &[usize],
    rows: &[usize],
    colors: &ui::theme::ThemeColors,
) {
    let mono = egui::FontId::new(12.0, egui::FontFamily::Monospace);
    egui::Grid::new(format!(
        "compare_rows_inline_{}_{}",
        rows.first().copied().unwrap_or(usize::MAX),
        rows.len(),
    ))
    .num_columns(cols.len() + 1)
    .spacing([12.0, 2.0])
    .striped(true)
    .show(ui, |ui| {
        // Header row.
        ui.label(
            RichText::new("row")
                .font(mono.clone())
                .color(colors.text_muted),
        );
        for &c in cols {
            let name = table
                .columns
                .get(c)
                .map(|col| col.name.as_str())
                .unwrap_or("?");
            ui.label(
                RichText::new(name)
                    .font(mono.clone())
                    .color(colors.text_muted),
            );
        }
        ui.end_row();
        // Data rows, capped per-bucket so a million-row duplicate set
        // doesn't drown the UI thread.
        for row_idx in rows.iter().take(50) {
            ui.label(
                RichText::new(format!("{}", row_idx + 1 + table.row_offset))
                    .font(mono.clone())
                    .color(colors.text_muted),
            );
            for &c in cols {
                let text = match table.get(*row_idx, c) {
                    Some(CellValue::Null) => "—".to_string(),
                    Some(v) => v.to_string(),
                    None => String::new(),
                };
                ui.label(RichText::new(text).font(mono.clone()));
            }
            ui.end_row();
        }
        if rows.len() > 50 {
            ui.label(
                RichText::new(format!("… {} more rows in this bucket", rows.len() - 50))
                    .color(colors.text_muted),
            );
            ui.end_row();
        }
    });
}
