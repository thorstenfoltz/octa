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
            .show_inside(ui, |ui| {
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
            .show_inside(ui, |ui| {
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

        egui::CentralPanel::default().show_inside(ui, |ui| {
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

    // Write each group, deduplicating sanitised stems.
    let mut stem_counts: HashMap<String, usize> = HashMap::new();
    let mut written = 0usize;

    for (value, group_table) in &groups {
        let base_stem = octa::sql::sanitize_sql_name(value);
        let count = stem_counts.entry(base_stem.clone()).or_insert(0);
        *count += 1;
        let stem = if *count == 1 {
            base_stem
        } else {
            format!("{base_stem}_{count}")
        };
        let out_path = out_dir.join(format!("{stem}.{ext}"));
        out_reader
            .write_file(&out_path, group_table)
            .map_err(|e| format!("Write error for \"{}\": {e}", out_path.display()))?;
        written += 1;
    }

    Ok(format!("Wrote {written} file(s) to {}", out_dir.display()))
}
