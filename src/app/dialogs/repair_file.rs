//! Interactive repair prompt for a malformed delimited file. Raised by
//! `load_file` only when the opt-in `offer_repair_on_malformed` setting is on
//! and `csv_reader::analyze_delimited` flagged problems. Lists the detected
//! issues, previews the repaired result, and lets the user repair, open the
//! file without repair, or cancel.

use eframe::egui;
use egui::RichText;

use super::super::state::OctaApp;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

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

    let dialog_id = egui::Id::new("octa_repair_file_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(520.0, 400.0));
    let window = egui::Window::new("octa_repair_file")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .default_height(400.0)
            .min_width(360.0)
            .min_height(220.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("repair_file_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(octa::i18n::t("dialog.repair_title"))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            chrome_close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
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
    });
    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if chrome_close {
                DialogSize::Normal
            } else {
                size
            },
        )
    });
    if chrome_close {
        cancel = true;
    }

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
