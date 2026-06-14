//! Interactive repair prompt for a malformed delimited file. Raised by
//! `load_file` only when the opt-in `offer_repair_on_malformed` setting is on
//! and `csv_reader::analyze_delimited` flagged problems. Lists the detected
//! issues, previews the repaired result, and lets the user repair, open the
//! file without repair, or cancel.

use eframe::egui;
use egui::RichText;

use super::super::state::OctaApp;

pub(crate) fn render_repair_file_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(repair) = app.pending_file_repair.as_ref() else {
        return;
    };
    let file_name = repair
        .path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| repair.path.display().to_string());
    let issues = repair.issues.clone();
    let preview = repair.preview.clone();
    let has_ragged = issues
        .iter()
        .any(|i| i.contains("inconsistent column counts"));
    let mut preserve_ragged = repair.options.preserve_ragged;
    let initial_preserve = preserve_ragged;

    let mut do_repair = false;
    let mut open_as_is = false;
    let mut cancel = false;

    egui::Window::new(octa::i18n::t("dialog.repair_title"))
        .resizable(true)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(
                RichText::new(format!(
                    "\"{file_name}\" {}",
                    octa::i18n::t("dialog.repair_subtitle")
                ))
                .strong(),
            );
            ui.add_space(4.0);
            ui.label(octa::i18n::t("dialog.repair_detected"));
            for issue in &issues {
                ui.label(format!("  - {issue}"));
            }
            ui.add_space(8.0);

            if has_ragged {
                ui.checkbox(
                    &mut preserve_ragged,
                    octa::i18n::t("dialog.repair_keep_extra"),
                );
                ui.add_space(8.0);
            }

            if !preview.is_empty() {
                ui.label(RichText::new(octa::i18n::t("dialog.repair_preview")).strong());
                ui.add_space(2.0);
                egui::ScrollArea::horizontal()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        let cols = preview.iter().map(|r| r.len()).max().unwrap_or(0);
                        egui::Grid::new("repair_preview_grid")
                            .striped(true)
                            .show(ui, |ui| {
                                for (ri, row) in preview.iter().enumerate() {
                                    for ci in 0..cols {
                                        let cell = row.get(ci).map(String::as_str).unwrap_or("");
                                        let truncated: String = cell.chars().take(40).collect();
                                        if ri == 0 {
                                            ui.label(RichText::new(truncated).strong());
                                        } else {
                                            ui.label(truncated);
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                ui.add_space(8.0);
            }

            ui.horizontal(|ui| {
                if ui.button(octa::i18n::t("dialog.repair_and_open")).clicked() {
                    do_repair = true;
                }
                if ui
                    .button(octa::i18n::t("dialog.repair_open_as_is"))
                    .clicked()
                {
                    open_as_is = true;
                }
                if ui.button(octa::i18n::t("common.cancel")).clicked() {
                    cancel = true;
                }
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(octa::i18n::t("dialog.repair_footer"))
                    .weak()
                    .size(11.0),
            );
        });

    // Reflect a toggle of "keep extra values" back onto the pending repair and
    // regenerate the preview so the shown table matches the chosen option.
    if preserve_ragged != initial_preserve {
        if let Some(r) = app.pending_file_repair.as_mut() {
            r.options.preserve_ragged = preserve_ragged;
        }
        app.refresh_repair_preview();
    }

    if do_repair {
        app.resolve_file_repair(true);
    } else if open_as_is {
        app.resolve_file_repair(false);
    } else if cancel {
        app.pending_file_repair = None;
    }
}
