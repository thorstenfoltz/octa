//! Partition-by-column dialog (Analyse -> Partition by column...).
//!
//! The user picks a column of the active tab, an output directory, and an
//! optional format override. Apply writes one file per distinct value of that
//! column into the directory and reports how many files were written.
//!
//! File-writing logic mirrors `src/cli/partition.rs` exactly; sanitised stems
//! are deduplicated with `_2`, `_3`, ... suffixes on collision.

use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{OctaApp, PartitionState};

pub(crate) fn render_partition_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.partition_dialog.is_none() {
        return;
    }

    // Need an active tab with at least one column.
    if app.tabs.is_empty() || app.tabs[app.active_tab].table.col_count() == 0 {
        app.partition_dialog = None;
        return;
    }

    let mut close = false;
    let mut apply = false;
    let mut st = app.partition_dialog.take().unwrap();

    // Clamp column index in case the table changed while the dialog was open.
    let col_count = app.tabs[app.active_tab].table.col_count();
    if st.col >= col_count {
        st.col = 0;
    }

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_partition_dialog");
    let window = egui::Window::new("octa_partition")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(420.0)
            .default_height(280.0)
            .min_width(320.0)
            .min_height(220.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("partition_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("partition.title"))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close = true;
                        }
                    });
                });
            });

        if minimized {
            return;
        }

        egui::Panel::bottom("partition_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("partition.apply")).clicked() {
                        apply = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("partition.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            let tab = &app.tabs[app.active_tab];
            let col_names: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();

            // --- Column picker ---
            ui.label(RichText::new(octa::i18n::t("partition.column_label")).strong());
            let selected_name = col_names.get(st.col).cloned().unwrap_or_default();
            egui::ComboBox::from_id_salt(egui::Id::new("partition_col"))
                .selected_text(selected_name.as_str())
                .show_ui(ui, |ui| {
                    for (i, name) in col_names.iter().enumerate() {
                        ui.selectable_value(&mut st.col, i, name.as_str());
                    }
                });

            ui.add_space(8.0);

            // --- Folder picker ---
            ui.label(RichText::new(octa::i18n::t("partition.folder_label")).strong());
            ui.horizontal(|ui| {
                if ui
                    .button(octa::i18n::t("partition.choose_folder"))
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    st.out_dir = Some(path);
                }
                let dir_label = st
                    .out_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| octa::i18n::t("partition.no_folder"));
                ui.label(dir_label);
            });

            ui.add_space(8.0);

            // --- Layout ---
            // Each option carries its own tooltip: "Flat" and "Hive" mean
            // nothing without knowing that only one of them can be reopened
            // as a table.
            ui.label(RichText::new(octa::i18n::t("partition.layout_label")).strong())
                .on_hover_text(octa::i18n::t("partition.layout_hint"));
            // One list of shapes rather than two independent switches, and a
            // live preview underneath: the surest description of a naming
            // scheme is the names themselves, on this user's own values.
            let ext = preview_ext(&tab.table, &st.format);
            for layout in octa::data::partition::PartitionLayout::ALL {
                ui.radio_value(&mut st.layout, *layout, octa::i18n::t(layout.i18n_key()))
                    .on_hover_text(octa::i18n::t(layout.hint_key()));
            }

            ui.add_space(6.0);
            let examples = layout_examples(&tab.table, st.col, &selected_name, st.layout, &ext);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_width(360.0);
                ui.label(
                    RichText::new(octa::i18n::t("partition.preview_label"))
                        .size(11.0)
                        .color(ui.visuals().weak_text_color()),
                );
                if examples.is_empty() {
                    ui.label(
                        RichText::new(octa::i18n::t("partition.preview_empty"))
                            .size(11.0)
                            .italics()
                            .color(ui.visuals().weak_text_color()),
                    );
                } else {
                    let root = st
                        .out_dir
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| octa::i18n::t("partition.preview_root"));
                    for line in &examples {
                        ui.label(
                            RichText::new(format!("{root}/{line}"))
                                .monospace()
                                .size(11.0),
                        );
                    }
                    if examples.len() < tab.table.row_count() {
                        ui.label(
                            RichText::new(octa::i18n::t("partition.preview_more"))
                                .size(10.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                }
            });

            ui.add_space(8.0);

            // --- Format override ---
            ui.label(RichText::new(octa::i18n::t("partition.format_label")).strong());
            ui.text_edit_singleline(&mut st.format)
                .on_hover_text(octa::i18n::t("partition.format_hint"));

            // --- Inline error ---
            if let Some(err) = &st.error {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(err)
                        .color(ui.visuals().error_fg_color)
                        .size(11.0),
                );
            }
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if apply {
        match apply_partition(app, &st) {
            Ok(msg) => {
                app.status_message = Some((msg, std::time::Instant::now()));
                // Success: drop the dialog.
                return;
            }
            Err(e) => st.error = Some(e),
        }
    }
    if !close {
        app.partition_dialog = Some(st);
    }
}

