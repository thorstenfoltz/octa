//! Multi-select Excel sheet picker. Shown when a workbook has more sheets
//! than `excel_max_auto_sheets`. The user ticks which sheets to open; each
//! checked sheet loads into its own tab. The first N are pre-checked but the
//! user may pick any number (including all).

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;

pub(crate) fn render_sheet_picker_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.pending_sheet_picker.is_none() {
        return;
    }

    let mut confirm = false;
    let mut close = false;

    let dialog_id = egui::Id::new("octa_sheet_picker_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;

    let window = egui::Window::new("octa_sheet_picker")
        .title_bar(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true).default_width(360.0).min_width(320.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("sheet_picker_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.sheets_title"))
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

        let picker = app.pending_sheet_picker.as_mut().unwrap();
        let count = picker.selected.iter().filter(|&&v| v).count();

        egui::Panel::bottom("sheet_picker_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let open_btn = ui.add_enabled(
                        count > 0,
                        egui::Button::new(format!(
                            "{} ({count})",
                            octa::i18n::t("dialog.open_selected_sheets")
                        )),
                    );
                    if open_btn.clicked() {
                        confirm = true;
                    }
                    if ui.button(octa::i18n::t("common.cancel")).clicked() {
                        close = true;
                    }
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let file_label = picker
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            ui.label(format!(
                "{} - {}",
                file_label,
                octa::i18n::t("dialog.sheets_prompt")
            ));
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                if ui
                    .small_button(octa::i18n::t("dialog.select_all"))
                    .clicked()
                {
                    for v in &mut picker.selected {
                        *v = true;
                    }
                }
                if ui
                    .small_button(octa::i18n::t("dialog.select_none"))
                    .clicked()
                {
                    for v in &mut picker.selected {
                        *v = false;
                    }
                }
            });
            ui.add_space(4.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (idx, name) in picker.sheet_names.iter().enumerate() {
                    let mut checked = picker.selected[idx];
                    if ui.checkbox(&mut checked, name).changed() {
                        picker.selected[idx] = checked;
                    }
                }
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if close || confirm {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if confirm {
        if let Some(picker) = app.pending_sheet_picker.take() {
            let path = picker.path.clone();
            let chosen: Vec<String> = picker
                .sheet_names
                .iter()
                .zip(picker.selected.iter())
                .filter(|&(_, &sel)| sel)
                .map(|(name, _)| name.clone())
                .collect();
            for name in chosen {
                app.load_table(path.clone(), name);
            }
        }
    } else if close {
        app.pending_sheet_picker = None;
    }
}