/// Execute the partition and write files. Returns a success message on `Ok`.
fn apply_partition(app: &mut OctaApp, st: &PartitionState) -> Result<String, String> {
    // Require an output directory.
    let out_dir = st
        .out_dir
        .as_ref()
        .ok_or_else(|| octa::i18n::t("partition.need_folder"))?;

    // Snapshot the active tab (apply pending cell edits).
    let mut snap = app.tabs[app.active_tab].table.clone();
    snap.apply_edits();

    // Determine output extension.
    let ext = if !st.format.is_empty() {
        st.format.trim_start_matches('.').to_string()
    } else {
        snap.source_path
            .as_ref()
            .and_then(|p| {
                std::path::Path::new(p)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| octa::i18n::t("partition.need_format"))?
    };

    // Create output directory.
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("Could not create output directory: {e}"))?;

    // Check the format is writable before doing any work.
    let dummy_path = PathBuf::from(format!("_check_.{ext}"));
    let registry = octa::formats::FormatRegistry::new();
    let out_reader = registry
        .reader_for_path(&dummy_path)
        .ok_or_else(|| format!("No writer available for extension \".{ext}\""))?;
    if !out_reader.supports_write() {
        return Err(format!(
            "Format {} does not support writing; pick a different extension.",
            out_reader.name()
        ));
    }

    // Split the table.
    let groups = octa::data::partition::partition_table(&snap, st.col);

    // Write each group. `partition_path` decides every name, so the dialog's
    // preview and the files on disk cannot drift apart. Flat stems can still
    // collide after sanitising (New York and new-york both give new_york), so
    // that layout alone needs the `_2` counter.
    let mut stem_counts: HashMap<String, usize> = HashMap::new();
    let mut written = 0usize;
    let col_name = snap
        .columns
        .get(st.col)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "value".to_string());

    for (index, (value, group_table)) in groups.iter().enumerate() {
        let rel =
            octa::data::partition::partition_path(st.layout, &col_name, value, &ext, index + 1);
        let rel = octa::data::partition::dedupe_flat_name(st.layout, rel, &ext, &mut stem_counts);
        let out_path = out_dir.join(&rel);
        if let Some(parent) = out_path.parent()
            && parent != out_dir
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Could not create \"{}\": {e}", parent.display()))?;
        }
        out_reader
            .write_file(&out_path, group_table)
            .map_err(|e| format!("Write error for \"{}\": {e}", out_path.display()))?;
        written += 1;
    }

    Ok(format!("Wrote {written} file(s) to {}", out_dir.display()))
}

/// Distinct values of `col`, in row order, stopping at `want`.
///
/// ponytail: bounded scan rather than a cached result. It runs per frame to
/// build a tooltip, so it reads at most `SCAN_ROWS` rows and stops as soon as
/// it has enough - cheap enough to need no invalidation logic, and the values
/// are real ones from the table either way.
fn sample_values(table: &octa::data::DataTable, col: usize, want: usize) -> Vec<String> {
    const SCAN_ROWS: usize = 2_000;
    let mut seen: Vec<String> = Vec::with_capacity(want);
    for row in 0..table.row_count().min(SCAN_ROWS) {
        let v = table
            .get(row, col)
            .map(|c| c.to_string())
            .unwrap_or_default();
        if v.is_empty() || seen.contains(&v) {
            continue;
        }
        seen.push(v);
        if seen.len() == want {
            break;
        }
    }
    seen
}

/// What the chosen layout would actually name the first few outputs.
///
/// Built with the same two functions the write path uses (`hive_dir_name` and
/// `sanitize_sql_name`), so the tooltip cannot promise a name the writer would
/// not produce. Empty when the table has no values to show, and the caller
/// then falls back to the generic wording.
fn layout_examples(
    table: &octa::data::DataTable,
    col: usize,
    col_name: &str,
    layout: octa::data::partition::PartitionLayout,
    ext: &str,
) -> Vec<String> {
    let values = sample_values(table, col, 3);
    let mut seen = std::collections::HashMap::new();
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let rel = octa::data::partition::partition_path(layout, col_name, v, ext, i + 1);
            octa::data::partition::dedupe_flat_name(layout, rel, ext, &mut seen)
        })
        .collect()
}

/// The extension the outputs would get: the override if set, else the source
/// file's own. Mirrors `apply_partition`, which does the same thing before
/// writing anything.
fn preview_ext(table: &octa::data::DataTable, format_override: &str) -> String {
    if !format_override.is_empty() {
        return format_override.trim_start_matches('.').to_string();
    }
    table
        .source_path
        .as_ref()
        .and_then(|p| {
            std::path::Path::new(p)
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "csv".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use octa::data::partition::PartitionLayout;
    use octa::data::{CellValue, ColumnInfo, DataTable};

    fn table(values: &[&str]) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "city".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "n".into(),
                data_type: "Int64".into(),
            },
        ];
        t.rows = values
            .iter()
            .enumerate()
            .map(|(i, v)| vec![CellValue::String((*v).into()), CellValue::Int(i as i64)])
            .collect();
        t
    }

    /// The preview is only useful if it is the user's own data, and short
    /// enough to read at a glance.
    #[test]
    fn preview_takes_up_to_three_real_values_skipping_repeats() {
        let t = table(&["Berlin", "Berlin", "Hamburg", "Munich", "Koeln"]);
        assert_eq!(
            layout_examples(&t, 0, "city", PartitionLayout::Flat, "csv"),
            vec!["berlin.csv", "hamburg.csv", "munich.csv"]
        );
    }

    /// Naming itself is `partition_path`'s job and is tested there; what
    /// matters here is that the preview calls it rather than reimplementing
    /// it, so the two cannot drift.
    #[test]
    fn preview_matches_the_shared_path_builder() {
        let t = table(&["New York", "Berlin"]);
        for layout in PartitionLayout::ALL {
            let shown = layout_examples(&t, 0, "city", *layout, "parquet");
            let expected: Vec<String> = ["New York", "Berlin"]
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    octa::data::partition::partition_path(*layout, "city", v, "parquet", i + 1)
                })
                .collect();
            assert_eq!(shown, expected, "{layout:?}");
        }
    }

    /// The counter that disambiguates colliding flat stems has to be visible
    /// in the preview too, or the preview promises names the writer will not
    /// produce.
    #[test]
    fn preview_shows_the_flat_collision_counter() {
        let t = table(&["New York", "new-york", "NEW_YORK"]);
        assert_eq!(
            layout_examples(&t, 0, "city", PartitionLayout::Flat, "csv"),
            vec!["new_york.csv", "new_york_2.csv", "new_york_3.csv"]
        );
    }

    #[test]
    fn no_values_means_no_preview_and_the_dialog_says_so() {
        assert!(layout_examples(&table(&[]), 0, "city", PartitionLayout::Flat, "csv").is_empty());
        let blanks = table(&["", ""]);
        assert!(layout_examples(&blanks, 0, "city", PartitionLayout::Flat, "csv").is_empty());
    }

    #[test]
    fn preview_ext_prefers_the_override_then_the_source() {
        let mut t = table(&["a"]);
        t.source_path = Some("/tmp/sales.parquet".into());
        assert_eq!(preview_ext(&t, ""), "parquet");
        assert_eq!(preview_ext(&t, ".tsv"), "tsv");
        assert_eq!(preview_ext(&t, "json"), "json");
        t.source_path = None;
        assert_eq!(
            preview_ext(&t, ""),
            "csv",
            "a tab with no file still shows something"
        );
    }
}
